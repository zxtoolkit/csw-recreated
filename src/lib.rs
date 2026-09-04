//! Sampled tape audio and OUT traces to CSW pulses and back: the format
//! readers, the pulse detector, the digital filter and the CSW container.

pub mod container;
pub mod convert;
pub mod detect;
pub mod encode;
pub mod error;
pub mod filter;
pub mod iff;
pub mod out;
pub mod rle;
pub mod signal;
pub mod source;
pub mod voc;
pub mod wav;

mod complex;
