import super::utils::{hsv2rgb, rgb2hsv, isnan, isinf, debug, TAU};

// inputs
@group(0) @binding(0) var<storage> step_buffer : array<i32>;
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

struct Light {
    color: vec3f,
    strength: f32,
    direction: vec3f,
    // padding: f32
}

// note: the arrays must use a stride of 16 (e.g. vec4x)
struct ColorParams {
    saturation: f32,
    brightness: f32,
    color_frequency: f32,
    color_offset: f32,
    gradient_kind: u32,
    lighting_kind: u32,
    // padding: u32,
    // padding: u32,
    gradient: array<vec4f, 3>,
    color_layer_types: vec2u,
    light_layer_types: vec2u,
    color_strengths: array<vec4f, 2>,
    color_params: array<vec4f, 2>,
    light_strengths: array<vec4f, 2>,
    light_params: array<vec4f, 2>,
    lights: array<Light, 3>,
    overlays: Overlays,
}

struct RenderParams {
    width: u32,
    height: u32,
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
const LAYER_STRIPE = 5u;

const GRADIENT_FLAT = 0u;
const GRADIENT_PROCEDURAL = 1u;
const GRADIENT_MANUAL = 2u;
const GRADIENT_HUE = 3u;

const LIGHTING_FLAT = 0u;
const LIGHTING_GRADIENT = 1u;
const LIGHTING_REPEATING_GRADIENT = 2u;
const LIGHTING_SHADED = 3u;

fn calculate_lighting_layers(x: u32, y: u32, color_params: ColorParams) -> f32 {
    let pixel_index = x + y * render_params.width;
    var step = step_buffer[pixel_index];
    let orbits = orbit_buffer[pixel_index];
    let z = z_buffer[pixel_index];
    let dz = dz_buffer[pixel_index];
    let stripes = stripe_buffer[pixel_index];

    let r = length(z);
    let dr = length(dz);

    var smoothed_step = f32(step) + (1.0 - log(log(r)) / log(2.0));
    let internal = step < 0;
    if step < 0 {
        step = -step;
        smoothed_step = dz.x;
    }
    let distance_estimate = log(r) * r / dr;
    let scale = pow(2.0, -render_params.zoom);
    var brightness = 0.0;
    for (var i = 0u; i < 8; i += 1) {
        let layer_kind_big = color_params.light_layer_types[i / 4];
        let layer_kind = (layer_kind_big >> ((i % 4u) * 8u)) & 255u;
        let layer_strength = color_params.light_strengths[i/4][i%4];
        let layer_param = color_params.light_params[i/4][i%4];
        switch layer_kind {
            case LAYER_NONE {
                continue;
            }
            case LAYER_STEP {
                brightness += (log(f32(step)) * log(f32(step)) - layer_param) * 0.05 * layer_strength;
            }
            case LAYER_SMOOTH_STEP {
                if internal {
                    brightness += (smoothed_step - layer_param) * 0.3 * layer_strength;
                } else {
                    brightness += (log(smoothed_step) * log(smoothed_step) - layer_param) * 0.05 * layer_strength;
                }
            }
            case LAYER_DISTANCE {
                if !isinf(distance_estimate) && !isnan(distance_estimate) {
                    brightness += (-log(distance_estimate) + 5.0 + layer_param * 2.5) * layer_strength * 0.1;
                }
            }
            case LAYER_ORBIT_TRAP {
                let orbit = orbits[u32(layer_param)];
                brightness += (orbit - fract(layer_param)) * pow(2.0, layer_strength);
            }
            case LAYER_STRIPE {
                let stripe = stripes[u32(layer_param)];
                brightness += (stripe - fract(layer_param)) * layer_strength * layer_strength * layer_strength;
            }
            default {}
        }
    }
    return brightness;
}

@compute @workgroup_size(16, 16, 1)
fn main_color(@builtin(global_invocation_id) global_id: vec3<u32>) {

    if global_id.x >= render_params.width {
        return;
    }

    let pixel_index = global_id.x + global_id.y * render_params.width;
    var step = step_buffer[pixel_index];
    let orbits = orbit_buffer[pixel_index];
    let z = z_buffer[pixel_index];
    let dz = dz_buffer[pixel_index];
    let stripes = stripe_buffer[pixel_index];

    let r = length(z);
    let dr = length(dz);

    var smoothed_step = f32(step) + (1.0 - log2(log(r)));
    let distance_estimate = log(r) * r / dr;
    let scale = pow(2.0, -render_params.zoom);

    var color_params = external_coloring;
    let internal = step < 0;
    if step < 0 {
        step = -step;
        smoothed_step = dz.x;
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
                color_value += log(f32(step)) * log(f32(step)) * 0.05 * layer_strength;
            }
            case LAYER_SMOOTH_STEP {
                if internal {
                    color_value += (smoothed_step - layer_param) * 0.3 * layer_strength;
                } else {
                    color_value += (log(smoothed_step) * log(smoothed_step) - layer_param) * 0.05 * layer_strength;
                }
            }
            case LAYER_DISTANCE {
                if !isinf(distance_estimate) && !isnan(distance_estimate) {
                    color_value += (log(distance_estimate) + layer_param) * 0.1 * layer_strength;
                }
            }
            case LAYER_ORBIT_TRAP {
                let orbit = orbits[u32(layer_param)];
                color_value += orbit * pow(2.0, layer_strength);
            }
            case LAYER_STRIPE {
                let stripe = stripes[u32(layer_param)];
                color_value += (stripe - fract(layer_param)) * layer_strength * layer_strength * layer_strength;
            }
            default {}
        }
    }

    // then, turn the color value into a color
    var color = vec3f(0.0);
    switch color_params.gradient_kind {
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
                TAU * (
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
        case GRADIENT_HUE {
            color = hsv2rgb(vec3(fract(color_value * color_params.color_frequency + color_params.color_offset), color_params.gradient[0].x, color_params.gradient[0].y));
        }
        default {}
    }

    let lighting_value = 0.0;

    // next, calculate lighting
    var brightness = vec3(0.0);
    switch color_params.lighting_kind {
        case LIGHTING_FLAT {
            brightness = vec3(1.0);
        }
        case LIGHTING_GRADIENT {
            brightness = vec3(calculate_lighting_layers(global_id.x, global_id.y, color_params));
        }
        case LIGHTING_REPEATING_GRADIENT {
            brightness = vec3(cos(calculate_lighting_layers(global_id.x, global_id.y, color_params)) * 0.5 + 0.5);
        }
        case LIGHTING_SHADED {
            let point = calculate_lighting_layers(global_id.x, global_id.y, color_params);
            let plus_x = calculate_lighting_layers(global_id.x + 1, global_id.y, color_params);
            let plus_y = calculate_lighting_layers(global_id.x, global_id.y + 1, color_params);
            var aspect_scale = vec2<f32>(1.0, 1.0);
            let aspect = f32(render_params.width) / f32(render_params.height);
            if aspect< 1.0 {
                aspect_scale.x = aspect;
            } else {
                aspect_scale.y = 1.0 / aspect;
            }
            let normal = normalize(cross(vec3f(100.0 / f32(render_params.width) * aspect_scale.x, 0.0, plus_x - point), vec3f(0.0, 100.0 / f32(render_params.height) * aspect_scale.y, plus_y - point)));
            brightness += max(dot(normal, color_params.lights[0].direction), 0.0) * color_params.lights[0].strength * color_params.lights[0].color;
            brightness += max(dot(normal, color_params.lights[1].direction), 0.0) * color_params.lights[1].strength * color_params.lights[1].color;
            brightness += max(dot(normal, color_params.lights[2].direction), 0.0) * color_params.lights[2].strength * color_params.lights[2].color;
            brightness /= 3.0;
            if debug(global_id.x) {
                color = normalize(normal) * 0.5 + 0.5;
                brightness = vec3(1.0);
            }
        }
        default {}
    }
    let hsv_color = rgb2hsv(color);
    let b = length(brightness) * color_params.brightness / 1.73205080756887729353;
    color = hsv2rgb(vec3f(hsv_color.x, hsv_color.y * color_params.saturation, hsv_color.z * b));
    color *= brightness;
    // above 1.0 brightness, start fading to the light color
    color = mix(color, clamp(brightness, vec3(0.0), vec3(1.0)), clamp(b-1.0, 0.0, 1.0));

    // finally, check for ovelays
    if fract(color_params.overlays.iteration_outline_color.a) > 0.0 {
        let step_x = step_buffer[(global_id.x+1) + global_id.y * render_params.width];
        let step_y = step_buffer[global_id.x + (global_id.y+1) * render_params.width];
        let steps = i32(floor(color_params.overlays.iteration_outline_color.a));
        if abs(i32(step) / steps - i32(step_x) / steps)  == 1 || abs(i32(step) / steps - i32(step_y) / steps) == 1 {
            color = mix(color, color_params.overlays.iteration_outline_color.rgb, color_params.overlays.iteration_outline_color.a);
        }
    }
    if fract(color_params.overlays.set_outline_color.a) > 0.0 {
        let pixel_size = scale / f32(render_params.width);
        if distance_estimate < pixel_size * 0.03 * floor(color_params.overlays.set_outline_color.a) {
            color = mix(color, color_params.overlays.set_outline_color.rgb, fract(color_params.overlays.set_outline_color.a));
        }
    }
    
    textureStore(
        final_texture,
        vec2<i32>(i32(global_id.x), i32(global_id.y)),
        vec4<f32>(color, 1.0),
    );
}
