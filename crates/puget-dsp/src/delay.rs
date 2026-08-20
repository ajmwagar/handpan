//! Fractional delay line (linear interpolation) — the shared primitive for
//! bores, strings and waveguides. One copy for every core.

extern crate alloc;
use alloc::{vec, vec::Vec};

use crate::mathf;

/// A fractional delay line with a cached last output.
pub struct Delay {
    buf: Vec<f32>,
    w: usize,
    delay: f32,
    last: f32,
}

impl Delay {
    /// Allocate for up to `max_len` samples of delay.
    pub fn new(max_len: usize) -> Self {
        Delay { buf: vec![0.0; max_len.max(4)], w: 0, delay: 1.0, last: 0.0 }
    }

    /// Set the delay in (fractional) samples, clamped to the buffer.
    #[inline]
    pub fn set_delay(&mut self, d: f32) {
        let max = (self.buf.len() - 2) as f32;
        self.delay = d.clamp(1.0, max);
    }

    /// The most recent interpolated output (without advancing).
    #[inline]
    pub fn last_out(&self) -> f32 {
        self.last
    }

    /// Push `input`, return the delayed (interpolated) sample.
    #[inline]
    pub fn tick(&mut self, input: f32) -> f32 {
        let len = self.buf.len();
        let mut dr = self.w as f32 - self.delay;
        if dr < 0.0 {
            dr += len as f32;
        }
        let fl = mathf::floor(dr);
        let i0 = fl as usize % len;
        let i1 = (i0 + 1) % len;
        let frac = dr - fl;
        let out = self.buf[i0] * (1.0 - frac) + self.buf[i1] * frac;
        self.buf[self.w] = input;
        self.w = (self.w + 1) % len;
        self.last = out;
        out
    }
}
