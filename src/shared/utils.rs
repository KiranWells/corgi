use super::coloring::color_spaces::{
    COLOR_SPACE_HSL, COLOR_SPACE_HSV, COLOR_SPACE_LINEAR_SRGB, COLOR_SPACE_OKLAB,
    COLOR_SPACE_OKLCH, hsl_to_linear_srgb, hsv_to_linear_srgb, linear_srgb_to_hsl,
    linear_srgb_to_hsv, linear_srgb_to_oklab, linear_srgb_to_oklch, oklab_to_linear_srgb,
    oklch_to_linear_srgb,
};
use crate::shared::wgsl_primitives::*;

#[expect(clippy::approx_constant)]
pub const PI: f32 = 3.1415926535897932384626433832795028841971693993751058209749445923078164062862089986280348253421170679;
pub const TAU: f32 = 2.0 * PI;
pub const ESCAPE_RADIUS: f32 = 10000.0;
pub const FRACTEXP_SCALE_FACTOR: f32 = 10.0;
// flags
pub const STRIPES_ENABLED: u32 = 0x1u32;
pub const TOTAL_ANGLE_ENABLED: u32 = 0x2u32;
pub const ORBIT_ENABLED: u32 = 0x4u32;
pub const DERIVATIVE_ENABLED: u32 = 0x8u32;
pub const JULIA: u32 = 0x10000000u32;

pub fn isnan(x: f32) -> bool {
    let bits = bitcast(x);
    let exp = (bits >> 23) & 0xffu32;
    let frac = bits & 0x7fffffu32;
    return exp == 0xffu32 && frac != 0u32;
}

pub fn isinf(x: f32) -> bool {
    let bits = bitcast(x);
    let exp = (bits >> 23) & 0xffu32;
    let frac = bits & 0x7fffffu32;
    return exp == 0xffu32 && frac == 0u32;
}

pub fn length_squared(v: Vec2f) -> f32 {
    return v.x * v.x + v.y * v.y;
}

pub fn rotation_matrix(angle: f32) -> Mat2x2f {
    return Mat2x2::new(angle.cos(), -angle.sin(), angle.sin(), angle.cos());
}

pub fn scaled(v: Vec2f, zoom: f32) -> Vec2f {
    return v * 2.0f32.powf(zoom);
}

pub fn aspect(width: u32, height: u32) -> Vec2f {
    let mut aspect_scale = Vec2::new(1.0, 1.0);
    let aspect = width as f32 / height as f32;
    if aspect < 1.0 {
        aspect_scale.x = aspect;
    } else {
        aspect_scale.y = 1.0 / aspect;
    }
    return aspect_scale;
}

pub fn get_orbit_values(z_n: Vec2f) -> Vec4f {
    let len = length_squared(z_n);
    let abs = z_n.abs();
    return Vec4::new(
        len,
        (len - 2.0).abs(),
        (abs.x).min(abs.y),
        (abs.x + abs.y - 2.0).abs(),
    );
}

pub fn get_stripe_values(z_n: Vec2f) -> Vec4f {
    if z_n.x == 0.0 && z_n.y == 0.0 {
        return Vec4::splat(0.0);
    }
    let tan = z_n.x.atan2(z_n.y);
    return Vec4::new(
        0.5 + 0.5 * (5.0 * tan).sin(),
        0.5 + 0.5 * z_n.normalize().x,
        0.5 + 0.5 * z_n.normalize().y,
        0.0,
    );
}

pub fn step_frac(start_radius_squared: f32, end_radius_squared: f32) -> f32 {
    let start_log = start_radius_squared.ln();
    let end_log = end_radius_squared.ln();
    return -1.0 + (2.0 * end_log).log2() - (0.5 * start_log).log2();
}

pub const INTERPOLATION_CONSTANT: u32 = 0u32;
pub const INTERPOLATION_LINEAR: u32 = 1u32;
pub const INTERPOLATION_SMOOTHSTEP: u32 = 2u32;

