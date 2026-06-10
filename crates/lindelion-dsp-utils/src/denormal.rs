//! Hardware denormal flushing for the realtime path.
//!
//! Denormal (subnormal) floats arise in decaying IIR/feedback DSP — resonator tails, reverb,
//! waveguide rings — as the state decays toward zero. On most CPUs arithmetic on denormals is
//! one to two orders of magnitude slower than on normal floats, which can spike audio-thread load
//! and cause dropouts. The classic software fix is to flush tiny values to zero per sample; the
//! cheaper fix is to put the FPU in flush-to-zero mode once per audio callback so the hardware does
//! it for free, which also keeps the hot loops branch-free (and so auto-vectorizable).

/// Enable flush-to-zero (FTZ) and denormals-are-zero (DAZ) on the **current thread's** FPU.
///
/// Call once at the top of each audio callback: the mode is thread-local and some hosts reset it
/// between callbacks. Idempotent and allocation-free. A no-op on architectures without a supported
/// control register (the software guards remain the fallback there).
#[inline]
pub fn flush_denormals_on_this_thread() {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // MXCSR: bit 15 = FTZ (flush results to zero), bit 6 = DAZ (treat inputs as zero). Read it
        // with `stmxcsr`, set the bits, write it back with `ldmxcsr` (inline asm — the `_mm_*csr`
        // intrinsics are deprecated in favor of this).
        const FTZ_DAZ: u32 = (1 << 15) | (1 << 6);
        let mut mxcsr: u32 = 0;
        core::arch::asm!(
            "stmxcsr [{p}]",
            p = in(reg) core::ptr::addr_of_mut!(mxcsr),
            options(nostack, preserves_flags),
        );
        mxcsr |= FTZ_DAZ;
        core::arch::asm!(
            "ldmxcsr [{p}]",
            p = in(reg) core::ptr::addr_of!(mxcsr),
            options(nostack, readonly, preserves_flags),
        );
    }
    #[cfg(target_arch = "aarch64")]
    unsafe {
        // FPCR bit 24 = FZ (flush-to-zero for normal float ops).
        let mut fpcr: u64;
        core::arch::asm!("mrs {0}, fpcr", out(reg) fpcr, options(nomem, nostack));
        fpcr |= 1 << 24;
        core::arch::asm!("msr fpcr, {0}", in(reg) fpcr, options(nomem, nostack));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flush_denormals_is_callable() {
        // Smoke test: the call must not panic on the host architecture.
        flush_denormals_on_this_thread();
    }
}
