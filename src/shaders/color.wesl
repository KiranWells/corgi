import super::utils::{
    debug,
    INTERPOLATION_LINEAR,
    INTERPOLATION_SMOOTHSTEP,
    isinf,
    isnan,
    lerp,
    MAX_GRADIENT_STOPS,
    mix_advanced,
    PBRNeutralToneMapping,
    TAU,
    tone_map,
};
import super::utils::color_spaces::{
    COLOR_SPACE_OKLAB,
    hsv_to_linear_srgb,
    linear_srgb_to_oklab,
    linear_srgb_to_oklch,
    oklab_to_linear_srgb,
    oklch_to_linear_srgb,
};

// inputs
@group(0) @binding(0) var<storage> step_buffer : array<i32>;
@group(0) @binding(1) var<storage> orbit_buffer : array<vec4f>;
@group(0) @binding(2) var<storage> stripe_buffer : array<vec4f>;
@group(0) @binding(3) var<storage> z_buffer : array<vec3f>;
@group(0) @binding(4) var<storage> dz_buffer : array<vec3f>;

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
    gradient_size: u32,
    lighting_kind: u32,
    // padding: u32,
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
@group(2) @binding(3) var<uniform> gradient : array<vec4f, MAX_GRADIENT_STOPS * 2>;

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
const GRADIENT_OKLCH = 4u;

const LIGHTING_FLAT = 0u;
const LIGHTING_GRADIENT = 1u;
const LIGHTING_REPEATING_GRADIENT = 2u;
const LIGHTING_SHADED = 3u;