pub fn mix_advanced(
    color_a: Vec3f,
    color_b: Vec3f,
    t: f32,
    color_space: u32,
    interpolation: u32,
) -> Vec3f {
    // input transform
    #[expect(unused_assignments)]
    let mut transformed_a = Vec3::splat(0.0);
    #[expect(unused_assignments)]
    let mut transformed_b = Vec3::splat(0.0);
    match color_space {
        x if x == COLOR_SPACE_LINEAR_SRGB => {
            transformed_a = color_a;
            transformed_b = color_b;
        }
        x if x == COLOR_SPACE_HSL => {
            transformed_a = linear_srgb_to_hsl(color_a);
            transformed_b = linear_srgb_to_hsl(color_b);
        }
        x if x == COLOR_SPACE_HSV => {
            transformed_a = linear_srgb_to_hsv(color_a);
            transformed_b = linear_srgb_to_hsv(color_b);
        }
        x if x == COLOR_SPACE_OKLAB => {
            transformed_a = linear_srgb_to_oklab(color_a);
            transformed_b = linear_srgb_to_oklab(color_b);
        }
        x if x == COLOR_SPACE_OKLCH => {
            transformed_a = linear_srgb_to_oklch(color_a);
            transformed_b = linear_srgb_to_oklch(color_b);
        }
        _ => {
            return Vec3::splat(0.0);
        }
    }

    #[expect(unused_assignments)]
    let mut transformed_output = Vec3::splat(0.0);
    match interpolation {
        x if x == INTERPOLATION_CONSTANT => {
            transformed_output = mix(transformed_b, transformed_a, step(t, 0.99f32));
        }
        x if x == INTERPOLATION_LINEAR => {
            transformed_output = mix(transformed_a, transformed_b, t);
        }
        x if x == INTERPOLATION_SMOOTHSTEP => {
            transformed_output = mix(transformed_a, transformed_b, smoothstep(0.0f32, 1.0f32, t));
        }
        _ => {
            return Vec3::new(1.0, 0.0, 1.0);
        }
    }

    match color_space {
        x if x == COLOR_SPACE_LINEAR_SRGB => {
            return transformed_output;
        }
        x if x == COLOR_SPACE_HSL => {
            return hsl_to_linear_srgb(transformed_output);
        }
        x if x == COLOR_SPACE_HSV => {
            return hsv_to_linear_srgb(transformed_output);
        }
        x if x == COLOR_SPACE_OKLAB => {
            return oklab_to_linear_srgb(transformed_output);
        }
        x if x == COLOR_SPACE_OKLCH => {
            return oklch_to_linear_srgb(transformed_output);
        }
        _ => {
            return Vec3::splat(0.0);
        }
    }
}

pub fn lerp(low: f32, high: f32, t: f32) -> f32 {
    return (t - low) / (high - low);
}

// included from https://github.com/KhronosGroup/ToneMapping/blob/b5a2eed5ddf6c2227090449399de9c7affb9e4c9/PBR_Neutral/pbrNeutral.glsl under the Apache 2.0 license
// Input color is non-negative and resides in the Linear Rec. 709 color space.
// Output color is also Linear Rec. 709, but in the [0, 1] range.

pub fn pbr_neutral_tone_mapping(in_color: Vec3f) -> Vec3f {
    let mut color = in_color;
    let start_compression = 0.8 - 0.04;
    let desaturation = 0.15;

    let min_rg = color.x.min(color.y);
    let x = min_rg.min(color.z);
    if x < 0.08 {
        color -= x - 6.25 * x * x;
    } else {
        color -= 0.04;
    }

    let max_rg = color.x.max(color.y);
    let peak = max_rg.max(color.z);
    if peak < start_compression {
        return color;
    }

    let d = 1. - start_compression;
    let new_peak = 1. - d * d / (peak + d - start_compression);
    color *= new_peak / peak;

    let g = 1. - 1. / (desaturation * (peak - new_peak) + 1.);
    return mix(color, Vec3::splat(new_peak), g);
}
