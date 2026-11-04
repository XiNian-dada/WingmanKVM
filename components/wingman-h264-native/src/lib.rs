//! The standalone encoder has its own GPL license and address space.
//! This crate embeds the executable; the application communicates over pipes.
pub const PROGRAM: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/wingman-h264-encoder"));
