import super::utils::{hsl2rgb, isnan, isinf};

// inputs
@group(0) @binding(0) var<storage> intermediate_step : array<u32>;
@group(0) @binding(1) var<storage> orbit_trap : array<f32>;
@group(0) @binding(2) var<storage> intermediate_r : array<f32>;
@group(0) @binding(3) var<storage> intermediate_dr : array<f32>;

// texture
@group(1) @binding(0) var final_texture: texture_storage_2d<rgba8unorm, write>;

struct ColoringParams {
    image_width: u32,
    max_step: u32,
    zoom: f32,
    saturation: f32,
    color_frequency: f32,
    color_offset: f32,
    glow_spread: f32,
    glow_intensity: f32,
    brightness: f32,
    internal_brightness: f32,
    misc: f32,
    debug_shutter: f32,
};
@group(2) @binding(0) var<uniform> params : ColoringParams;


@compute @workgroup_size(16, 16, 1)
fn main_color(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let PI = 3.1415926535897932384626433832795028841971693993751058209749445923078164062862089986280348253421170679;
    let step = intermediate_step[global_id.x + global_id.y * params.image_width];
    let orbit = orbit_trap[global_id.x + global_id.y * params.image_width];
    let r = intermediate_r[global_id.x + global_id.y * params.image_width];
    var dr = intermediate_dr[global_id.x + global_id.y * params.image_width];

    if global_id.x >= params.image_width {
        return;
    }

    // distance estimation: ln(r) * r/dr
    let distance_estimate = log(r) * r / dr;
    // a glow effect based on distance estimation
    var glow = (-log(distance_estimate) + params.glow_spread * 2.5 - params.zoom * 0.5) * params.glow_intensity * 0.1;
    if params.glow_intensity == 0.0 || isinf(glow) || isnan(glow) {
        glow = 0.5;
    }
    // a smoothed version of the iteration count: step + (1 - ln(ln(r)) / ln(2))
    let smoothed_step = f32(step) + (1.0 - log(log(r)) / log(2.0));
    var hsl_color: vec3<f32> = vec3<f32>(0.0, 1.0, 0.0);
    if step == params.max_step {
        hsl_color = vec3<f32>(
            0.0,
            0.0,
            orbit * params.brightness * pow(2.0, params.internal_brightness)
        );
    } else {
        hsl_color = vec3<f32>(
            fract(log(smoothed_step) * log(smoothed_step) * params.color_frequency * 0.1 - params.color_offset),
            (params.saturation * (1.0 - (glow * glow))),
            glow * params.brightness
        );
    }
    if global_id.x < u32(f32(params.image_width) * params.debug_shutter) {
        textureStore(
            final_texture,
            vec2<i32>(i32(global_id.x), i32(global_id.y)),
            vec4<f32>(
                vec3<f32>(1.0 - distance_estimate * 1000 * params.misc * params.zoom, f32(isnan(distance_estimate)), f32(isinf(distance_estimate))),
                1.0
            )
        );
    } else {
        textureStore(
            final_texture,
            vec2<i32>(i32(global_id.x), i32(global_id.y)),
            vec4<f32>(
                hsl2rgb(hsl_color),
                1.0
            )
        );
    }
}
