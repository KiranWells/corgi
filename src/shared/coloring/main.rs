// #[host]
use serde::{Deserialize, Serialize};

// This compute shader takes the raw fractal data and
// converts it into colors in the final image texture.
use super::super::utils::{TAU, isinf, isnan, lerp, mix_advanced, pbr_neutral_tone_mapping};
use super::color_spaces::{
    COLOR_SPACE_OKLAB, hsv_to_linear_srgb, linear_srgb_to_oklch, oklch_to_linear_srgb,
};
use crate::shared::wgsl_primitives::*;

pub const MAX_GRADIENT_STOPS: usize = 50_usize;
pub const MAX_LAYERS: usize = 8_usize;
pub const MAX_LIGHTS: usize = 3_usize;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct Overlays {
    pub iteration_outline: Vec4<f32>,
    pub set_outline: Vec4<f32>,
}

#[derive(
    Clone, Copy, Debug, Deserialize, Serialize, PartialEq, bytemuck::Pod, bytemuck::Zeroable,
)]
#[repr(C)]
pub struct Light {
    pub color: Vec3f,
    pub strength: f32,
    pub direction: Vec3f,
    /// Included for compatibility with GPU binary format
    #[serde(skip)]
    pub _padding: f32,
}

/// The GPU-safe version of coloring data. This is sent as a uniform
/// to the compute shader.
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
// note: the arrays must use a stride of 16 (e.g. Vec4x)
pub struct ColorParams {
    pub saturation: f32,
    pub brightness: f32,
    pub color_frequency: f32,
    pub color_offset: f32,
    pub gradient_kind: u32,
    pub gradient_size: u32,
    pub lighting_kind: u32,
    pub padding: u32,
    // Note: these types need to be adjusted when MAX_LAYERS changes;
    //   an array is not used for alignment reasons.
    pub color_layer_types: Vec2u,
    pub light_layer_types: Vec2u,
    pub color_strengths: [Vec4f; MAX_LAYERS / 4],
    pub color_params: [Vec4f; MAX_LAYERS / 4],
    pub light_strengths: [Vec4f; MAX_LAYERS / 4],
    pub light_params: [Vec4f; MAX_LAYERS / 4],
    pub lights: [Light; MAX_LIGHTS],
    pub overlays: Overlays,
}

/// Image parameters used by the color shader.
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct RenderParams {
    pub width: u32,
    pub height: u32,
}

const LAYER_NONE: u32 = 0u32;
const LAYER_STEP: u32 = 1u32;
const LAYER_SMOOTH_STEP: u32 = 2u32;
const LAYER_DISTANCE: u32 = 3u32;
const LAYER_ORBIT_TRAP: u32 = 4u32;
const LAYER_STRIPE: u32 = 5u32;

const GRADIENT_FLAT: u32 = 0u32;
const GRADIENT_PROCEDURAL: u32 = 1u32;
const GRADIENT_MANUAL: u32 = 2u32;
const GRADIENT_HUE: u32 = 3u32;
const GRADIENT_OKLCH: u32 = 4u32;

const LIGHTING_FLAT: u32 = 0u32;
const LIGHTING_GRADIENT: u32 = 1u32;
const LIGHTING_REPEATING_GRADIENT: u32 = 2u32;
const LIGHTING_SHADED: u32 = 3u32;

