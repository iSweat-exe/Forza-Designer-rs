//! Forza Designer core.
//!
//! * [`shapegen`] turns an image into a list of vinyl shapes (the geometric
//!   "primitive" search ported from Forza Designer 6).
//! * [`document`] is the on-disk JSON format (`fd6.shapes`, v1), compatible with
//!   files produced by the original Python application.
//! * [`inject`] writes those shapes into a running Forza Horizon game.
//!
//! This crate has no GUI dependency so it can be tested headless.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod document;
pub mod error;
pub mod inject;
pub mod shapegen;

pub use error::{Error, Result};
