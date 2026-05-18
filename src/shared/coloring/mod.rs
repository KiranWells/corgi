pub mod color_spaces;
pub mod main;

use super::wgsl_primitives::*;

impl Default for main::Light {
    fn default() -> Self {
        Self {
            color: Vec3::splat(1.0),
            strength: 0.0,
            direction: Vec3::new(0.0, 0.0, 1.0),
            _padding: 0.0,
        }
    }
}

impl main::Light {
    pub fn new(color: [f32; 3], strength: f32, direction: [f32; 3]) -> Self {
        Self {
            color: color.into(),
            strength,
            direction: Vec3f::from(direction).normalize(),
            _padding: 0.0,
        }
    }

    pub fn normalize(&mut self) {
        // prevent instability where calling normalize
        // repeatedly results in different values
        if (self.direction.length() - 1.0).abs() < 1e-4 {
            return;
        }
        self.direction = self.direction.normalize();
    }
}
