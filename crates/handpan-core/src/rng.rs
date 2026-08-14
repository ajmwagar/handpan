//! Tiny deterministic noise source for strike excitation. Xorshift32 —
//! fast, allocation-free, and identical on host and embedded targets.

#[derive(Clone)]
pub struct Rng(u32);

impl Rng {
    #[inline]
    pub fn new(seed: u32) -> Self {
        // Avoid the all-zero state, which xorshift cannot escape.
        Self(seed | 1)
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform noise in [-1.0, 1.0].
    #[inline]
    pub fn next_bipolar(&mut self) -> f32 {
        (self.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}
