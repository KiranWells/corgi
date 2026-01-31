/*!
# Types

A Collection of types used throughout the application, and their associated functions.
 */

mod coloring;
mod image;
pub mod serde;

use emath::Vec2;

pub use self::coloring::*;
pub use self::image::*;

/// A utility trait for adjusting the angle of a vector
pub trait Rotate {
    /// Returns a copy of this rotated by `angle` radians
    fn rotated(&self, angle: f32) -> Self;
}

impl Rotate for Vec2 {
    fn rotated(&self, angle: f32) -> Self {
        Self {
            x: self.x * angle.cos() - self.y * angle.sin(),
            y: self.x * angle.sin() + self.y * angle.cos(),
        }
    }
}
