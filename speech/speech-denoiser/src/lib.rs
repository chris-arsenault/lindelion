//! Speech Denoiser: streaming DeepFilterNet 3 via ONNX Runtime.
//!
//! Wraps hot-mic's self-contained streaming DFN3 graph (`assets/denoiser_model.onnx`): inputs
//! `input_frame` (one 480-sample hop), `states` (the recurrent state carried between hops), and
//! `atten_lim_db` (attenuation-limit in dB); outputs `enhanced_audio_frame`, `new_states`, and
//! `lsnr`. The whole DFN3 pipeline (STFT, ERB/DF features, encoder, decoders, deep-filter apply,
//! ISTFT) is baked into the graph, so the host side is just hop-buffering plus state threading.
//!
//! The model runs on the native ONNX Runtime through the `ort` crate. `tract` (the pure-Rust
//! runtime SwiftF0 uses) cannot load this graph — it was exported with `SplitToSequence` and
//! ORT-fused ops — so this is the one effect in the workspace with a native dependency.
//!
//! Per [ADR-0014](../../docs/adr/0014-nn-inference-allocation.md) inference runs inline on the
//! audio thread with bounded allocation, not on an audio-through worker. The strict
//! allocation-free bar of ADR-0001 is therefore scoped out for this effect; its bar is bounded,
//! contention-free, deadline-safe inline work.

#![forbid(unsafe_code)]

use lindelion_effect::{Effect, EffectParam};
use ort::session::Session;
use ort::value::Tensor;

/// The streaming DFN3 model (single self-contained graph; see `speech/MODELS.md`).
pub const MODEL_BYTES: &[u8] = include_bytes!("../assets/denoiser_model.onnx");

/// Streaming hop in samples (10 ms at 48 kHz).
pub const HOP_SIZE: usize = 480;
/// Recurrent state vector length carried between hops.
pub const STATE_SIZE: usize = 45_304;
/// The only sample rate the model is trained for; other rates auto-bypass.
pub const MODEL_SAMPLE_RATE: f32 = 48_000.0;
/// End-to-end latency in samples: the host-side hop buffer (one hop) plus the model's internal
/// algorithmic latency (three hops). Measured by best-alignment of clean speech in vs. out
/// (31 dB match at this lag); equals four hops.
pub const LATENCY_SAMPLES: usize = 4 * HOP_SIZE;

/// Dry/wet mix, percent (0 = dry, 100 = fully denoised).
pub const PARAM_MIX: u32 = 0;
/// Attenuation limit in dB (caps how much the model may attenuate; 100 ≈ unlimited).
pub const PARAM_ATTEN_LIMIT_DB: u32 = 1;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_MIX,
        name: "Mix",
        min: 0.0,
        max: 100.0,
        default: 100.0,
        unit: "%",
    },
    EffectParam {
        index: PARAM_ATTEN_LIMIT_DB,
        name: "Atten Limit",
        min: 0.0,
        max: 100.0,
        default: 100.0,
        unit: "dB",
    },
];

/// Streaming DeepFilterNet 3 speech denoiser.
pub struct SpeechDenoiser {
    mix: f32,
    atten_lim_db: f32,
    bypassed: bool,
    sample_rate: f32,
    /// `true` only at the model's native rate; otherwise `process` is passthrough.
    active: bool,
    session: Option<Session>,
    /// Recurrent state threaded through every hop.
    states: Vec<f32>,
    /// Accumulates input until a full hop is available.
    in_buf: Vec<f32>,
    in_pos: usize,
    /// Holds the most recent enhanced hop being read out (primes `HOP_SIZE` zeros for latency).
    out_buf: Vec<f32>,
    out_pos: usize,
}

impl SpeechDenoiser {
    pub fn new() -> Self {
        Self {
            mix: 1.0,
            atten_lim_db: 100.0,
            bypassed: false,
            sample_rate: MODEL_SAMPLE_RATE,
            active: false,
            session: None,
            states: vec![0.0; STATE_SIZE],
            in_buf: vec![0.0; HOP_SIZE],
            in_pos: 0,
            out_buf: vec![0.0; HOP_SIZE],
            out_pos: 0,
        }
    }