// This function is similar, but not exactly identical to the color layer
// calculation logic. The differences are designed to make the color
// and lighting parameters more intuitive.
#[expect(clippy::too_many_arguments)]
fn calculate_lighting_layers(
    location: Vec2u,
    render_params: RenderParams,
    color_params: ColorParams,
    step_buffer: &[i32],
    orbit_buffer: &[Vec4f],
    stripe_buffer: &[Vec4f],
    z_buffer: &[Vec4f],
    dz_buffer: &[Vec4f],
) -> f32 {
    let pixel_index = (location.x + location.y * render_params.width) as usize;
    let mut step = step_buffer[pixel_index];
    let orbits = orbit_buffer[pixel_index];
    let z = z_buffer[pixel_index].xy();
    let dz = dz_buffer[pixel_index].xy();
    let stripes = stripe_buffer[pixel_index];

    let r = z.length();
    let dr = dz.length();

    let mut smoothed_step = step as f32 + (1.0 - r.ln().ln() / 2.0_f32.ln());
    let internal = step < 0;
    if step < 0 {
        step = -step;
        smoothed_step = dz.x;
    }
    let distance_estimate = r.ln() * r / dr;
    let mut brightness = 0.0;
    for i in 0_usize..MAX_LAYERS {
        let layer_kind_big = color_params.light_layer_types[i / 4];
        let layer_kind = (layer_kind_big >> ((i % 4) * 8)) & 255;
        let layer_strength = color_params.light_strengths[i / 4][i % 4];
        let layer_param = color_params.light_params[i / 4][i % 4];
        match layer_kind {
            n if n == LAYER_NONE => {
                continue;
            }
            n if n == LAYER_STEP => {
                brightness += (step as f32 - layer_param) * 0.05 * layer_strength;
            }
            n if n == LAYER_SMOOTH_STEP => {
                if internal {
                    brightness += (smoothed_step - layer_param) * 0.3 * layer_strength;
                } else {
                    brightness += (smoothed_step.ln() - layer_param) * 0.4 * layer_strength;
                }
            }
            n if n == LAYER_DISTANCE => {
                if !isinf(distance_estimate) && !isnan(distance_estimate) {
                    let x = -(distance_estimate / 10.0_f32).ln();
                    brightness += x.powf(layer_param + 3.75) * layer_strength * 0.002;
                }
            }
            n if n == LAYER_ORBIT_TRAP => {
                let mut orbit = orbits[layer_param as usize];
                if internal {
                    let offset = 0.25 * layer_param.fract();
                    brightness +=
                        (orbit - offset) / (0.25 - offset) * 2.0f32.powf(layer_strength - 1.0);
                } else {
                    orbit = 10.0 - orbit;
                    let offset = 10.0 * layer_param.fract();
                    brightness +=
                        (orbit - offset) / (10.0 - offset) * 2.0f32.powf(layer_strength - 1.0);
                }
            }
            n if n == LAYER_STRIPE => {
                let stripe = stripes[layer_param as usize];
                brightness += (stripe - layer_param.fract())
                    * layer_strength
                    * layer_strength
                    * layer_strength;
            }
            _ => {}
        }
    }
    return brightness;
}

#[expect(clippy::too_many_arguments)]
fn calc_normal(
    global_id_in: Vec3u,
    render_params: RenderParams,
    color_params: ColorParams,
    step_buffer: &[i32],
    orbit_buffer: &[Vec4f],
    stripe_buffer: &[Vec4f],
    z_buffer: &[Vec4f],
    dz_buffer: &[Vec4f],
) -> Vec3f {
    let mut global_id = global_id_in;
    if global_id.x == 0 {
        global_id.x += 1;
    }
    if global_id.y == 0 {
        global_id.y += 1;
    }
    if global_id.x >= render_params.width - 1 {
        global_id.x -= 1;
    }
    if global_id.y >= render_params.height - 1 {
        global_id.y -= 1;
    }
    let plus_x = calculate_lighting_layers(
        Vec2::new(global_id.x + 1, global_id.y),
        render_params,
        color_params,
        // #[host]
        step_buffer,
        // #[host]
        orbit_buffer,
        // #[host]
        stripe_buffer,
        // #[host]
        z_buffer,
        // #[host]
        dz_buffer,
    );
    let plus_y = calculate_lighting_layers(
        Vec2::new(global_id.x, global_id.y + 1),
        render_params,
        color_params,
        // #[host]
        step_buffer,
        // #[host]
        orbit_buffer,
        // #[host]
        stripe_buffer,
        // #[host]
        z_buffer,
        // #[host]
        dz_buffer,
    );
    let sub_x = calculate_lighting_layers(
        Vec2::new(global_id.x - 1, global_id.y),
        render_params,
        color_params,
        // #[host]
        step_buffer,
        // #[host]
        orbit_buffer,
        // #[host]
        stripe_buffer,
        // #[host]
        z_buffer,
        // #[host]
        dz_buffer,
    );
    let sub_y = calculate_lighting_layers(
        Vec2::new(global_id.x, global_id.y - 1),
        render_params,
        color_params,
        // #[host]
        step_buffer,
        // #[host]
        orbit_buffer,
        // #[host]
        stripe_buffer,
        // #[host]
        z_buffer,
        // #[host]
        dz_buffer,
    );
    let delta_xy = 1.0;
    let x = Vec3f::new(delta_xy, 0.0, plus_x - sub_x);
    let y = Vec3f::new(0.0, delta_xy, plus_y - sub_y);
    return x.cross(y).normalize();
}

