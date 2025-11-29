const COLOR_SPACE_LINEAR_SRGB = 0u;
const COLOR_SPACE_HSL = 1u;
const COLOR_SPACE_HSV = 2u;
const COLOR_SPACE_OKLAB = 3u;
const COLOR_SPACE_OKLCH = 4u;

// Matrix values taken from the reference here:
// https://bottosson.github.io/posts/oklab/#converting-from-linear-srgb-to-oklab

const SRGB_TO_LMS_T = mat3x3f(
    0.4122214708, 0.5363325363, 0.0514459929,
    0.2119034982, 0.6806995451, 0.1073969566,
    0.0883024619, 0.2817188376, 0.6299787005,
);
const LMS_TO_OKLAB_T = mat3x3f(
    0.2104542553,  0.7936177850, -0.0040720468,
    1.9779984951, -2.4285922050,  0.4505937099,
    0.0259040371,  0.7827717662, -0.8086757660,
);

fn linear_srgb_to_oklab(rgb: vec3f) -> vec3f {
    var lms = transpose(SRGB_TO_LMS_T) * rgb;
    lms = pow(lms, vec3(1.0 / 3.0));
    return transpose(LMS_TO_OKLAB_T) * lms;
}

const OKLAB_TO_LMS_T = mat3x3f(
    1.0,  0.3963377774,  0.2158037573,
    1.0, -0.1055613458, -0.0638541728,
    1.0, -0.0894841775, -1.2914855480,
);
const LMS_TO_SRGB_T = mat3x3f(
     4.0767416621, -3.3077115913,  0.2309699292,
    -1.2684380046,  2.6097574011, -0.3413193965,
    -0.0041960863, -0.7034186147,  1.7076147010,
);

fn oklab_to_linear_srgb(lab: vec3f) -> vec3f {
    var lms = transpose(OKLAB_TO_LMS_T) * lab;
    lms = lms * lms * lms;
    return transpose(LMS_TO_SRGB_T) * lms;
}

fn oklab_to_oklch(c: vec3f) -> vec3f {
    let C = sqrt(c.y * c.y + c.z * c.z);
    let h = atan2(c.z, c.y);
    return vec3f(c.x, C, h);
}

fn oklch_to_oklab(c: vec3f) -> vec3f {
    let a = c.y * cos(c.z);
    let b = c.y * sin(c.z);
    return vec3f(c.x, a, b);
}

fn linear_srgb_to_oklch(c: vec3f) -> vec3f {
    return oklab_to_oklch(linear_srgb_to_oklab(c));
}

fn oklch_to_linear_srgb(c: vec3f) -> vec3f {
    return oklab_to_linear_srgb(oklch_to_oklab(c));
}

// converted from https://gist.github.com/unitycoder/aaf94ddfe040ec2da93b58d3c65ab9d9
// included under the MIT license

const HCV_EPSILON = 1e-10;
const HSL_EPSILON = 1e-10;
const HCY_EPSILON = 1e-10;

// Converts a value from linear RGB to HCV (Hue, Chroma, Value)
fn linear_srgb_to_hcv(rgb: vec3f) -> vec3f {
    // Based on work by Sam Hocevar and Emil Persson
    var P: vec4f;
    if rgb.g < rgb.b {
        P = vec4(rgb.bg, -1.0, 2.0/3.0);
    } else {
        P = vec4(rgb.gb, 0.0, -1.0/3.0);
    }
    var Q: vec4f;
    if rgb.r < P.x {
        Q = vec4(P.xyw, rgb.r);
    } else {
        Q = vec4(rgb.r, P.yzx);
    }
    let C = Q.x - min(Q.w, Q.y);
    let H = abs((Q.w - Q.y) / (6.0 * C + HCV_EPSILON) + Q.z);
    return vec3(H, C, Q.x);
}

// Converts from pure Hue to linear RGB
fn hue_to_rgb(hue: f32) -> vec3f {
    let R = abs(hue * 6.0 - 3.0) - 1.0;
    let G = 2.0 - abs(hue * 6.0 - 2.0);
    let B = 2.0 - abs(hue * 6.0 - 4.0);
    return saturate(vec3(R,G,B));
}

// Converts from HSV to linear RGB
fn hsv_to_linear_srgb(hsv: vec3f) -> vec3f {
    let rgb = hue_to_rgb(hsv.x);
    return ((rgb - 1.0) * hsv.y + 1.0) * hsv.z;
}

// Converts from HSL to linear RGB
fn hsl_to_linear_srgb(hsl: vec3f) -> vec3f {
    let rgb = hue_to_rgb(hsl.x);
    let C = (1.0 - abs(2.0 * hsl.z - 1.0)) * hsl.y;
    return (rgb - 0.5) * C + hsl.z;
}

// Converts from linear RGB to HSV
fn linear_srgb_to_hsv(rgb: vec3f) -> vec3f {
    let HCV = linear_srgb_to_hcv(rgb);
    let S = HCV.y / (HCV.z + HCV_EPSILON);
    return vec3(HCV.x, S, HCV.z);
}

// Converts from linear rgb to HSL
fn linear_srgb_to_hsl(rgb: vec3f) -> vec3f {
    let HCV = linear_srgb_to_hcv(rgb);
    let L = HCV.z - HCV.y * 0.5;
    let S = HCV.y / (1.0 - abs(L * 2.0 - 1.0) + HSL_EPSILON);
    return vec3(HCV.x, S, L);
}
