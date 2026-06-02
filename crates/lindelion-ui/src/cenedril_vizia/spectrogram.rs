//! Platform-neutral spectrogram model: turns STFT magnitude frames into a scrolling 2-D intensity
//! image (log-frequency rows, dB-mapped intensity), decoupled from drawing. The Vizia view
//! (the `windows`-gated `platform` module) drains the audio→editor frame ring, pushes columns here,
//! and renders the image. This model is platform-neutral and `make ci`-tested.

/// dB window mapped to intensity `[0, 1]`.
const FLOOR_DB: f32 = -100.0;
const CEIL_DB: f32 = 0.0;
/// Bottom of the log-frequency axis.
const F_MIN_HZ: f32 = 20.0;

/// Frequency axis of the spectrogram: perceptual **log** (default) or **linear** (bin-proportional).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FreqScale {
    #[default]
    Log,
    Linear,
}

pub struct Spectrogram {
    rows: usize,
    columns: usize,
    /// row → half-open source-bin band `[start, end)` it aggregates (log-frequency binning). At
    /// high frequencies a row spans many linear bins; at low frequencies several rows may share a
    /// bin. Each band is at least one bin wide.
    row_band: Vec<(usize, usize)>,
    /// dB display window `[db_floor, db_ceil]` mapped to intensity `[0, 1]`.
    db_floor: f32,
    db_ceil: f32,
    /// `columns * rows` intensities, column-major, ring-buffered over `columns`.
    data: Vec<f32>,
    write_col: usize,
    filled: usize,
}

