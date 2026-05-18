// This is a non-perturbed version of the fractal compute step.
// It is used for increased performance at low zoom values, and
// as a sanity check to ensure the perturbed algorithm is correct.
import shared::types::{ComputeParams, BufferValues};
import shared::algorithms::direct_32::calculate_point;

// inputs
@group(0) @binding(0) var<storage> probed_point : array<vec2<f32>>;

// intermediate buffers
@group(0) @binding(2) var<storage, read_write> z_grid_iter : array<vec3<f32>>;
@group(0) @binding(3) var<storage, read_write> z_grid_prime : array<vec3<f32>>;

// outputs
@group(0) @binding(4) var<storage, read_write> intermediate_step : array<i32>;
@group(0) @binding(5) var<storage, read_write> orbit_traps : array<vec4f>;
@group(0) @binding(6) var<storage, read_write> stripes_buffer : array<vec4f>;

@group(1) @binding(0) var<uniform> params : ComputeParams;

override flags: u32 = 0;

@compute @workgroup_size(16, 16, 1)
fn main_mandel(@builtin(global_invocation_id) global_id: vec3<u32>) {
    // needed to prevent probed_point from being stripped by wesl
    let _dummy = probed_point[0];
    // skip if the point is outside the image 
    // (this is caused by the workgroup size not being a factor of the image size)
    if global_id.x >= params.width || global_id.y >= params.height {
        return;
    }
    let buffer_index = global_id.x + global_id.y * params.width;
    // skip if the point is already outside the escape radius
    // The step buffer is only set to non-zero if the point is
    // done being iterated
    if params.iter_offset == 0u {
        intermediate_step[buffer_index] = 0;
    }
    if intermediate_step[buffer_index] != 0 {
        return;
    }

    let bv=  BufferValues(
        z_grid_iter[buffer_index].xy,
        0.0,
        0u,
        z_grid_prime[buffer_index].xy,
        0.0,
        orbit_traps[buffer_index],
        stripes_buffer[buffer_index],
        0,
    );
    let output = calculate_point(global_id.xy, bv, flags, params);

    orbit_traps[buffer_index] = output.orbits;
    stripes_buffer[buffer_index] = output.stripes;
    z_grid_iter[buffer_index] = vec3(output.delta_n, output.zoom);
    z_grid_prime[buffer_index] = vec3(output.z_n_prime, output.zoom_prime);
    if output.step != 0 {
        intermediate_step[buffer_index] = output.step;
    }
}
