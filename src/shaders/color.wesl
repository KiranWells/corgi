import super::utils::{hsv2rgb, rgb2hsv, isnan, isinf};

// inputs
@group(0) @binding(0) var<storage> step_buffer : array<u32>;
@group(0) @binding(1) var<storage> orbit_buffer : array<vec4f>;
@group(0) @binding(2) var<storage> stripe_buffer : array<vec4f>;
@group(0) @binding(3) var<storage> z_buffer : array<vec2f>;
@group(0) @binding(4) var<storage> dz_buffer : array<vec2f>;

// texture
@group(1) @binding(0) var final_texture: texture_storage_2d<rgba8unorm, write>;

struct Overlays {
    iteration_outline_color: vec4<f32>,
    set_outline_color: vec4<f32>,
}

// note: the arrays must use a stride of 16 (e.g. vec4x)
struct ColorParams {
    saturation: f32,
    brightness: f32,
    color_frequency: f32,
    color_offset: f32,
    gradient_type: u32,
    // padding0: u32,
    // padding1: u32,
    // padding1: u32,
    gradient: array<vec4f, 3>,
    color_layer_types: vec4u,
    color_strengths: array<vec4f, 2>,
    color_params: array<vec4f, 2>,
    lighting_data: array<vec4u, 5>,
    lighting_type: u32,
    // padding2: u32,
    overlays: Overlays,
}

struct RenderParams {
    image_width: u32,
    max_step: u32,
    zoom: f32,
    misc: f32,
    debug_shutter: f32,
};
@group(2) @binding(0) var<uniform> external_coloring : ColorParams;
@group(2) @binding(1) var<uniform> internal_coloring : ColorParams;
@group(2) @binding(2) var<uniform> render_params : RenderParams;

const LAYER_NONE = 0u;
const LAYER_STEP = 1u;
const LAYER_SMOOTH_STEP = 2u;
const LAYER_DISTANCE = 3u;
const LAYER_ORBIT_TRAP = 4u;
const LAYER_NORMAL = 5u;
const LAYER_STRIPE = 6u;

const GRADIENT_FLAT = 0u;
const GRADIENT_PROCEDURAL = 1u;
const GRADIENT_MANUAL = 2u;

const LIGHTING_FLAT = 0u;
const LIGHTING_LAYERS = 1u;
const LIGHTING_DIFFUSE = 2u;

const PI = 3.1415926535897932384626433832795028841971693993751058209749445923078164062862089986280348253421170679;

