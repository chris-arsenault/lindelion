//! Platform-neutral **reassigned** spectrogram model: scatters per-bin reassignment frames
//! `(magnitude, frequency offset, time offset)` into a scrolling 2-D intensity image, decoupled
//! from drawing. Unlike the magnitude [`Spectrogram`] (which max-bins each frame into one column on
//! a log-frequency axis), this **accumulates** each bin's energy at its *reassigned* row and column,
//! sharpening tones (frequency reassignment) and transients (time reassignment).
//!
//! It shares the magnitude spectrogram's log-frequency axis ([`super::spectrogram::log_freq_row`])
//! and dB/colour mapping ([`super::spectrogram::magnitude_to_db`] /
//! [`super::spectrogram::db_to_intensity`] / [`super::spectrogram::colormap`]), and exposes the same
//! `rows`/`columns`/`newest_column`/`column_in_display_order` surface, so the editor view renders it
//! interchangeably with the magnitude model.
//!
//! Time reassignment shifts a bin's energy among the most-recent columns, so the newest few columns
//! are kept "open" (still accumulating) before they finalize into the displayed scroll history. The
//! analysis hop is the analyzer's 75 % overlap (`frame_size / 4`); a bin can be reassigned backward
//! by up to one window (`Cmax = frame_size / hop` columns) but never into a not-yet-arrived future
//! column. This model is platform-neutral and `make ci`-tested.

use super::spectrogram::{FreqScale, db_to_intensity_in, freq_row, magnitude_to_db};

pub struct ReassignedSpectrogram {
    rows: usize,
    columns: usize,
    bins: usize,
    sample_rate: f32,
    frame_size: usize,
    scale: FreqScale,
    db_floor: f32,
    db_ceil: f32,
    hop: usize,
    /// Max backward column reassignment (`frame_size / hop`); also the open-window depth - 1.
    cmax: usize,
    /// Open-window energy accumulators: `(cmax + 1)` columns × `rows`, indexed by absolute column
    /// modulo `cmax + 1`. A column finalizes (energy → intensity, into `data`) once it can no longer
    /// receive a backward-reassigned contribution.
    open: Vec<f32>,
    /// Finalized intensities, `columns * rows`, column-major, ring-buffered over `columns` — the
    /// displayed scroll history (same shape as the magnitude model's buffer).
    data: Vec<f32>,
    /// Absolute index of the next frame to push (also the current open column).
    frame_count: usize,
    write_col: usize,
    filled: usize,
}

