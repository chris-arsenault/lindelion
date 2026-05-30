//! Voice Gate: a Silero-VAD-driven gate for spoken word.
//!
//! Silero VAD emits a speech probability for each 512-sample chunk at 16 kHz. The host runs at
//! 48 kHz, so input is decimated 3:1 (box-averaged) into 16 kHz chunks; each full chunk runs the
//! model and updates the speech probability. A hysteretic, hold-and-release smoothed gain then
//! gates the 48 kHz audio: open while speech is present (plus a hold tail), closing to a floor
//! during non-speech. The audio is never delayed — the gate decision lags by up to one chunk
//! (32 ms) but the gain is applied in place, so reported latency is zero.
//!
//! Silero v5 (`assets/silero_vad.onnx`) cannot be optimized by `tract` (an `If`/`Squeeze` in the
//! decoder fails analysis), so it runs on the native ONNX Runtime via `ort` like the speech
//! denoiser ([ADR-0015](../../docs/adr/0015-denoiser-native-onnx-runtime.md)). Inference is inline
//! with bounded allocation per [ADR-0014](../../docs/adr/0014-nn-inference-allocation.md): the
//! per-sample decimate/gate path is allocation-free; only the once-per-chunk model run allocates.

#![forbid(unsafe_code)]

use lindelion_dsp_utils::db_to_gain;
use lindelion_effect::{Effect, EffectParam};
use ort::session::Session;
use ort::value::Tensor;

/// The Silero VAD model (v5; see `speech/MODELS.md`).
pub const MODEL_BYTES: &[u8] = include_bytes!("../assets/silero_vad.onnx");

/// The only host sample rate the gate operates at; other rates auto-bypass.
pub const MODEL_SAMPLE_RATE: f32 = 48_000.0;
/// Silero's native rate.
pub const VAD_SAMPLE_RATE: i64 = 16_000;
/// Host:VAD decimation factor (48 kHz / 16 kHz).
pub const DECIMATION: usize = 3;
/// Silero v5 window length in 16 kHz samples (new samples advanced per inference).
pub const VAD_CHUNK: usize = 512;
/// Context samples (the last 64 of the previous window) prepended to each window. Silero v5
/// requires this; without it the internal STFT windows are misaligned and speech is not detected.
pub const VAD_CONTEXT: usize = 64;
/// Model input length: context + window.
pub const VAD_INPUT: usize = VAD_CONTEXT + VAD_CHUNK;
/// Silero recurrent state length (`[2, 1, 128]`).
pub const STATE_SIZE: usize = 2 * 128;

/// Speech-probability threshold above which the gate opens (0..1).
pub const PARAM_THRESHOLD: u32 = 0;
/// Gate open ramp time in ms.
pub const PARAM_ATTACK_MS: u32 = 1;
/// Hold time in ms after speech stops before the gate starts closing.
pub const PARAM_HOLD_MS: u32 = 2;
/// Gate close ramp time in ms.
pub const PARAM_RELEASE_MS: u32 = 3;
/// Attenuation of the closed gate in dB (0 = no gating).
pub const PARAM_REDUCTION_DB: u32 = 4;

const PARAMS: &[EffectParam] = &[
    EffectParam {
        index: PARAM_THRESHOLD,
        name: "Threshold",
        min: 0.0,
        max: 1.0,
        default: 0.5,
        unit: "",
    },
    EffectParam {
        index: PARAM_ATTACK_MS,
        name: "Attack",
        min: 0.1,
        max: 50.0,
        default: 5.0,
        unit: "ms",
    },
    EffectParam {
        index: PARAM_HOLD_MS,
        name: "Hold",
        min: 0.0,
        max: 1000.0,
        default: 200.0,
        unit: "ms",
    },
    EffectParam {
        index: PARAM_RELEASE_MS,
        name: "Release",
        min: 10.0,
        max: 1000.0,
        default: 150.0,
        unit: "ms",
    },
    EffectParam {
        index: PARAM_REDUCTION_DB,
        name: "Reduction",
        min: 0.0,
        max: 80.0,
        default: 30.0,
        unit: "dB",
    },
];

fn time_coeff(time_s: f32, sample_rate: f32) -> f32 {
    if time_s <= 0.0 || sample_rate <= 0.0 {
        0.0
    } else {
        (-1.0 / (time_s * sample_rate)).exp()
    }
}

/// Model-free gate-gain controller: maps a stream of VAD probabilities (updated once per chunk)
/// plus per-sample ticks into a hysteretic, hold-and-release-smoothed gain. Deterministic and
/// allocation-free; tested without the model.
struct GateController {
    threshold: f32,
    floor_gain: f32,
    attack_coeff: f32,
    release_coeff: f32,
    hold_samples: usize,
    speech: bool,
    hold_counter: usize,
    gain: f32,
}

