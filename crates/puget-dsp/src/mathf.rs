//! Feature-gated transcendental math: `std` on the host, `libm` on `no_std`.
//! One shared copy for every Puget core (previously duplicated per crate — and
//! it drifted, which is exactly why it now lives here).

#[cfg(feature = "std")]
mod imp {
    #[inline]
    pub fn sin(x: f32) -> f32 {
        x.sin()
    }
    #[inline]
    pub fn cos(x: f32) -> f32 {
        x.cos()
    }
    #[inline]
    pub fn exp(x: f32) -> f32 {
        x.exp()
    }
    #[inline]
    pub fn powf(a: f32, b: f32) -> f32 {
        a.powf(b)
    }
    #[inline]
    pub fn sqrt(x: f32) -> f32 {
        x.sqrt()
    }
    #[inline]
    pub fn floor(x: f32) -> f32 {
        x.floor()
    }
    #[inline]
    pub fn log2(x: f32) -> f32 {
        x.log2()
    }
    #[inline]
    pub fn tanh(x: f32) -> f32 {
        x.tanh()
    }
}

#[cfg(all(not(feature = "std"), feature = "libm"))]
mod imp {
    #[inline]
    pub fn sin(x: f32) -> f32 {
        libm::sinf(x)
    }
    #[inline]
    pub fn cos(x: f32) -> f32 {
        libm::cosf(x)
    }
    #[inline]
    pub fn exp(x: f32) -> f32 {
        libm::expf(x)
    }
    #[inline]
    pub fn powf(a: f32, b: f32) -> f32 {
        libm::powf(a, b)
    }
    #[inline]
    pub fn sqrt(x: f32) -> f32 {
        libm::sqrtf(x)
    }
    #[inline]
    pub fn floor(x: f32) -> f32 {
        libm::floorf(x)
    }
    #[inline]
    pub fn log2(x: f32) -> f32 {
        libm::log2f(x)
    }
    #[inline]
    pub fn tanh(x: f32) -> f32 {
        libm::tanhf(x)
    }
}

pub use imp::{cos, exp, floor, log2, powf, sin, sqrt, tanh};

/// 2^x = e^(x·ln2).
#[inline]
pub fn exp2(x: f32) -> f32 {
    exp(x * core::f32::consts::LN_2)
}
