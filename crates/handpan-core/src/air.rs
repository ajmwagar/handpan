//! "Air" — a compact stereo ambience so the instrument sits in a room the way
//! premium-handpan demos are always recorded. A trimmed Schroeder/Freeverb
//! (parallel damped combs → series allpasses), stable by construction
//! (all feedback gains < 1). Dependency-free; `alloc` for the delay buffers.

#[cfg(not(feature = "std"))]
use alloc::{vec, vec::Vec};

struct Comb {
    buf: Vec<f32>,
    idx: usize,
    store: f32,
    feedback: f32,
    damp: f32,
}

impl Comb {
    fn new(len: usize, feedback: f32, damp: f32) -> Self {
        Self { buf: vec![0.0; len.max(1)], idx: 0, store: 0.0, feedback, damp }
    }
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let out = self.buf[self.idx];
        self.store = out * (1.0 - self.damp) + self.store * self.damp;
        self.buf[self.idx] = input + self.store * self.feedback;
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    idx: usize,
    feedback: f32,
}

impl Allpass {
    fn new(len: usize, feedback: f32) -> Self {
        Self { buf: vec![0.0; len.max(1)], idx: 0, feedback }
    }
    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        let buf = self.buf[self.idx];
        let out = -input + buf;
        self.buf[self.idx] = input + buf * self.feedback;
        self.idx += 1;
        if self.idx >= self.buf.len() {
            self.idx = 0;
        }
        out
    }
}

/// A small stereo reverb. `process` returns the wet stereo pair; callers mix it
/// against the dry signal.
pub struct Air {
    combs_l: [Comb; 4],
    combs_r: [Comb; 4],
    aps_l: [Allpass; 2],
    aps_r: [Allpass; 2],
    wet_scale: f32,
}

impl Air {
    pub fn new(fs: f32) -> Self {
        // Freeverb tunings (samples @ 44.1k), scaled to fs; right channel is
        // offset by the classic stereo spread for width.
        let s = fs / 44_100.0;
        let sc = |n: usize| (n as f32 * s) as usize;
        let spread = sc(23);
        let comb_t = [1116usize, 1188, 1277, 1356];
        let ap_t = [556usize, 441];
        let feedback = 0.86;
        let damp = 0.24;

        let combs_l = [
            Comb::new(sc(comb_t[0]), feedback, damp),
            Comb::new(sc(comb_t[1]), feedback, damp),
            Comb::new(sc(comb_t[2]), feedback, damp),
            Comb::new(sc(comb_t[3]), feedback, damp),
        ];
        let combs_r = [
            Comb::new(sc(comb_t[0]) + spread, feedback, damp),
            Comb::new(sc(comb_t[1]) + spread, feedback, damp),
            Comb::new(sc(comb_t[2]) + spread, feedback, damp),
            Comb::new(sc(comb_t[3]) + spread, feedback, damp),
        ];
        let aps_l = [Allpass::new(sc(ap_t[0]), 0.5), Allpass::new(sc(ap_t[1]), 0.5)];
        let aps_r = [
            Allpass::new(sc(ap_t[0]) + spread, 0.5),
            Allpass::new(sc(ap_t[1]) + spread, 0.5),
        ];

        Self { combs_l, combs_r, aps_l, aps_r, wet_scale: 0.30 }
    }

    /// Process one sample; returns the wet `(left, right)` ambience.
    #[inline]
    pub fn process(&mut self, l: f32, r: f32) -> (f32, f32) {
        let input = (l + r) * 0.5;
        let mut wl = 0.0;
        let mut wr = 0.0;
        for c in &mut self.combs_l {
            wl += c.process(input);
        }
        for c in &mut self.combs_r {
            wr += c.process(input);
        }
        for a in &mut self.aps_l {
            wl = a.process(wl);
        }
        for a in &mut self.aps_r {
            wr = a.process(wr);
        }
        (wl * self.wet_scale * 0.25, wr * self.wet_scale * 0.25)
    }
}
