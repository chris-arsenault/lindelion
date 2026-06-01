//! Channel adaptation between a device stream and the stereo VST3 chain.
//!
//! Decision (Galad M2): capture **mono → stereo** by duplicating the sample to L and R (centered
//! mono); render **stereo → mono** by averaging `(L+R)/2`; equal channel counts are bit-exact
//! passthrough. Other counts fall back to copying the common channels and zero-filling extras.

/// Adapt interleaved `src` (`src_channels`) into interleaved `dst` (`dst_channels`); returns the
/// number of frames written (min of the two frame capacities). Allocation-free.
pub fn adapt(src_channels: u16, dst_channels: u16, src: &[f32], dst: &mut [f32]) -> usize {
    let src_ch = src_channels.max(1) as usize;
    let dst_ch = dst_channels.max(1) as usize;
    let frames = (src.len() / src_ch).min(dst.len() / dst_ch);

    for frame in 0..frames {
        let si = frame * src_ch;
        let di = frame * dst_ch;
        match (src_ch, dst_ch) {
            (1, 2) => {
                dst[di] = src[si];
                dst[di + 1] = src[si];
            }
            (2, 1) => {
                dst[di] = 0.5 * (src[si] + src[si + 1]);
            }
            _ if src_ch == dst_ch => {
                dst[di..di + dst_ch].copy_from_slice(&src[si..si + src_ch]);
            }
            _ => {
                let common = src_ch.min(dst_ch);
                dst[di..di + common].copy_from_slice(&src[si..si + common]);
                for slot in dst[di + common..di + dst_ch].iter_mut() {
                    *slot = 0.0;
                }
            }
        }
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mono_to_stereo_duplicates() {
        let src = [0.1, 0.2, 0.3];
        let mut dst = [0.0; 6];
        assert_eq!(adapt(1, 2, &src, &mut dst), 3);
        assert_eq!(dst, [0.1, 0.1, 0.2, 0.2, 0.3, 0.3]);
    }

    #[test]
    fn stereo_to_mono_averages() {
        let src = [0.2, 0.4, 1.0, 0.0]; // frames (0.2,0.4) and (1.0,0.0)
        let mut dst = [0.0; 2];
        assert_eq!(adapt(2, 1, &src, &mut dst), 2);
        assert_eq!(dst, [0.3, 0.5]);
    }

    #[test]
    fn equal_channels_are_bit_exact() {
        let src = [0.1, 0.2, 0.3, 0.4];
        let mut dst = [0.0; 4];
        assert_eq!(adapt(2, 2, &src, &mut dst), 2);
        assert_eq!(dst, src);
    }
}
