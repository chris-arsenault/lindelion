//! Platform-neutral spectrogram model: turns STFT magnitude frames into a scrolling 2-D intensity
//! image (log-frequency rows, dB-mapped intensity), decoupled from drawing. The Vizia view
//! (the `windows`-gated `platform` module) drains the audio→editor frame ring, pushes columns here,
//! and renders the image. This model is platform-neutral and `make ci`-tested.

/// dB window mapped to intensity `[0, 1]`.
const FLOOR_DB: f32 = -100.0;
const CEIL_DB: f32 = 0.0;
/// Bottom of the log-frequency axis.
const F_MIN_HZ: f32 = 20.0;

pub struct Spectrogram {
    rows: usize,
    columns: usize,
    /// row → half-open source-bin band `[start, end)` it aggregates (log-frequency binning). At
    /// high frequencies a row spans many linear bins; at low frequencies several rows may share a
    /// bin. Each band is at least one bin wide.
    row_band: Vec<(usize, usize)>,
    /// `columns * rows` intensities, column-major, ring-buffered over `columns`.
    data: Vec<f32>,
    write_col: usize,
    filled: usize,
}

impl Spectrogram {
    pub fn new(
        rows: usize,
        columns: usize,
        sample_rate: f32,
        bins: usize,
        frame_size: usize,
    ) -> Self {
        let rows = rows.max(1);
        let columns = columns.max(1);
        let bins = bins.max(1);
        let f_max = (sample_rate * 0.5).max(F_MIN_HZ * 2.0);
        let denom = (rows.max(2) - 1) as f32;
        let bin_at = |frac: f32| -> usize {
            let freq = F_MIN_HZ * (f_max / F_MIN_HZ).powf(frac.clamp(0.0, 1.0));
            ((freq * frame_size as f32 / sample_rate).round() as i64).clamp(0, bins as i64 - 1)
                as usize
        };
        let row_band = (0..rows)
            .map(|r| {
                // The row covers the band between its neighbours' midpoints.
                let lo = bin_at((r as f32 - 0.5) / denom);
                let hi = bin_at((r as f32 + 0.5) / denom);
                let start = lo.min(hi);
                let end = (lo.max(hi) + 1).min(bins); // exclusive, ≥ 1 wide
                (start, end)
            })
            .collect();
        Self {
            rows,
            columns,
            row_band,
            data: vec![0.0; columns * rows],
            write_col: 0,
            filled: 0,
        }
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn columns(&self) -> usize {
        self.columns
    }

    pub fn filled(&self) -> usize {
        self.filled
    }

    /// The center source-bin of a row's band (log-frequency axis). Useful for axis labels.
    pub fn bin_for_row(&self, row: usize) -> usize {
        let (start, end) = self.row_band[row.min(self.rows - 1)];
        (start + end - 1) / 2
    }

    /// Append one time column from a magnitude frame (the oldest column scrolls off). Each row takes
    /// the peak magnitude over its bin band, so a narrow spectral peak still lights its row.
    pub fn push_column(&mut self, magnitudes: &[f32]) {
        let base = self.write_col * self.rows;
        for r in 0..self.rows {
            let (start, end) = self.row_band[r];
            let end = end.min(magnitudes.len());
            let peak = magnitudes
                .get(start.min(end)..end)
                .unwrap_or(&[])
                .iter()
                .copied()
                .fold(0.0_f32, f32::max);
            self.data[base + r] = db_to_intensity(magnitude_to_db(peak));
        }
        self.write_col = (self.write_col + 1) % self.columns;
        self.filled = (self.filled + 1).min(self.columns);
    }

    /// The most recently written column (`rows` intensities, low frequency first).
    pub fn newest_column(&self) -> &[f32] {
        let col = (self.write_col + self.columns - 1) % self.columns;
        &self.data[col * self.rows..col * self.rows + self.rows]
    }

    /// Columns in display order, oldest first (`display_index` `0` = oldest of the visible window).
    pub fn column_in_display_order(&self, display_index: usize) -> &[f32] {
        let start = if self.filled < self.columns {
            0
        } else {
            self.write_col
        };
        let col = (start + display_index) % self.columns;
        &self.data[col * self.rows..col * self.rows + self.rows]
    }
}

pub fn magnitude_to_db(magnitude: f32) -> f32 {
    20.0 * magnitude.max(1.0e-9).log10()
}

pub fn db_to_intensity(db: f32) -> f32 {
    ((db - FLOOR_DB) / (CEIL_DB - FLOOR_DB)).clamp(0.0, 1.0)
}

/// Perceptual **magma** color map (black → purple → red → orange → near-white), a polynomial fit
/// (Matt Zucker) to matplotlib's magma — perceptually uniform and monotonic in lightness, the
/// standard for high-fidelity spectrograms.
#[allow(clippy::excessive_precision)] // the magma polynomial fit constants are kept verbatim
pub fn colormap(intensity: f32) -> [u8; 4] {
    let t = intensity.clamp(0.0, 1.0);
    const C0: [f32; 3] = [-0.002136, -0.000750, -0.005386];
    const C1: [f32; 3] = [0.251661, 0.677523, 2.494027];
    const C2: [f32; 3] = [8.353717, -3.577720, 0.314468];
    const C3: [f32; 3] = [-27.668733, 14.264731, -13.649213];
    const C4: [f32; 3] = [52.176140, -27.943606, 12.944169];
    const C5: [f32; 3] = [-50.768525, 29.046583, 4.234153];
    const C6: [f32; 3] = [18.655705, -11.489774, -5.601962];
    let mut out = [0u8; 4];
    for (ch, byte) in out[..3].iter_mut().enumerate() {
        let v = C0[ch]
            + t * (C1[ch] + t * (C2[ch] + t * (C3[ch] + t * (C4[ch] + t * (C5[ch] + t * C6[ch])))));
        *byte = (v.clamp(0.0, 1.0) * 255.0) as u8;
    }
    out[3] = 255;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 48_000.0;
    const FRAME: usize = 2048;
    const BINS: usize = FRAME / 2 + 1;

    fn bin_for_freq(freq: f32) -> usize {
        (freq * FRAME as f32 / SR).round() as usize
    }

    /// A small spectral peak (a few bins wide, like real STFT leakage) centered on `freq`.
    fn frame_peaked_at(freq: f32) -> Vec<f32> {
        let k = bin_for_freq(freq);
        let mut f = vec![0.0; BINS];
        for v in &mut f[k.saturating_sub(2)..=(k + 2).min(BINS - 1)] {
            *v = 1.0;
        }
        f
    }

    fn argmax_row(col: &[f32]) -> usize {
        let mut best = (0usize, f32::MIN);
        for (r, &v) in col.iter().enumerate() {
            if v > best.1 {
                best = (r, v);
            }
        }
        best.0
    }

    #[test]
    fn sine_peak_lands_on_its_log_frequency_row() {
        let mut s = Spectrogram::new(256, 64, SR, BINS, FRAME);
        let k = bin_for_freq(3_000.0);
        s.push_column(&frame_peaked_at(3_000.0));
        let row = argmax_row(s.newest_column());
        assert!(
            (s.bin_for_row(row) as i64 - k as i64).abs() <= 3,
            "peak row {row} → bin {} not near {k}",
            s.bin_for_row(row)
        );
    }

    #[test]
    fn rising_sweep_makes_a_monotonic_diagonal() {
        let mut s = Spectrogram::new(256, 16, SR, BINS, FRAME);
        let mut prev = 0usize;
        for (i, &freq) in [200.0, 600.0, 1_500.0, 4_000.0, 9_000.0].iter().enumerate() {
            s.push_column(&frame_peaked_at(freq));
            let row = argmax_row(s.newest_column());
            if i > 0 {
                assert!(row >= prev, "row {row} < prev {prev} at {freq} Hz");
            }
            prev = row;
        }
    }

    #[test]
    fn silence_is_all_floor() {
        let mut s = Spectrogram::new(64, 8, SR, BINS, FRAME);
        s.push_column(&vec![0.0; BINS]);
        assert!(s.newest_column().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn colormap_luminance_is_monotonic() {
        let lum = |c: [u8; 4]| 0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32;
        assert!(lum(colormap(0.0)) < lum(colormap(0.5)));
        assert!(lum(colormap(0.5)) < lum(colormap(1.0)));
    }
}
