//! Scoped PCB arithmetic environment. Never leave Node's calling thread changed.
//! Solver workers inherit it at creation and also enter it explicitly.
use std::{marker::PhantomData, rc::Rc};

pub(crate) struct Guard {
    saved: u64,
    // A floating environment belongs to the entering thread.
    _thread: PhantomData<Rc<()>>,
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn read() -> u64 {
    let mut value = 0u32;
    unsafe { std::arch::asm!("stmxcsr [{p}]", p = in(reg) &mut value,
        options(nostack, preserves_flags)); }
    value as u64
}
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
fn write(value: u64) {
    let value = value as u32;
    unsafe { std::arch::asm!("ldmxcsr [{p}]", p = in(reg) &value,
        options(nostack, preserves_flags)); }
}
#[cfg(target_arch = "aarch64")]
fn read() -> u64 {
    let value: u64;
    unsafe { std::arch::asm!("mrs {v}, fpcr", v = out(reg) value,
        options(nostack, preserves_flags)); }
    value
}
#[cfg(target_arch = "aarch64")]
fn write(value: u64) {
    unsafe { std::arch::asm!("msr fpcr, {v}", v = in(reg) value,
        options(nostack, preserves_flags)); }
}
#[cfg(not(any(target_arch = "x86", target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("PCB F32 FTZ requires a verified scoped float environment for this architecture");

impl Guard {
    pub(crate) fn enter() -> Self {
        let saved = read();
        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        let selected = (saved & !(3 << 13)) | (1 << 15) | (1 << 6); // RTE, FTZ, DAZ
        #[cfg(target_arch = "aarch64")]
        let selected = (saved & !(3 << 22)) | (1 << 24); // RTE, FZ
        write(selected);
        Self { saved, _thread: PhantomData }
    }
}
impl Drop for Guard {
    fn drop(&mut self) { write(self.saved); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hint::black_box;

    fn check_arithmetic() {
        let tiny = black_box(f32::from_bits(0x007fffff));
        assert_eq!((tiny * black_box(2.0)).to_bits(), 0);
        let negative = black_box(-f32::MIN_POSITIVE);
        assert_eq!((negative * black_box(0.5)).to_bits(), 0x80000000);
        // Subnormal input must be zero even when scaling would produce normal.
        let result = black_box(f32::from_bits(1)) * black_box(2.0f32.powi(126));
        assert_eq!(result.to_bits(), 0);
        // RTE and no contraction. A fused multiply-add would yield -2^-46.
        let a = black_box(f32::from_bits(0x3f800001));
        let b = black_box(f32::from_bits(0x3f7ffffe));
        assert_eq!((a * b - black_box(1.0)).to_bits(), 0);
    }
    #[test]
    fn ftz_rte_is_scoped_and_applies_to_workers() {
        let original = read();
        {
            let _guard = Guard::enter();
            check_arithmetic();
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    let worker_original = read();
                    { let _worker = Guard::enter(); check_arithmetic(); }
                    assert_eq!(read(), worker_original);
                }).join().unwrap();
            });
        }
        assert_eq!(read(), original);
        let failure = std::panic::catch_unwind(|| {
            let _guard = Guard::enter();
            panic!("test restoration");
        });
        assert!(failure.is_err());
        assert_eq!(read(), original);
    }
}