impl GateController {
    fn new() -> Self {
        Self {
            threshold: 0.5,
            floor_gain: db_to_gain(-30.0),
            attack_coeff: 0.0,
            release_coeff: 0.0,
            hold_samples: 0,
            speech: false,
            hold_counter: 0,
            gain: db_to_gain(-30.0),
        }
    }

    /// Update from a fresh VAD probability (once per 16 kHz chunk).
    fn on_vad(&mut self, prob: f32) {
        self.speech = prob >= self.threshold;
        if self.speech {
            self.hold_counter = self.hold_samples;
        }
    }

    /// Advance one host sample and return the smoothed gain to apply.
    fn tick(&mut self) -> f32 {
        let open = self.speech || self.hold_counter > 0;
        if self.hold_counter > 0 {
            self.hold_counter -= 1;
        }
        let target = if open { 1.0 } else { self.floor_gain };
        let coeff = if target > self.gain {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.gain = target + (self.gain - target) * coeff;
        self.gain
    }

    fn reset(&mut self) {
        self.speech = false;
        self.hold_counter = 0;
        self.gain = self.floor_gain;
    }
}

/// Silero-VAD-driven voice gate.
pub struct VoiceGate {
    threshold: f32,
    attack_ms: f32,
    hold_ms: f32,
    release_ms: f32,
    reduction_db: f32,
    bypassed: bool,
    sample_rate: f32,
    active: bool,
    session: Option<Session>,
    state: Vec<f32>,
    /// Last `VAD_CONTEXT` samples of the previous window, prepended to the next input.
    context: Vec<f32>,
    gate: GateController,
    chunk: Vec<f32>,
    chunk_len: usize,
    decim_acc: f32,
    decim_count: usize,
}

impl VoiceGate {
    pub fn new() -> Self {
        let mut gate = Self {
            threshold: 0.5,
            attack_ms: 5.0,
            hold_ms: 200.0,
            release_ms: 150.0,
            reduction_db: 30.0,
            bypassed: false,
            sample_rate: MODEL_SAMPLE_RATE,
            active: false,
            session: None,
            state: vec![0.0; STATE_SIZE],
            context: vec![0.0; VAD_CONTEXT],
            gate: GateController::new(),
            chunk: vec![0.0; VAD_CHUNK],
            chunk_len: 0,
            decim_acc: 0.0,
            decim_count: 0,
        };
        gate.reconfigure();
        gate
    }

    fn reconfigure(&mut self) {
        self.gate.threshold = self.threshold;
        self.gate.floor_gain = db_to_gain(-self.reduction_db);
        self.gate.attack_coeff = time_coeff(self.attack_ms / 1_000.0, self.sample_rate);
        self.gate.release_coeff = time_coeff(self.release_ms / 1_000.0, self.sample_rate);
        self.gate.hold_samples = ((self.hold_ms / 1_000.0) * self.sample_rate).round() as usize;
    }

    /// Run Silero on the filled `chunk` (prepended with the previous window's context), returning
    /// the speech probability and updating `state`. On any runtime error the gate is told there is
    /// no speech (below-threshold) so the audio never gets stuck open.
    fn run_vad(&mut self) -> f32 {
        let mut input = Vec::with_capacity(VAD_INPUT);
        input.extend_from_slice(&self.context);
        input.extend_from_slice(&self.chunk);
        let state = self.state.clone();
        let prob = match self.infer(input, state) {
            Some((prob, new_state)) => {
                if new_state.len() == STATE_SIZE {
                    self.state.copy_from_slice(&new_state);
                }
                prob
            }
            None => 0.0,
        };
        // The next window's context is the last VAD_CONTEXT samples of this window.
        self.context
            .copy_from_slice(&self.chunk[VAD_CHUNK - VAD_CONTEXT..]);
        prob
    }

    fn infer(&mut self, input: Vec<f32>, state: Vec<f32>) -> Option<(f32, Vec<f32>)> {
        let session = self.session.as_mut()?;
        let input = Tensor::from_array(([1usize, VAD_INPUT], input)).ok()?;
        let state = Tensor::from_array(([2usize, 1, 128], state)).ok()?;
        let sr = Tensor::from_array(([1usize], vec![VAD_SAMPLE_RATE])).ok()?;
        let outputs = session
            .run(ort::inputs![
                "input" => input,
                "state" => state,
                "sr" => sr
            ])
            .ok()?;
        let (_, prob) = outputs["output"].try_extract_tensor::<f32>().ok()?;
        let (_, ns) = outputs["stateN"].try_extract_tensor::<f32>().ok()?;
        Some((*prob.first()?, ns.to_vec()))
    }
}

impl Default for VoiceGate {
    fn default() -> Self {
        Self::new()
    }
}

impl Effect for VoiceGate {
    fn name(&self) -> &str {
        "Voice Gate"
    }

    fn parameters(&self) -> &[EffectParam] {
        PARAMS
    }

