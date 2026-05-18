// This compute shader takes the raw fractal data and
// converts it into colors in the final image texture.
import shared::coloring::main::{
    Overlays,
    Light,
    ColorParams,
    RenderParams,
    calculate_lighting_layers,
    calc_normal,
    evaluate_style,
    MAX_GRADIENT_STOPS,
};

// inputs
@group(0) @binding(0) var<storage> step_buffer : array<i32>;
@group(0) @binding(1) var<storage> orbit_buffer : array<vec4f>;
@group(0) @binding(2) var<storage> stripe_buffer : array<vec4f>;
@group(0) @binding(3) var<storage> z_buffer : array<vec3f>;
@group(0) @binding(4) var<storage> dz_buffer : array<vec3f>;

// texture
@group(1) @binding(0) var final_texture: texture_storage_2d<rgba8unorm, write>;

@group(2) @binding(0) var<uniform> external_coloring : ColorParams;
@group(2) @binding(1) var<uniform> internal_coloring : ColorParams;
@group(2) @binding(2) var<uniform> render_params : RenderParams;
@group(2) @binding(3) var<uniform> gradient : array<vec4f, MAX_GRADIENT_STOPS * 2>;

@compute @workgroup_size(16, 16, 1)
fn main_color(@builtin(global_invocation_id) global_id: vec3<u32>) {
    if global_id.x >= render_params.width || global_id.y >= render_params.height {
        return;
    }
    let _dummy0 = step_buffer[0];
    let _dummy1 = stripe_buffer[0];
    let _dummy2 = orbit_buffer[0];
    let _dummy3 = z_buffer[0];
    let _dummy4 = dz_buffer[0];    
    
    var color = evaluate_style(global_id, render_params, internal_coloring, external_coloring, gradient);

    textureStore(
        final_texture,
        vec2<i32>(i32(global_id.x), i32(global_id.y)),
        vec4<f32>(color, 1.0),
    );
}