@compute @workgroup_size(16, 16, 1)
fn main_color(@builtin(global_invocation_id) global_id: vec3<u32>) {

    if global_id.x >= render_params.image_width {
        return;
    }

    let pixel_index = global_id.x + global_id.y * render_params.image_width;
    let step = step_buffer[pixel_index];
    let orbits = orbit_buffer[pixel_index];
    let z = z_buffer[pixel_index];
    let dz = dz_buffer[pixel_index];
    let stripes = stripe_buffer[pixel_index];

    let r = length(z);
    let dr = length(dz);

    let smoothed_step = f32(step) + (1.0 - log(log(r)) / log(2.0));
    // distance estimation: ln(r) * r/dr
    let distance_estimate = log(r) * r / dr;
    let scale = pow(2.0, -render_params.zoom);

    var color_params = external_coloring;
    if step == render_params.max_step {
        color_params = internal_coloring;
    }

    // first, calculate color layers
    var color_value = 0.0;
    var dbg = 0.0;
    for (var i = 0u; i < 8u; i += 1) {
        let layer_kind_big = color_params.color_layer_types[i / 4];
        let layer_kind = (layer_kind_big >> ((i % 4u) * 8u)) & 255u;
        let layer_strength = color_params.color_strengths[i/4][i%4];
        let layer_param = color_params.color_params[i/4][i%4];
        switch layer_kind {
            case LAYER_NONE {
                continue;
            }
            case LAYER_STEP {
                color_value += log(f32(step)) * 0.1 * layer_strength;
            }
            case LAYER_SMOOTH_STEP {
                color_value += log(smoothed_step) * log(smoothed_step) * 0.1 * layer_strength;
            }
            case LAYER_DISTANCE {
                if !isinf(distance_estimate) && !isnan(distance_estimate) {
                    color_value += (log(distance_estimate) + layer_param) * 0.1 * layer_strength;
                }
            }
            case LAYER_ORBIT_TRAP {
                let orbit = orbits[u32(layer_param)];
                color_value += orbit * layer_strength;
            }
            case LAYER_NORMAL {
                color_value += (atan2(dz.x, dz.y) + layer_param) * layer_strength;
            }
            case LAYER_STRIPE {
                let stripe = stripes[u32(layer_param)];
                color_value += (stripe - fract(layer_param)) * (render_params.zoom + 10.0) * (render_params.zoom + 10.0) * 0.01 * layer_strength + fract(layer_param);
            }
            default {
            }
        }
    }

    // then, turn the color value into a color
    var color = vec3f(0.0);
    switch color_params.gradient_type {
        case GRADIENT_FLAT {
            let g = color_params.gradient;
            color = g[0].rgb;
        }
        case GRADIENT_PROCEDURAL {
            let g = color_params.gradient;
            let a = vec3f(g[0].rgb);
            let b = vec3f(g[0].a, g[1].rg);
            let c = vec3f(g[1].ba, g[2].r);
            let d = vec3f(g[2].gba);
            color = a + b * cos(
                2.0 * PI * (
                    c * color_value * color_params.color_frequency + color_params.color_offset + d
                )
            );
        }
        case GRADIENT_MANUAL {
            let g = color_params.gradient;
            let a = g[0].rgb;
            let a_stop = g[0].a;
            let b = g[1].rgb;
            let b_stop = g[1].a;
            let c = g[2].rgb;
            let c_stop = g[2].a;
            let frac = fract(color_value * color_params.color_frequency + color_params.color_offset);
            if frac > c_stop {
                let ratio = (frac - c_stop) / (a_stop - c_stop + 1.0);
                color = mix(c, a, ratio);
            } else if frac > b_stop {
                let ratio = (frac - b_stop) / (c_stop - b_stop);
                color = mix(b, c, ratio);
            } else if frac > a_stop {
                let ratio = (frac - a_stop) / (b_stop - a_stop);
                color = mix(a, b, ratio);
            } else {
                let ratio = (frac - c_stop + 1.0) / (a_stop - c_stop + 1.0);
                color = mix(c, a, ratio);
            }
        }
        default {}
    }

    // next, calculate lighting
    var brightness = 0.0;
    switch color_params.lighting_type {
        case LIGHTING_FLAT {
            brightness = 1.0;
        }
        case LIGHTING_LAYERS {
            for (var i = 0u; i < 8; i += 1) {
                let lighting_type_big = color_params.lighting_data[0][i / 4];
                let layer_kind = (lighting_type_big >> ((i % 4u) * 8)) & 255u;
                let layer_strength = bitcast<f32>(color_params.lighting_data[(i+2)/4][(i+2)%4]);
                let layer_param = bitcast<f32>(color_params.lighting_data[(i + 10)/4][(i + 10)%4]);
                switch layer_kind {
                    case LAYER_NONE {
                        continue;
                    }
                    case LAYER_STEP {
                        brightness += log(f32(step) + 1.0) * layer_strength;
                    }
                    case LAYER_SMOOTH_STEP {
                        brightness += log(smoothed_step + 1.0) * layer_strength;
                    }
                    case LAYER_DISTANCE {
                        if !isinf(distance_estimate) && !isnan(distance_estimate) {
                            brightness += (-log(distance_estimate) + layer_param * 2.5 - render_params.zoom * 0.5) * layer_strength * 0.1;
                        }
                    }
                    case LAYER_ORBIT_TRAP {
                        let orbit = orbits[u32(layer_param)];
                        brightness += (cos(orbit * 2.0 * PI) + 1.0) * 0.5 * layer_strength;
                    }
                    case LAYER_NORMAL {
                        let zx = z_buffer[global_id.x +1 + global_id.y * render_params.image_width];
                        let dzx = dz_buffer[global_id.x +1 + global_id.y * render_params.image_width];
                        let stepx = step_buffer[global_id.x +1 + global_id.y * render_params.image_width];
                        let rx = length(zx);
                        let drx = length(dzx);
                        let distance_estimatex = log(rx) * rx / drx;
                        let smoothed_stepx = f32(stepx) + (1.0 - log(log(rx)) / log(2.0));
                        let zy = z_buffer[global_id.x + (global_id.y+1) * render_params.image_width];
                        let dzy = dz_buffer[global_id.x + (global_id.y+1) * render_params.image_width];
                        let stepy = step_buffer[global_id.x + (global_id.y+1) * render_params.image_width];
                        let ry = length(zy);
                        let dry = length(dzy);
                        let distance_estimatey = log(ry) * ry / dry;
                        let smoothed_stepy = f32(stepy) + (1.0 - log(log(ry)) / log(2.0));
                        let normal = cross(vec3f(0.1, 0.0, smoothed_stepx - smoothed_step), vec3f(0.0, 0.1, smoothed_stepy - smoothed_step));
                        // let gradient = vec2(smoothed_step - smoothed_stepx, smoothed_step - smoothed_stepy);
                        // let gradient = vec2(distance_estimate - distance_estimatex, distance_estimate - distance_estimatey);
                        brightness += max(dot(normalize(normal), normalize(vec3f(1.0, 0.3, 1.0))), layer_param) * layer_strength;
                        brightness += max(dot(normalize(normal), normalize(vec3f(-0.3, 1.0, 0.5))), layer_param) * layer_strength;
    if global_id.x < u32(f32(render_params.image_width) * render_params.debug_shutter) {
    color = normalize(normal) * 0.5 + 0.5;
    brightness = 1.0;
    }
                    }
                    case LAYER_STRIPE {
                        let stripe = stripes[u32(layer_param)];
                        brightness += (stripe - fract(layer_param)) * (render_params.zoom + 10.0) * (render_params.zoom + 10.0) * 0.01 * layer_strength + fract(layer_param);
                    }
                    default {}
                }
            }
        }
        case LIGHTING_DIFFUSE {
            // TODO
        }
        default {}
    }
    let hsv_color = rgb2hsv(color);
    brightness *= color_params.brightness;
    color = hsv2rgb(vec3f(hsv_color.x, hsv_color.y * color_params.saturation, hsv_color.z * brightness));
    // above 1.0 brightness, start fading to white
    color = mix(color, vec3(1.0), clamp(brightness-1.0, 0.0, 1.0));

    // if global_id.x < u32(f32(render_params.image_width) * render_params.debug_shutter) {
    // // color = vec3(normalize(dz), length(dz));
    // color = vec3(stripes.xyz * 10.0 - render_params.misc);
    // }
    // finally, check for ovelays
    if color_params.overlays.iteration_outline_color.a > 0.0 {
        let step2 = step_buffer[(global_id.x+1) + global_id.y * render_params.image_width];
        let step3 = step_buffer[global_id.x + (global_id.y+1) * render_params.image_width];
        if abs(i32(step) - i32(step2)) == 1 || abs(i32(step) - i32(step3)) == 1 {
            color = mix(color, color_params.overlays.iteration_outline_color.rgb, color_params.overlays.iteration_outline_color.a);
        }
    }
    if color_params.overlays.set_outline_color.a > 0.0 {
        let pixel_size = scale / f32(render_params.image_width);
        if distance_estimate < pixel_size * 0.3 {
            color = mix(color, color_params.overlays.set_outline_color.rgb, color_params.overlays.set_outline_color.a);
        }
    }
    
    textureStore(
        final_texture,
        vec2<i32>(i32(global_id.x), i32(global_id.y)),
        vec4<f32>(color, 1.0),
    );
}
