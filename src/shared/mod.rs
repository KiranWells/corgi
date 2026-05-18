#![allow(clippy::needless_return)]

pub mod algorithms;
pub mod coloring;
pub mod types;
mod utils;
pub mod wgsl_primitives;

use wgsl_primitives::*;

impl Default for types::Transform {
    fn default() -> Self {
        Self {
            angle: 0.0,
            _padding: 0.0,
            prescale: Vec2::splat(1.0),
            postscale: Vec2::splat(1.0),
            offset: Vec2::splat(0.0),
        }
    }
}