    /// Run the model on the filled `in_buf`, blending the enhanced hop into `out_buf`. On any
    /// runtime error the dry hop is passed through so the audio never drops out.
    fn run_hop(&mut self) {
        let frame_data = self.in_buf.clone();
        let state_data = self.states.clone();
        let atten = self.atten_lim_db;

        let enhanced = self.infer(frame_data, state_data, atten);
        match enhanced {
            Some((enh, new_states)) => {
                let mix = self.mix;
                for (out, (&wet, &dry)) in self
                    .out_buf
                    .iter_mut()
                    .zip(enh.iter().zip(self.in_buf.iter()))
                {
                    *out = mix * wet + (1.0 - mix) * dry;
                }
                if new_states.len() == STATE_SIZE {
                    self.states.copy_from_slice(&new_states);
                }
            }
            None => self.out_buf.copy_from_slice(&self.in_buf),
        }
    }

    /// One model forward pass. Returns `(enhanced_hop, new_states)`, or `None` on any error.
    fn infer(
        &mut self,
        frame: Vec<f32>,
        state: Vec<f32>,
        atten: f32,
    ) -> Option<(Vec<f32>, Vec<f32>)> {
        let session = self.session.as_mut()?;
        let frame = Tensor::from_array(([HOP_SIZE], frame)).ok()?;
        let state = Tensor::from_array(([STATE_SIZE], state)).ok()?;
        let atten = Tensor::from_array(([1usize], vec![atten])).ok()?;
        let outputs = session
            .run(ort::inputs![
                "input_frame" => frame,
                "states" => state,
                "atten_lim_db" => atten
            ])
            .ok()?;
        let (_, enh) = outputs["enhanced_audio_frame"]
            .try_extract_tensor::<f32>()
            .ok()?;
        let (_, ns) = outputs["new_states"].try_extract_tensor::<f32>().ok()?;
        Some((enh.to_vec(), ns.to_vec()))
    }
}

impl Default for SpeechDenoiser {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for SpeechDenoiser {
    fn name(&self) -> &str {
        "Speech Denoiser"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_MIX => self.mix = (value / 100.0).clamp(0.0, 1.0),
            PARAM_ATTEN_LIMIT_DB => self.atten_lim_db = value.clamp(0.0, 100.0),
            _ => {}
        }
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate;
        self.active = (sample_rate - MODEL_SAMPLE_RATE).abs() < 1.0;
        if self.active && self.session.is_none() {
            self.session = Session::builder()
                .ok()
                .and_then(|mut b| b.commit_from_memory(MODEL_BYTES).ok());
        }
        self.reset();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed || !self.active || self.session.is_none() {
            return;
        }
        for sample in buffer.iter_mut() {
            let out = self.out_buf[self.out_pos];
            self.in_buf[self.in_pos] = *sample;
            self.in_pos += 1;
            self.out_pos += 1;
            if self.in_pos == HOP_SIZE {
                self.run_hop();
                self.in_pos = 0;
                self.out_pos = 0;
            }
            *sample = out;
        }
    }

    fn latency_samples(&self) -> usize {
        if self.active && !self.bypassed {
            LATENCY_SAMPLES
        } else {
            0
        }
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn reset(&mut self) {
        self.states.iter_mut().for_each(|s| *s = 0.0);
        self.in_buf.iter_mut().for_each(|s| *s = 0.0);
        self.out_buf.iter_mut().for_each(|s| *s = 0.0);
        self.in_pos = 0;
        self.out_pos = 0;
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(9);
        bytes.extend_from_slice(&(self.mix * 100.0).to_le_bytes());
        bytes.extend_from_slice(&self.atten_lim_db.to_le_bytes());
        bytes.push(self.bypassed as u8);
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        if state.len() >= 8 {
            let mix = f32::from_le_bytes([state[0], state[1], state[2], state[3]]);
            let atten = f32::from_le_bytes([state[4], state[5], state[6], state[7]]);
            self.set_parameter(PARAM_MIX, mix);
            self.set_parameter(PARAM_ATTEN_LIMIT_DB, atten);
        }
        if state.len() >= 9 {
            self.bypassed = state[8] != 0;
        }
    }
}
