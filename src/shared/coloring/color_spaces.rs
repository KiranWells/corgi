use crate::shared::wgsl_primitives::*;

pub const COLOR_SPACE_LINEAR_SRGB: u32 = 0u32;
pub const COLOR_SPACE_HSL: u32 = 1u32;
pub const COLOR_SPACE_HSV: u32 = 2u32;
pub const COLOR_SPACE_OKLAB: u32 = 3u32;
pub const COLOR_SPACE_OKLCH: u32 = 4u32;

// Matrix values taken from the reference here:
// https://bottosson.github.io/posts/oklab/#converting-from-linear-srgb-to-oklab

const SRGB_TO_LMS_T: Mat3x3f = Mat3x3::new(
    0.41222146,
    0.53633255,
    0.051445995,
    0.2119035,
    0.6806995,
    0.10739696,
    0.08830246,
    0.28171885,
    0.6299787,
);
const LMS_TO_OKLAB_T: Mat3x3f = Mat3x3::new(
    0.21045426,
    0.7936178,
    -0.004072047,
    1.9779985,
    -2.4285922,
    0.4505937,
    0.025904037,
    0.78277177,
    -0.80867577,
);

pub fn linear_srgb_to_oklab(rgb: Vec3f) -> Vec3f {
    let mut lms: Vec3f = SRGB_TO_LMS_T.transpose() * rgb;
    let third = Vec3::splat(1.0 / 3.0);
    lms = lms.powf(third);
    return LMS_TO_OKLAB_T.transpose() * lms;
}

const OKLAB_TO_LMS_T: Mat3x3f = Mat3x3::new(
    1.0,
    0.39633778,
    0.21580376,
    1.0,
    -0.105561346,
    -0.06385417,
    1.0,
    -0.08948418,
    -1.2914855,
);
const LMS_TO_SRGB_T: Mat3x3f = Mat3x3::new(
    4.0767417,
    -3.3077116,
    0.23096994,
    -1.268438,
    2.6097574,
    -0.34131938,
    -0.0041960863,
    -0.7034186,
    1.7076147,
);

pub fn oklab_to_linear_srgb(lab: Vec3f) -> Vec3f {
    let mut lms = OKLAB_TO_LMS_T.transpose() * lab;
    lms = lms * lms * lms;
    return LMS_TO_SRGB_T.transpose() * lms;
}

pub fn oklab_to_oklch(oklab: Vec3f) -> Vec3f {
    let c = (oklab.y * oklab.y + oklab.z * oklab.z).sqrt();
    let h = oklab.z.atan2(oklab.y);
    return Vec3::new(oklab.x, c, h);
}

pub fn oklch_to_oklab(c: Vec3f) -> Vec3f {
    let a = c.y * c.z.cos();
    let b = c.y * c.z.sin();
    return Vec3::new(c.x, a, b);
}

pub fn linear_srgb_to_oklch(c: Vec3f) -> Vec3f {
    return oklab_to_oklch(linear_srgb_to_oklab(c));
}

pub fn oklch_to_linear_srgb(c: Vec3f) -> Vec3f {
    return oklab_to_linear_srgb(oklch_to_oklab(c));
}

// converted from https://gist.github.com/unitycoder/aaf94ddfe040ec2da93b58d3c65ab9d9
// included under the MIT license

const HCV_EPSILON: f32 = 1e-10;
const HSL_EPSILON: f32 = 1e-10;

// Converts a value from linear RGB to HCV (Hue, Chroma, Value)
pub fn linear_srgb_to_hcv(rgb: Vec3f) -> Vec3f {
    // Based on work by Sam Hocelet mut and Emil Persson
    #[expect(unused_assignments)]
    let mut p = Vec4::splat(0.0);
    if rgb.y < rgb.z {
        p = Vec4::new(rgb.z, rgb.y, -1.0, 2.0 / 3.0);
    } else {
        p = Vec4::new(rgb.y, rgb.z, 0.0, -1.0 / 3.0);
    }
    #[expect(unused_assignments)]
    let mut q = Vec4::splat(0.0);
    if rgb.x < p.x {
        q = Vec4::new(p.x, p.y, p.w, rgb.x);
    } else {
        q = Vec4::new(rgb.x, p.y, p.z, p.x);
    }
    let c = q.x - q.w.min(q.y);
    let signed_h = (q.w - q.y) / (6.0 * c + HCV_EPSILON) + q.z;
    let h = signed_h.abs();
    return Vec3::new(h, c, q.x);
}

// Converts from pure Hue to linear RGB
pub fn hue_to_rgb(hue: f32) -> Vec3f {
    let r = (hue * 6.0 - 3.0).abs() - 1.0;
    let g = 2.0 - (hue * 6.0 - 2.0).abs();
    let b = 2.0 - (hue * 6.0 - 4.0).abs();
    let rgb = Vec3::new(r, g, b);
    return rgb.saturate();
}

// Converts from HSV to linear RGB
pub fn hsv_to_linear_srgb(hsv: Vec3f) -> Vec3f {
    let rgb = hue_to_rgb(hsv.x);
    return ((rgb - 1.0f32) * hsv.y + 1.0) * hsv.z;
}

// Converts from HSL to linear RGB
pub fn hsl_to_linear_srgb(hsl: Vec3f) -> Vec3f {
    let rgb = hue_to_rgb(hsl.x);
    let c = (1.0 - (2.0 * hsl.z - 1.0).abs()) * hsl.y;
    return (rgb - 0.5f32) * c + hsl.z;
}

// Converts from linear RGB to HSV
pub fn linear_srgb_to_hsv(rgb: Vec3f) -> Vec3f {
    let hcv = linear_srgb_to_hcv(rgb);
    let s = hcv.y / (hcv.z + HCV_EPSILON);
    return Vec3::new(hcv.x, s, hcv.z);
}

// Converts from linear rgb to HSL
pub fn linear_srgb_to_hsl(rgb: Vec3f) -> Vec3f {
    let hcv = linear_srgb_to_hcv(rgb);
    let l = hcv.z - hcv.y * 0.5;
    let s = hcv.y / (1.0 - (l * 2.0 - 1.0).abs() + HSL_EPSILON);
    return Vec3::new(hcv.x, s, l);
}