    fn set_parameter(&mut self, index: u32, value: f32) {
        match index {
            PARAM_THRESHOLD => self.threshold = value.clamp(0.0, 1.0),
            PARAM_ATTACK_MS => self.attack_ms = value.clamp(0.1, 50.0),
            PARAM_HOLD_MS => self.hold_ms = value.clamp(0.0, 1000.0),
            PARAM_RELEASE_MS => self.release_ms = value.clamp(10.0, 1000.0),
            PARAM_REDUCTION_DB => self.reduction_db = value.clamp(0.0, 80.0),
            _ => return,
        }
        self.reconfigure();
    }

    fn prepare(&mut self, sample_rate: f32, _max_block: usize) {
        self.sample_rate = sample_rate;
        self.active = (sample_rate - MODEL_SAMPLE_RATE).abs() < 1.0;
        if self.active && self.session.is_none() {
            self.session = Session::builder()
                .ok()
                .and_then(|mut b| b.commit_from_memory(MODEL_BYTES).ok());
        }
        self.reconfigure();
        self.reset();
    }

    fn process(&mut self, buffer: &mut [f32]) {
        if self.bypassed || !self.active || self.session.is_none() {
            return;
        }
        for sample in buffer.iter_mut() {
            self.decim_acc += *sample;
            self.decim_count += 1;
            if self.decim_count == DECIMATION {
                self.chunk[self.chunk_len] = self.decim_acc / DECIMATION as f32;
                self.chunk_len += 1;
                self.decim_acc = 0.0;
                self.decim_count = 0;
                if self.chunk_len == VAD_CHUNK {
                    let prob = self.run_vad();
                    self.gate.on_vad(prob);
                    self.chunk_len = 0;
                }
            }
            *sample *= self.gate.tick();
        }
    }

    fn latency_samples(&self) -> usize {
        0
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_bypassed(&mut self, bypassed: bool) {
        self.bypassed = bypassed;
    }

    fn reset(&mut self) {
        self.state.iter_mut().for_each(|s| *s = 0.0);
        self.context.iter_mut().for_each(|s| *s = 0.0);
        self.gate.reset();
        self.chunk_len = 0;
        self.decim_acc = 0.0;
        self.decim_count = 0;
    }

    fn save_state(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(21);
        bytes.extend_from_slice(&self.threshold.to_le_bytes());
        bytes.extend_from_slice(&self.attack_ms.to_le_bytes());
        bytes.extend_from_slice(&self.hold_ms.to_le_bytes());
        bytes.extend_from_slice(&self.release_ms.to_le_bytes());
        bytes.extend_from_slice(&self.reduction_db.to_le_bytes());
        bytes.push(self.bypassed as u8);
        bytes
    }

    fn load_state(&mut self, state: &[u8]) {
        let f = |b: &[u8], i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        if state.len() >= 20 {
            self.set_parameter(PARAM_THRESHOLD, f(state, 0));
            self.set_parameter(PARAM_ATTACK_MS, f(state, 4));
            self.set_parameter(PARAM_HOLD_MS, f(state, 8));
            self.set_parameter(PARAM_RELEASE_MS, f(state, 12));
            self.set_parameter(PARAM_REDUCTION_DB, f(state, 16));
        }
        if state.len() >= 21 {
            self.bypassed = state[20] != 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pure gate-mapping: the controller opens on a speech-probability above threshold and, after
    /// the hold tail, closes to the floor on a below-threshold probability — no model involved.
    #[test]
    fn gate_opens_above_threshold_and_closes_after_hold() {
        let mut gate = GateController::new();
        gate.threshold = 0.5;
        gate.floor_gain = db_to_gain(-30.0);
        gate.attack_coeff = time_coeff(0.005, 48_000.0);
        gate.release_coeff = time_coeff(0.05, 48_000.0);
        gate.hold_samples = 100;

        // Speech detected: gain ramps up toward fully open.
        gate.on_vad(0.9);
        let mut g = 0.0;
        for _ in 0..2_000 {
            g = gate.tick();
        }
        assert!(g > 0.9, "gate should be open on speech, got {g}");

        // Speech stops: after the hold tail elapses, gain decays toward the floor.
        gate.on_vad(0.1);
        for _ in 0..20_000 {
            g = gate.tick();
        }
        assert!(
            g < db_to_gain(-30.0) + 0.02,
            "gate should close to floor on silence, got {g}"
        );
    }

    #[test]
    fn hold_keeps_gate_open_briefly_after_speech() {
        let mut gate = GateController::new();
        gate.threshold = 0.5;
        gate.attack_coeff = 0.0; // instant for a crisp assertion
        gate.release_coeff = 0.0;
        gate.hold_samples = 50;
        gate.gain = db_to_gain(-30.0);

        gate.on_vad(0.9);
        let _ = gate.tick();
        gate.on_vad(0.1); // speech stops, hold begins
        let during_hold = gate.tick();
        assert_eq!(during_hold, 1.0, "gate stays open during the hold window");
        for _ in 0..60 {
            gate.tick();
        }
        let after_hold = gate.tick();
        assert!(after_hold < 0.5, "gate closes after the hold elapses");
    }
}
