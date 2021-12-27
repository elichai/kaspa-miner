#[cfg(any(not(target_arch = "x86_64"), feature = "no-asm", target_os = "windows"))]
pub(super) fn f1600(state: &mut [u64; 25]) {
    keccak::f1600(state);
}

#[cfg(all(target_arch = "x86_64", not(feature = "no-asm"), not(target_os = "windows")))]
pub(super) fn f1600(state: &mut [u64; 25]) {
    extern "C" {
        fn KeccakF1600(state: &mut [u64; 25]);
    }
    unsafe { KeccakF1600(state) }
}
