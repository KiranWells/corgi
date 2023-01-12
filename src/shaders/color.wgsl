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
};
@group(2) @binding(0) var<uniform> params : ColoringParams;

fn hsl2rgb(hsv: vec3<f32>) -> vec3<f32> {
    var rgb: vec3<f32>;

    let i = floor(hsv.x * 6.);
    let f = hsv.x * 6. - i;
    let p = hsv.z * (1. - hsv.y);
    let q = hsv.z * (1. - f * hsv.y);
    let t = hsv.z * (1. - (1. - f) * hsv.y);

    switch(i32((i % 6.0))){
        case 0: {rgb = vec3<f32>(hsv.z, t, p);}
        case 1: {rgb = vec3<f32>(q, hsv.z, p);}
        case 2: {rgb = vec3<f32>(p, hsv.z, t);}
        case 3: {rgb = vec3<f32>(p, q, hsv.z);}
        case 4: {rgb = vec3<f32>(t, p, hsv.z);}
        case 5: {rgb = vec3<f32>(hsv.z, p, q);}
        default: {rgb = vec3<f32>(0.0, 0.0, 0.0);}
    }

    return rgb;
}

@compute @workgroup_size(16, 16, 1)
fn main_color(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let PI = 3.1415926535897932384626433832795028841971693993751058209749445923078164062862089986280348253421170679;
    let step = intermediate_step[global_id.x + global_id.y * params.image_width];
    let orbit = orbit_trap[global_id.x + global_id.y * params.image_width];
    let r = intermediate_r[global_id.x + global_id.y * params.image_width];
    let dr = intermediate_dr[global_id.x + global_id.y * params.image_width];

    if global_id.x >= params.image_width {
        return;
    }
    // let dr = dr / r / r;

    // distance estimation: 0.5 * ln(r) * r/dr
    let distance_estimate = 0.5 * log(r) * r * r / dr;
    // a glow effect based on distance estimation
    var glow = (-log(distance_estimate) - params.zoom + params.glow_spread) * params.glow_intensity * 0.08;
    if params.glow_intensity == 0.0 {
        glow = 0.5;
    }
    // a smoothed version of the iteration count: step + (1 - ln(ln(r)) / ln(2))
    let smoothed_step = f32(step) + (1.0 - log(log(r)) / log(2.0));
    var hsl_color: vec3<f32> = vec3<f32>(0.0, 1.0, 0.0);
    if step == params.max_step {
        hsl_color = vec3<f32>(
            0.0,
            0.0,
            orbit * params.brightness * params.internal_brightness
        );
    } else {
        hsl_color = vec3<f32>(
            sin(log(smoothed_step) * params.color_frequency - params.color_offset * 2.0 * PI) * 0.5 + 0.5,
            (params.saturation * (1.0 - (glow * glow))),
            glow * params.brightness + params.misc
        );
    }
    // hsl_color = vec3<f32>(1.0, sin(smoothed_step / 30.0) * 0.5 + 0.5, 0.0);
    // hsl_color = vec3<f32>(1.0, smoothed_step / f32(params.max_step), 0.0);
    // hsl_color = vec3<f32>(1.0, smoothed_step / f32(params.max_step), 0.0);
    textureStore(final_texture, vec2<i32>(i32(global_id.x), i32(global_id.y)), vec4<f32>(hsl2rgb(hsl_color), 1.0));
    // if global_id.x < params.image_width / 3u {
    //     textureStore(final_texture, vec2<i32>(i32(global_id.x), i32(global_id.y)), vec4<f32>(vec3<f32>(clamp(0.001 * dr * 0.001 * params.misc, 0.0, 1.0)), 1.0));
    // } else if global_id.x < params.image_width / 3u * 2u {
    //     textureStore(final_texture, vec2<i32>(i32(global_id.x), i32(global_id.y)), vec4<f32>(vec3<f32>(clamp(0.001 * r * params.misc, 0.0, 1.0)), 1.0));
    // } else {
    //     textureStore(final_texture, vec2<i32>(i32(global_id.x), i32(global_id.y)), vec4<f32>(hsl2rgb(hsl_color), 1.0));
    // }
}