impl ReassignedSpectrogram {
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
        let frame_size = frame_size.max(4);
        let hop = (frame_size / 4).max(1); // ReassignStft's 75 % overlap
        let cmax = (frame_size / hop).max(1);
        let open_cols = cmax + 1;
        Self {
            rows,
            columns,
            bins,
            sample_rate,
            frame_size,
            scale,
            db_floor,
            db_ceil,
            hop,
            cmax,
            open: vec![0.0; open_cols * rows],
            data: vec![0.0; columns * rows],
            frame_count: 0,
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

    /// Scatter one reassignment frame into the image, then advance one column. Each bin's `mag²`
    /// energy is accumulated at its reassigned row (`log_freq_row(bin + freq_offset)`) and column
    /// (`current + clamp(round(time_offset / hop), −Cmax, 0)`); the column that can no longer receive
    /// a contribution is finalized (energy → dB intensity) into the displayed history.
    pub fn push_frame(&mut self, magnitudes: &[f32], freq_offsets: &[f32], time_offsets: &[f32]) {
        let open_cols = self.cmax + 1;
        let f = self.frame_count;
        let n = magnitudes.len().min(self.bins);

        for (k, &mag) in magnitudes.iter().enumerate().take(n) {
            if mag == 0.0 {
                continue;
            }
            let df = freq_offsets.get(k).copied().unwrap_or(0.0);
            let dt = time_offsets.get(k).copied().unwrap_or(0.0);

            let reassigned_bin = (k as f32 + df).clamp(0.0, (self.bins - 1) as f32);
            let row = (freq_row(
                self.scale,
                reassigned_bin,
                self.rows,
                self.sample_rate,
                self.frame_size,
            )
            .round() as usize)
                .min(self.rows - 1);

            let col_off = (dt / self.hop as f32)
                .round()
                .clamp(-(self.cmax as f32), 0.0) as i64;
            let col_abs = f as i64 + col_off;
            if col_abs < 0 {
                continue; // window predates the start of time
            }
            let slot = (col_abs as usize) % open_cols;
            self.open[slot * self.rows + row] += mag * mag;
        }

        // Finalize the column whose contributors (frames `c..=c+Cmax`) are now all processed.
        let finalize = f as i64 - self.cmax as i64;
        if finalize >= 0 {
            let slot = (finalize as usize) % open_cols;
            let src = slot * self.rows;
            let dst = self.write_col * self.rows;
            for r in 0..self.rows {
                let energy = self.open[src + r];
                self.data[dst + r] =
                    db_to_intensity_in(magnitude_to_db(energy.sqrt()), self.db_floor, self.db_ceil);
                self.open[src + r] = 0.0; // clear so the slot can serve the next column
            }
            self.write_col = (self.write_col + 1) % self.columns;
            self.filled = (self.filled + 1).min(self.columns);
        }

        self.frame_count += 1;
    }

    /// The most recently finalized column (`rows` intensities, low frequency first).
    pub fn newest_column(&self) -> &[f32] {
        let col = (self.write_col + self.columns - 1) % self.columns;
        &self.data[col * self.rows..col * self.rows + self.rows]
    }

    /// Finalized columns in display order, oldest first (`display_index` `0` = oldest visible).
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cenedril_vizia::spectrogram::{FreqScale, Spectrogram};

    const SR: f32 = 48_000.0;
    const FRAME: usize = 2048;
    const BINS: usize = FRAME / 2 + 1;

    fn max_of(col: &[f32]) -> f32 {
        col.iter().copied().fold(0.0f32, f32::max)
    }

    #[test]
    fn frequency_reassignment_is_sharper_than_magnitude() {
        let rows = 320;
        let cols = 64;
        let k0 = 40usize;

        // A leakage frame: bins k0-3..k0+3 carry tapering magnitude; each `df` points back to k0
        // (so reassignment concentrates them onto a single bin/row); no time shift.
        let mut mag = vec![0.0f32; BINS];
        let mut df = vec![0.0f32; BINS];
        let dt = vec![0.0f32; BINS];
        for d in -3i32..=3 {
            let k = (k0 as i32 + d) as usize;
            mag[k] = 1.0 - 0.2 * d.unsigned_abs() as f32;
            df[k] = -(d as f32);
        }

        let mut reassigned =
            ReassignedSpectrogram::new(rows, cols, SR, BINS, FRAME, FreqScale::Log, -100.0, 0.0);
        let mut magnitude =
            Spectrogram::new(rows, cols, SR, BINS, FRAME, FreqScale::Log, -100.0, 0.0);
        for _ in 0..10 {
            reassigned.push_frame(&mag, &df, &dt);
            magnitude.push_column(&mag);
        }

        let count_above = |col: &[f32]| {
            let peak = max_of(col);
            col.iter().filter(|&&v| v >= peak * 0.5 && v > 0.01).count()
        };
        let r_rows = count_above(reassigned.newest_column());
        let m_rows = count_above(magnitude.newest_column());
        assert!(
            r_rows < m_rows,
            "reassigned rows {r_rows} not < magnitude rows {m_rows}"
        );
        assert!(
            r_rows <= 2,
            "reassigned should concentrate; got {r_rows} rows"
        );
    }

    #[test]
    fn time_reassignment_concentrates_a_transient_in_fewer_columns() {
        let rows = 128;
        let cols = 64;
        let hop = FRAME / 4;

        let flat = vec![1.0f32; BINS];
        let zero = vec![0.0f32; BINS];
        let mut reassigned =
            ReassignedSpectrogram::new(rows, cols, SR, BINS, FRAME, FreqScale::Log, -100.0, 0.0);
        let mut magnitude =
            Spectrogram::new(rows, cols, SR, BINS, FRAME, FreqScale::Log, -100.0, 0.0);

        for _ in 0..2 {
            reassigned.push_frame(&zero, &zero, &zero);
            magnitude.push_column(&zero);
        }
        // Four broadband frames whose time offset ramps so all reassign to one absolute column.
        for i in 0..4i32 {
            let dt = vec![-(i as f32) * hop as f32; BINS];
            reassigned.push_frame(&flat, &zero, &dt);
            magnitude.push_column(&flat);
        }
        for _ in 0..8 {
            reassigned.push_frame(&zero, &zero, &zero);
            magnitude.push_column(&zero);
        }

        let mut r_cols = 0;
        for i in 0..reassigned.filled() {
            if max_of(reassigned.column_in_display_order(i)) > 0.5 {
                r_cols += 1;
            }
        }
        let mut m_cols = 0;
        for i in 0..magnitude.filled() {
            if max_of(magnitude.column_in_display_order(i)) > 0.5 {
                m_cols += 1;
            }
        }
        assert!(
            r_cols < m_cols,
            "reassigned cols {r_cols} not < magnitude cols {m_cols}"
        );
        assert!(
            r_cols <= 1,
            "transient should concentrate; got {r_cols} cols"
        );
    }

    #[test]
    fn silence_is_floor_and_output_is_bounded() {
        let mut sg =
            ReassignedSpectrogram::new(64, 16, SR, BINS, FRAME, FreqScale::Log, -100.0, 0.0);
        let zero = vec![0.0f32; BINS];
        for _ in 0..20 {
            sg.push_frame(&zero, &zero, &zero);
        }
        for i in 0..sg.filled() {
            for &v in sg.column_in_display_order(i) {
                assert_eq!(v, 0.0, "silence not at floor");
            }
        }

        // Out-of-range offsets must clamp (no panic, bounded output).
        let mag = vec![0.5f32; BINS];
        let df = vec![1.0e6f32; BINS];
        let dt = vec![-1.0e6f32; BINS];
        for _ in 0..10 {
            sg.push_frame(&mag, &df, &dt);
        }
        for i in 0..sg.filled() {
            for &v in sg.column_in_display_order(i) {
                assert!(
                    v.is_finite() && (0.0..=1.0).contains(&v),
                    "out of range: {v}"
                );
            }
        }
    }
}
