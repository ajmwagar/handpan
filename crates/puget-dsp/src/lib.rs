//! # puget-dsp
//!
//! Shared `no_std` DSP foundation for the Puget physical-modeling cores. Holds
//! the pieces that were being copy-pasted into every voice crate:
//!
//! * [`mathf`] — feature-gated transcendental math (`std` host / `libm` firmware).
//! * [`Ensemble`] / [`Voice`] — the generic humanized "section" (chairs, spread,
//!   width, onset stagger, stereo placement), written once for all families.
//!
//! This is an *internal* workspace crate with no third-party dependencies (only
//! optional `libm`), so the cores stay dependency-free and firmware-portable —
//! sharing a foundation, not taking on an external dependency.
//!
//! Proprietary. Copyright (c) 2026 Avery Wagar. All rights reserved.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod mathf;

mod ensemble;
pub use ensemble::{Ensemble, Voice};