impl Spectrogram {
    #[allow(clippy::too_many_arguments)] // axis/range config: each is a distinct display parameter
    pub fn new(
        rows: usize,
        columns: usize,
        sample_rate: f32,
        bins: usize,
        frame_size: usize,
        scale: FreqScale,
        db_floor: f32,
        db_ceil: f32,
    ) -> Self {
        let rows = rows.max(1);
        let columns = columns.max(1);
        let bins = bins.max(1);
        let f_max = (sample_rate * 0.5).max(F_MIN_HZ * 2.0);
        let denom = (rows.max(2) - 1) as f32;
        // row-fraction → source bin, on the chosen scale.
        let bin_at = |frac: f32| -> usize {
            let frac = frac.clamp(0.0, 1.0);
            let bin = match scale {
                FreqScale::Log => {
                    let freq = F_MIN_HZ * (f_max / F_MIN_HZ).powf(frac);
                    freq * frame_size as f32 / sample_rate
                }
                FreqScale::Linear => frac * (bins.max(2) - 1) as f32,
            };
            (bin.round() as i64).clamp(0, bins as i64 - 1) as usize
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
            db_floor,
            db_ceil,
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
            self.data[base + r] =
                db_to_intensity_in(magnitude_to_db(peak), self.db_floor, self.db_ceil);
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

/// Fractional display row (0 = bottom / low frequency, `rows-1` = top) for a (possibly fractional)
/// source `bin` on the chosen frequency `scale`. This is the inverse of the row→bin mapping
/// [`Spectrogram`] builds, so the magnitude and reassigned spectrograms share one frequency axis.
/// Used by the reassigned-spectrogram scatter.
pub fn freq_row(
    scale: FreqScale,
    bin: f32,
    rows: usize,
    sample_rate: f32,
    frame_size: usize,
) -> f32 {
    let rows = rows.max(2);
    match scale {
        FreqScale::Log => {
            let f_max = (sample_rate * 0.5).max(F_MIN_HZ * 2.0);
            let freq = bin.max(0.0) * sample_rate / frame_size as f32;
            if freq <= F_MIN_HZ {
                return 0.0;
            }
            let frac = (freq / F_MIN_HZ).ln() / (f_max / F_MIN_HZ).ln();
            frac.clamp(0.0, 1.0) * (rows - 1) as f32
        }
        FreqScale::Linear => {
            let bins = frame_size / 2 + 1;
            let max_bin = (bins.max(2) - 1) as f32;
            (bin.max(0.0) / max_bin).clamp(0.0, 1.0) * (rows - 1) as f32
        }
    }
}

/// The log-frequency row mapping (the default scale). Thin wrapper over [`freq_row`].
pub fn log_freq_row(bin: f32, rows: usize, sample_rate: f32, frame_size: usize) -> f32 {
    freq_row(FreqScale::Log, bin, rows, sample_rate, frame_size)
}

pub fn magnitude_to_db(magnitude: f32) -> f32 {
    20.0 * magnitude.max(1.0e-9).log10()
}

/// Map `db` to `[0, 1]` over an explicit `[floor, ceil]` display window (clamped; guards
/// `floor < ceil`). The dB-range control drives `floor`/`ceil`.
pub fn db_to_intensity_in(db: f32, floor: f32, ceil: f32) -> f32 {
    let span = (ceil - floor).max(1.0e-6);
    ((db - floor) / span).clamp(0.0, 1.0)
}

/// The default dB window `[-100, 0]`. Thin wrapper over [`db_to_intensity_in`].
pub fn db_to_intensity(db: f32) -> f32 {
    db_to_intensity_in(db, FLOOR_DB, CEIL_DB)
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

/// Selectable color map for the spectrogram render.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorMap {
    #[default]
    Magma,
    Viridis,
    Grayscale,
}

/// Perceptual **viridis** color map (dark purple → blue → green → yellow), a polynomial fit
/// (Matt Zucker) to matplotlib's viridis — perceptually uniform and monotonic in lightness, the same
/// fit family as the magma [`colormap`].
#[allow(clippy::excessive_precision)] // the viridis polynomial fit constants are kept verbatim
fn viridis(intensity: f32) -> [u8; 4] {
    let t = intensity.clamp(0.0, 1.0);
    const C0: [f32; 3] = [0.2777273272234177, 0.005407344544966578, 0.3340998053353061];
    const C1: [f32; 3] = [0.1050930431085774, 1.404613529898575, 1.384590162594685];
    const C2: [f32; 3] = [-0.3308618287255563, 0.214847559468213, 0.09509516302823659];
    const C3: [f32; 3] = [-4.634230498983486, -5.799100973351585, -19.33244095627987];
    const C4: [f32; 3] = [6.228269936347081, 14.17993336680509, 56.69055260068105];
    const C5: [f32; 3] = [4.776384997670288, -13.74514537774601, -65.35303263337234];
    const C6: [f32; 3] = [-5.435455855934631, 4.645852612178535, 26.3124352495832];
    let mut out = [0u8; 4];
    for (ch, byte) in out[..3].iter_mut().enumerate() {
        let v = C0[ch]
            + t * (C1[ch] + t * (C2[ch] + t * (C3[ch] + t * (C4[ch] + t * (C5[ch] + t * C6[ch])))));
        *byte = (v.clamp(0.0, 1.0) * 255.0) as u8;
    }
    out[3] = 255;
    out
}

/// Map an intensity `[0, 1]` to RGBA through the selected [`ColorMap`].
pub fn colormap_for(map: ColorMap, intensity: f32) -> [u8; 4] {
    match map {
        ColorMap::Magma => colormap(intensity),
        ColorMap::Viridis => viridis(intensity),
        ColorMap::Grayscale => {
            let g = (intensity.clamp(0.0, 1.0) * 255.0) as u8;
            [g, g, g, 255]
        }
    }
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
        let mut s = Spectrogram::new(256, 64, SR, BINS, FRAME, FreqScale::Log, -100.0, 0.0);
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
        let mut s = Spectrogram::new(256, 16, SR, BINS, FRAME, FreqScale::Log, -100.0, 0.0);
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
        let mut s = Spectrogram::new(64, 8, SR, BINS, FRAME, FreqScale::Log, -100.0, 0.0);
        s.push_column(&vec![0.0; BINS]);
        assert!(s.newest_column().iter().all(|&v| v == 0.0));
    }

    #[test]
    fn linear_scale_maps_bin_proportionally_and_sweeps_monotonically() {
        // `freq_row` on the linear scale is the bin fraction across the spectrum.
        let rows = 256;
        let max_bin = (BINS - 1) as f32;
        assert_eq!(freq_row(FreqScale::Linear, 0.0, rows, SR, FRAME), 0.0);
        assert!(
            (freq_row(FreqScale::Linear, max_bin, rows, SR, FRAME) - (rows - 1) as f32).abs()
                < 1e-3
        );
        let mid = freq_row(FreqScale::Linear, max_bin * 0.5, rows, SR, FRAME);
        assert!((mid - (rows - 1) as f32 * 0.5).abs() < 1.0);

        // A rising sweep still climbs monotonically on the linear-scale spectrogram.
        let mut s = Spectrogram::new(rows, 16, SR, BINS, FRAME, FreqScale::Linear, -100.0, 0.0);
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
    fn db_range_controls_the_intensity_window() {
        // Default [-100, 0] reproduces `db_to_intensity`.
        for &db in &[-100.0, -50.0, -20.0, 0.0] {
            assert!((db_to_intensity_in(db, -100.0, 0.0) - db_to_intensity(db)).abs() < 1e-6);
        }
        // Moving the window changes the mapping (the control's effect): a higher floor (-60) pushes a
        // mid-level dB toward the bottom, a lower floor (-120) lifts it — both differ from the default.
        let db = -30.0;
        assert!(db_to_intensity_in(db, -60.0, 0.0) < db_to_intensity(db));
        assert!(db_to_intensity_in(db, -120.0, 0.0) > db_to_intensity(db));
        // Clamped to [0, 1].
        assert_eq!(db_to_intensity_in(10.0, -60.0, 0.0), 1.0);
        assert_eq!(db_to_intensity_in(-80.0, -60.0, 0.0), 0.0);
    }

    #[test]
    fn colormap_luminance_is_monotonic() {
        let lum = |c: [u8; 4]| 0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32;
        assert!(lum(colormap(0.0)) < lum(colormap(0.5)));
        assert!(lum(colormap(0.5)) < lum(colormap(1.0)));
    }

    #[test]
    fn selectable_colormaps_are_monotonic_and_magma_matches() {
        let lum = |c: [u8; 4]| 0.299 * c[0] as f32 + 0.587 * c[1] as f32 + 0.114 * c[2] as f32;
        for map in [ColorMap::Magma, ColorMap::Viridis, ColorMap::Grayscale] {
            assert!(
                lum(colormap_for(map, 0.0)) < lum(colormap_for(map, 0.5)),
                "{map:?}"
            );
            assert!(
                lum(colormap_for(map, 0.5)) < lum(colormap_for(map, 1.0)),
                "{map:?}"
            );
        }
        // Magma reproduces the existing fixed colormap (regression).
        for &x in &[0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(colormap_for(ColorMap::Magma, x), colormap(x));
        }
        // Grayscale is neutral (R = G = B).
        let g = colormap_for(ColorMap::Grayscale, 0.6);
        assert_eq!(g[0], g[1]);
        assert_eq!(g[1], g[2]);
    }
}