fn calculate_lighting_layers(x: u32, y: u32, color_params: ColorParams) -> f32 {
    let pixel_index = x + y * render_params.width;
    var step = step_buffer[pixel_index];
    let orbits = orbit_buffer[pixel_index];
    let z = z_buffer[pixel_index].xy;
    let dz = dz_buffer[pixel_index].xy;
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
                brightness += (f32(step) - layer_param) * 0.05 * layer_strength;
            }
            case LAYER_SMOOTH_STEP {
                if internal {
                    brightness += (smoothed_step - layer_param) * 0.3 * layer_strength;
                } else {
                    brightness += (log(smoothed_step) - layer_param) * 0.4 * layer_strength;
                }
            }
            case LAYER_DISTANCE {
                if !isinf(distance_estimate) && !isnan(distance_estimate) {
                    let x = -log(distance_estimate / 10.0);
                    brightness += pow(x, layer_param + 3.75) * layer_strength * 0.002;
                }
            }
            case LAYER_ORBIT_TRAP {
                var orbit = orbits[u32(layer_param)];
                if internal {
                    let offset = 0.25*fract(layer_param);
                    brightness += (orbit - offset)/(0.25-offset) * pow(2.0, layer_strength-1.0);
                } else {
                    orbit = 10.0 - orbit;
                    let offset = 10.0*fract(layer_param);
                    brightness += (orbit - offset)/(10.0-offset) * pow(2.0, layer_strength-1.0);
                }
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
    let z = z_buffer[pixel_index].xy;
    let dz = dz_buffer[pixel_index].xy;
    let stripes = stripe_buffer[pixel_index];

    let r = length(z);
    let dr = length(dz);

    var smoothed_step = f32(step) + (1.0 - log2(log(r)));
    let distance_estimate = log(r) * r / dr;
    let scale = pow(2.0, -render_params.zoom);

    var color_params = external_coloring;
    let internal = step < 0;
    var gradient_offset = 0u;
    if internal {
        step = -step;
        smoothed_step = dz.x;
        color_params = internal_coloring;
        gradient_offset = external_coloring.gradient_size;
    }

    // first, calculate color layers
    var color_value = 0.0;
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
                color_value += f32(step) * 0.05 * layer_strength;
            }
            case LAYER_SMOOTH_STEP {
                if internal {
                    color_value += (smoothed_step - layer_param) * 0.3 * layer_strength;
                } else {
                    color_value += (log(smoothed_step) * log(smoothed_step) - layer_param) * 0.1 * layer_strength;
                }
            }
            case LAYER_DISTANCE {
                if !isinf(distance_estimate) && !isnan(distance_estimate) {
                    color_value += (log(distance_estimate / (layer_param + 1.0))) * 0.1 * layer_strength;
                }
            }
            case LAYER_ORBIT_TRAP {
                var orbit = orbits[u32(layer_param)];
                if orbit > 2.0 {
                    orbit = log(orbit / 2.0) + 2.0;
                }
                color_value += orbit * 0.1 * pow(2.0, layer_strength) + layer_param;
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
            color = gradient[0 + gradient_offset].rgb;
        }
        case GRADIENT_PROCEDURAL {
            let a = vec3f(gradient[0 + gradient_offset].rgb);
            let b = vec3f(gradient[1 + gradient_offset].rgb);
            let c = vec3f(gradient[2 + gradient_offset].rgb);
            let d = vec3f(gradient[3 + gradient_offset].rgb);
            color = a + b * cos(
                TAU * (
                    c * color_value * color_params.color_frequency + color_params.color_offset + d
                )
            );
        }
        case GRADIENT_MANUAL {
            let frac = fract(color_value * color_params.color_frequency + color_params.color_offset);
            for (var i = 0u; i < color_params.gradient_size; i+=1) {
                let stop = gradient[i + gradient_offset];
                let stop_position = fract(stop.w) / 0.999;
                let color_space = COLOR_SPACE_OKLAB;
                let interpolation = u32(floor(stop.w));
                if frac < stop_position {
                    if i == 0 {
                        let prev_stop = gradient[color_params.gradient_size - 1 + gradient_offset];
                        let t = lerp(fract(prev_stop.w) - 1.0, stop_position, frac);
                        color = mix_advanced(prev_stop.rgb, stop.rgb, t, color_space, interpolation);
                    } else {
                        let prev_stop = gradient[i - 1 + gradient_offset];
                        let t = lerp(fract(prev_stop.w), stop_position, frac);
                        color = mix_advanced(prev_stop.rgb, stop.rgb, t, color_space, interpolation);
                    }
                    break;
                }
                if i == color_params.gradient_size - 1 {
                    let next_stop = gradient[0 + gradient_offset];
                    let t = lerp(stop_position, fract(next_stop.w) + 1.0, frac);
                    color = mix_advanced(stop.rgb, next_stop.rgb, t, color_space, u32(floor(next_stop.w)));
                }
            }
            color = color;
        }
        case GRADIENT_HUE {
            color = hsv_to_linear_srgb(vec3(
                fract(color_value * color_params.color_frequency + color_params.color_offset),
                gradient[0 + gradient_offset].x,
                gradient[0 + gradient_offset].y,
            ));
        }
        case GRADIENT_OKLCH {
            color = oklch_to_linear_srgb(vec3(
                gradient[0 + gradient_offset].x,
                gradient[0 + gradient_offset].y,
                fract(color_value * color_params.color_frequency + color_params.color_offset) * TAU,
            ));
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
            let normal = normalize(cross(
                vec3f(100.0 / f32(300) * aspect_scale.x, 0.0, plus_x - point),
                vec3f(0.0, 100.0 / f32(300) * aspect_scale.y, plus_y - point),
            ));
            for (var i = 0u; i < 3; i+=1) {
                let light = color_params.lights[i];
                brightness += max(dot(normal, light.direction), 0.0) * light.strength * light.color;
            }
            if debug(global_id.x) {
                color = normal * 0.5 + 0.5;
                brightness = vec3(1.0);
            }
        }
        default {}
    }
    // lighting calculations work best in RGB
    color *= brightness;
    // Adjust brightness, tone map, then adjust saturation.
    // Saturation needs to be done after tone mapping to
    // prevent desaturation in colors that exceed 1.0 in RGB
    // after saturation (which is not intuitive).
    color = linear_srgb_to_oklch(color);
    color.x *= color_params.brightness;
    color = oklch_to_linear_srgb(color);
    color = PBRNeutralToneMapping(color);
    color = linear_srgb_to_oklch(color);
    color.y = saturate(color.y * color_params.saturation);
    color = oklch_to_linear_srgb(color);

    // finally, check for ovelays
    if fract(color_params.overlays.iteration_outline_color.a) > 0.0 {
        let step_x = step_buffer[(global_id.x+1) + global_id.y * render_params.width];
        let step_y = step_buffer[global_id.x + (global_id.y+1) * render_params.width];
        let steps = i32(floor(color_params.overlays.iteration_outline_color.a));
        if abs(i32(step) / steps - i32(step_x) / steps)  == 1 || abs(i32(step) / steps - i32(step_y) / steps) == 1 {
            color = mix(color, color_params.overlays.iteration_outline_color.rgb, fract(color_params.overlays.iteration_outline_color.a));
        }
    }
    if fract(color_params.overlays.set_outline_color.a) > 0.0 {
        let frac = 1.0 - clamp(distance_estimate / 0.00005 / floor(color_params.overlays.set_outline_color.a), 0.0,1.0);
        color = mix(color, color_params.overlays.set_outline_color.rgb, fract(color_params.overlays.set_outline_color.a) * frac);
    }
    
    textureStore(
        final_texture,
        vec2<i32>(i32(global_id.x), i32(global_id.y)),
        vec4<f32>(color, 1.0),
    );
}