#[expect(clippy::too_many_arguments)]
pub fn evaluate_style(
    global_id: Vec3u,
    render_params: RenderParams,
    internal_coloring: ColorParams,
    external_coloring: ColorParams,
    gradient: [Vec4f; MAX_GRADIENT_STOPS * 2],
    step_buffer: &[i32],
    orbit_buffer: &[Vec4f],
    stripe_buffer: &[Vec4f],
    z_buffer: &[Vec4f],
    dz_buffer: &[Vec4f],
) -> Vec3f {
    let pixel_index = (global_id.x + global_id.y * render_params.width) as usize;
    let mut step = step_buffer[pixel_index];
    let orbits = orbit_buffer[pixel_index];
    let z = z_buffer[pixel_index].xy();
    let dz = dz_buffer[pixel_index].xy();
    let stripes = stripe_buffer[pixel_index];

    let r = z.length();
    let dr = dz.length();

    let mut smoothed_step = step as f32 + (1.0 - r.ln().log2());
    let distance_estimate = r.ln() * r / dr;

    let mut color_params = external_coloring;
    let internal = step < 0;
    let mut gradient_offset = 0_usize;
    if internal {
        step = -step;
        smoothed_step = dz.x;
        color_params = internal_coloring;
        gradient_offset = external_coloring.gradient_size as usize;
    }

    // first, calculate color layers
    let mut color_value = 0.0;
    for i in 0_usize..MAX_LAYERS {
        let layer_kind_big = color_params.color_layer_types[i / 4];
        let layer_kind = (layer_kind_big >> ((i % 4) * 8)) & 255;
        let layer_strength = color_params.color_strengths[i / 4][i % 4];
        let layer_param = color_params.color_params[i / 4][i % 4];
        match layer_kind {
            n if n == LAYER_NONE => {
                continue;
            }
            n if n == LAYER_STEP => {
                color_value += step as f32 * 0.05 * layer_strength;
            }
            n if n == LAYER_SMOOTH_STEP => {
                if internal {
                    color_value += (smoothed_step - layer_param) * 0.3 * layer_strength;
                } else {
                    color_value += (smoothed_step.ln() * smoothed_step.ln() - layer_param)
                        * 0.1
                        * layer_strength;
                }
            }
            n if n == LAYER_DISTANCE => {
                if !isinf(distance_estimate) && !isnan(distance_estimate) {
                    color_value +=
                        (distance_estimate / (layer_param + 1.0)).ln() * 0.1 * layer_strength;
                }
            }
            n if n == LAYER_ORBIT_TRAP => {
                let mut orbit = orbits[layer_param as usize];
                if orbit > 2.0 {
                    orbit = (orbit / 2.0_f32).ln() + 2.0;
                }
                color_value += orbit * 0.1 * 2.0f32.powf(layer_strength) + layer_param;
            }
            n if n == LAYER_STRIPE => {
                let stripe = stripes[layer_param as usize];
                color_value += (stripe - layer_param.fract())
                    * layer_strength
                    * layer_strength
                    * layer_strength;
            }
            _ => {}
        }
    }

    // then, turn the color value into a color
    let mut color = Vec3f::splat(0.0);
    match color_params.gradient_kind {
        n if n == GRADIENT_FLAT => {
            color = gradient[gradient_offset].rgb();
        }
        n if n == GRADIENT_PROCEDURAL => {
            let a = gradient[gradient_offset].rgb();
            let b = gradient[1 + gradient_offset].rgb();
            let c = gradient[2 + gradient_offset].rgb();
            let d = gradient[3 + gradient_offset].rgb();
            let t = TAU
                * (c * color_value * color_params.color_frequency + color_params.color_offset + d);
            color = a + b * t.cos();
        }
        n if n == GRADIENT_MANUAL => {
            let frac =
                (color_value * color_params.color_frequency + color_params.color_offset).fract();
            for i in 0u32..color_params.gradient_size {
                let stop = gradient[i as usize + gradient_offset];
                let stop_position = stop.w.fract() / 0.999;
                let color_space = COLOR_SPACE_OKLAB;
                let interpolation = stop.w.floor() as u32;
                if frac < stop_position {
                    if i == 0 {
                        let prev_stop =
                            gradient[color_params.gradient_size as usize - 1 + gradient_offset];
                        let t = lerp(prev_stop.w.fract() - 1.0, stop_position, frac);
                        color = mix_advanced(
                            prev_stop.rgb(),
                            stop.rgb(),
                            t,
                            color_space,
                            interpolation,
                        );
                    } else {
                        let prev_stop = gradient[i as usize - 1 + gradient_offset];
                        let t = lerp(prev_stop.w.fract(), stop_position, frac);
                        color = mix_advanced(
                            prev_stop.rgb(),
                            stop.rgb(),
                            t,
                            color_space,
                            interpolation,
                        );
                    }
                    break;
                }
                if i == color_params.gradient_size - 1 {
                    let next_stop = gradient[gradient_offset];
                    let t = lerp(stop_position, next_stop.w.fract() + 1.0, frac);
                    color = mix_advanced(
                        stop.rgb(),
                        next_stop.rgb(),
                        t,
                        color_space,
                        next_stop.w.floor() as u32,
                    );
                }
            }
        }
        n if n == GRADIENT_HUE => {
            color = hsv_to_linear_srgb(Vec3::new(
                (color_value * color_params.color_frequency + color_params.color_offset).fract(),
                gradient[gradient_offset].x,
                gradient[gradient_offset].y,
            ));
        }
        n if n == GRADIENT_OKLCH => {
            color = oklch_to_linear_srgb(Vec3::new(
                gradient[gradient_offset].x,
                gradient[gradient_offset].y,
                (color_value * color_params.color_frequency + color_params.color_offset).fract()
                    * TAU,
            ));
        }
        _ => {}
    }

    // next, calculate lighting
    let mut brightness = Vec3::splat(0.0);
    match color_params.lighting_kind {
        n if n == LIGHTING_FLAT => {
            brightness = Vec3::splat(1.0);
        }
        n if n == LIGHTING_GRADIENT => {
            brightness = Vec3::splat(calculate_lighting_layers(
                Vec2::new(global_id.x, global_id.y),
                render_params,
                color_params,
                // #[host]
                step_buffer,
                // #[host]
                orbit_buffer,
                // #[host]
                stripe_buffer,
                // #[host]
                z_buffer,
                // #[host]
                dz_buffer,
            ));
        }
        n if n == LIGHTING_REPEATING_GRADIENT => {
            let lighting = calculate_lighting_layers(
                Vec2::new(global_id.x, global_id.y),
                render_params,
                color_params,
                // #[host]
                step_buffer,
                // #[host]
                orbit_buffer,
                // #[host]
                stripe_buffer,
                // #[host]
                z_buffer,
                // #[host]
                dz_buffer,
            );
            brightness = Vec3::splat(lighting.cos() * 0.5 + 0.5);
        }
        n if n == LIGHTING_SHADED => {
            let normal = calc_normal(
                global_id,
                render_params,
                color_params,
                // #[host]
                step_buffer,
                // #[host]
                orbit_buffer,
                // #[host]
                stripe_buffer,
                // #[host]
                z_buffer,
                // #[host]
                dz_buffer,
            );
            for i in 0_usize..MAX_LIGHTS {
                let light = color_params.lights[i];
                let light_contribution = normal.dot(light.direction);
                brightness += light_contribution.max(0.0) * light.strength * light.color;
            }
        }
        _ => {}
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
    color = pbr_neutral_tone_mapping(color);
    color = linear_srgb_to_oklch(color);
    color.y = (color.y * color_params.saturation).saturate();
    color = oklch_to_linear_srgb(color);

    // finally, check for ovelays
    if color_params.overlays.iteration_outline.w.fract() > 0.0 {
        let step_x = step_buffer[((global_id.x + 1) + global_id.y * render_params.width) as usize];
        let step_y = step_buffer[(global_id.x + (global_id.y + 1) * render_params.width) as usize];
        let steps = color_params.overlays.iteration_outline.w.floor() as i32;
        if (step / steps - step_x / steps).abs() == 1 || (step / steps - step_y / steps).abs() == 1
        {
            color = mix(
                color,
                color_params.overlays.iteration_outline.rgb(),
                color_params.overlays.iteration_outline.w.fract(),
            );
        }
    }
    if color_params.overlays.set_outline.w.fract() > 0.0 {
        let scale_factor = color_params.overlays.set_outline.w.floor();
        let frac = 1.0 - (distance_estimate / 0.00005 / scale_factor).clamp(0.0, 1.0);
        color = mix(
            color,
            color_params.overlays.set_outline.rgb(),
            color_params.overlays.set_outline.w.fract() * frac,
        );
    }

    return color;
}
