//! Feature-gated transcendental math.

#[cfg(feature = "std")]
#[inline]
pub fn sin(x: f32) -> f32 {
    x.sin()
}
#[cfg(feature = "std")]
#[inline]
pub fn cos(x: f32) -> f32 {
    x.cos()
}
#[cfg(feature = "std")]
#[inline]
pub fn exp(x: f32) -> f32 {
    x.exp()
}
#[cfg(feature = "std")]
#[inline]
pub fn powf(a: f32, b: f32) -> f32 {
    a.powf(b)
}
#[cfg(feature = "std")]
#[inline]
pub fn floor(x: f32) -> f32 {
    x.floor()
}
#[cfg(feature = "std")]
#[inline]
pub fn sqrt(x: f32) -> f32 {
    x.sqrt()
}

#[cfg(all(not(feature = "std"), feature = "libm"))]
#[inline]
pub fn sin(x: f32) -> f32 {
    libm::sinf(x)
}
#[cfg(all(not(feature = "std"), feature = "libm"))]
#[inline]
pub fn cos(x: f32) -> f32 {
    libm::cosf(x)
}
#[cfg(all(not(feature = "std"), feature = "libm"))]
#[inline]
pub fn exp(x: f32) -> f32 {
    libm::expf(x)
}
#[cfg(all(not(feature = "std"), feature = "libm"))]
#[inline]
pub fn powf(a: f32, b: f32) -> f32 {
    libm::powf(a, b)
}
#[cfg(all(not(feature = "std"), feature = "libm"))]
#[inline]
pub fn floor(x: f32) -> f32 {
    libm::floorf(x)
}
#[cfg(all(not(feature = "std"), feature = "libm"))]
#[inline]
pub fn sqrt(x: f32) -> f32 {
    libm::sqrtf(x)
}
