import package::utils::color_spaces::{
    COLOR_SPACE_HSL,
    COLOR_SPACE_HSV,
    COLOR_SPACE_LINEAR_SRGB,
    COLOR_SPACE_OKLAB,
    COLOR_SPACE_OKLCH,
    hsl_to_linear_srgb,
    hsv_to_linear_srgb,
    linear_srgb_to_hsl,
    linear_srgb_to_hsv,
    linear_srgb_to_oklab,
    linear_srgb_to_oklch,
    oklab_to_linear_srgb,
    oklch_to_linear_srgb,
};

const PI = 3.1415926535897932384626433832795028841971693993751058209749445923078164062862089986280348253421170679;
const TAU = 2.0 * PI;
const ESCAPE_RADIUS = 10000.0;
const EPSILON = 1.1920929E-17;
const FRACTEXP_SCALE_FACTOR = 10.0;
// flags
const STRIPES_ENABLED = 0x1u;
const TOTAL_ANGLE_ENABLED = 0x2u;
const ORBIT_ENABLED = 0x4u;
const DERIVATIVE_ENABLED = 0x8u;
const JULIA = 0x10000000u;

fn isnan(x: f32) -> bool {
    let bits = bitcast<u32>(x);
    let exp = (bits >> 23) & 0xffu;
    let frac = bits & 0x7fffffu;
    return exp == 0xffu && frac != 0u;
}

fn isinf(x: f32) -> bool {
    let bits = bitcast<u32>(x);
    let exp = (bits >> 23) & 0xffu;
    let frac = bits & 0x7fffffu;
    return exp == 0xffu && frac == 0u;
}

fn length_squared(v: vec2f) -> f32 {
    return v.x * v.x + v.y * v.y;
}

fn rotation_matrix(angle: f32) -> mat2x2f {
    return mat2x2(cos(angle), -sin(angle), sin(angle), cos(angle));
}

fn debug(x: u32) -> bool {
    return x < u32(f32(render_params.width) * 0.0);//render_params.debug_shutter);
}

fn aspect(width: u32, height: u32) -> vec2f {
    var aspect_scale = vec2<f32>(1.0, 1.0);
    let aspect = f32(width) / f32(height);
    if aspect < 1.0 {
        aspect_scale.x = aspect;
    } else {
        aspect_scale.y = 1.0 / aspect;
    }
    return aspect_scale;
}

fn get_orbit_values(z_n: vec2f) -> vec4f {
    return vec4f(
        length_squared(z_n),
        abs(length_squared(z_n)-2.0),
        min(abs(z_n.x), abs(z_n.y)),
        abs(abs(z_n.x) + abs(z_n.y)-2.0)
    );
}

fn get_stripe_values(z_n: vec2f) -> vec4f {
    if all(z_n == vec2f(0.0)) {
        return vec4(0.0);
    }
    return vec4(
        0.5+0.5*sin(5.0*atan2(z_n.x, z_n.y)),
        0.5+0.5*normalize(z_n).x,
        0.5+0.5*normalize(z_n).y,
        0.0
    );
}

fn step_frac(start_radius_squared: f32, end_radius_squared: f32) -> f32 {
    return -1.0 + log2(2.0 * log(end_radius_squared)) - log2(0.5 * log(start_radius_squared));
}

const INTERPOLATION_CONSTANT = 0u;
const INTERPOLATION_LINEAR = 1u;
const INTERPOLATION_SMOOTHSTEP = 2u;

fn mix_advanced(color_a: vec3f, color_b: vec3f, t: f32, color_space: u32, interpolation: u32) -> vec3f {
    // input transform
    var transformed_a: vec3f;
    var transformed_b: vec3f;
    switch color_space {
        case COLOR_SPACE_LINEAR_SRGB {
            transformed_a = color_a;
            transformed_b = color_b;
        }
        case COLOR_SPACE_HSL {
            transformed_a = linear_srgb_to_hsl(color_a);
            transformed_b = linear_srgb_to_hsl(color_b);
        }
        case COLOR_SPACE_HSV {
            transformed_a = linear_srgb_to_hsv(color_a);
            transformed_b = linear_srgb_to_hsv(color_b);
        }
        case COLOR_SPACE_OKLAB {
            transformed_a = linear_srgb_to_oklab(color_a);
            transformed_b = linear_srgb_to_oklab(color_b);
        }
        case COLOR_SPACE_OKLCH {
            transformed_a = linear_srgb_to_oklch(color_a);
            transformed_b = linear_srgb_to_oklch(color_b);
        }
        default { return vec3(0.0); }
    }

    var transformed_output: vec3f;
    switch interpolation {
        case INTERPOLATION_CONSTANT {
            transformed_output = mix(transformed_b, transformed_a, step(t, 0.99));
        }
        case INTERPOLATION_LINEAR {
            transformed_output = mix(transformed_a, transformed_b, t);
        }
        case INTERPOLATION_SMOOTHSTEP {
            transformed_output = mix(transformed_a, transformed_b, smoothstep(0.0, 1.0, t));
        }
        default { return vec3(1.0, 0.0, 1.0); }
    }

    switch color_space {
        case COLOR_SPACE_LINEAR_SRGB {
            return transformed_output;
        }
        case COLOR_SPACE_HSL {
            return hsl_to_linear_srgb(transformed_output);
        }
        case COLOR_SPACE_HSV {
            return hsv_to_linear_srgb(transformed_output);
        }
        case COLOR_SPACE_OKLAB {
            return oklab_to_linear_srgb(transformed_output);
        }
        case COLOR_SPACE_OKLCH {
            return oklch_to_linear_srgb(transformed_output);
        }
        default { return vec3(0.0); }
    }
}

fn lerp(low: f32, high: f32, t: f32) -> f32 {
    return (t - low) / (high - low);
}

// included from https://github.com/KhronosGroup/ToneMapping/blob/b5a2eed5ddf6c2227090449399de9c7affb9e4c9/PBR_Neutral/pbrNeutral.glsl under the Apache 2.0 license
// Input color is non-negative and resides in the Linear Rec. 709 color space.
// Output color is also Linear Rec. 709, but in the [0, 1] range.

fn PBRNeutralToneMapping( in_color: vec3f ) -> vec3f {
    var color = in_color;
    let startCompression = 0.8 - 0.04;
    let desaturation = 0.15;

    let x = min(color.r, min(color.g, color.b));
    if x < 0.08 {
        color -= x - 6.25 * x * x; 
    } else {
        color -= 0.04;
    }

    let peak = max(color.r, max(color.g, color.b));
    if (peak < startCompression) {return color;}

    let d = 1. - startCompression;
    let newPeak = 1. - d * d / (peak + d - startCompression);
    color *= newPeak / peak;

    let g = 1. - 1. / (desaturation * (peak - newPeak) + 1.);
    return mix(color, newPeak * vec3(1.0, 1.0, 1.0), g);
}
