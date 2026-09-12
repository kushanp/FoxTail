//! FoxTail library: file tailing engine, highlighting, filters, and config.

#[cfg(all(feature = "wgpu", feature = "glow"))]
compile_error!(
    "features `wgpu` and `glow` are mutually exclusive; \
     use `--features wgpu` or `--no-default-features --features glow`"
);

#[cfg(not(any(feature = "wgpu", feature = "glow")))]
compile_error!("enable exactly one of the `wgpu` or `glow` features");

/// Rendering backend compiled into this build (`wgpu` or `glow`).
pub const RENDERER: &str = if cfg!(feature = "glow") {
    "glow"
} else {
    "wgpu"
};

pub mod config;
pub mod engine;
pub mod filter;
pub mod highlight;
pub mod util;
