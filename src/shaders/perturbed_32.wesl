// Perturbed version of the fractal compute step.
import shared::algorithms::perturbed_32::{
    iter_delta_n,
    iter_z_n_prime,
    rebase_fractexp,
    rebase_probe_julia,
    rebase_probe_mandel,
    calculate_point,
};
import shared::types::{ComputeParams, BufferValues};

// inputs
@group(0) @binding(0) var<storage> probed_point : array<vec2<f32>>;

// intermediate buffers
@group(0) @binding(2) var<storage, read_write> delta_grid_iter : array<vec4<f32>>;
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
        delta_grid_iter[buffer_index].xy,
        delta_grid_iter[buffer_index].z,
        bitcast<u32>(delta_grid_iter[buffer_index].w),
        z_grid_prime[buffer_index].xy,
        z_grid_prime[buffer_index].z,
        orbit_traps[buffer_index],
        stripes_buffer[buffer_index],
        intermediate_step[buffer_index],
    );
    let output = calculate_point(global_id.xy, bv, flags, params);

    orbit_traps[buffer_index] = output.orbits;
    stripes_buffer[buffer_index] = output.stripes;
    delta_grid_iter[buffer_index] = vec4(output.delta_n, output.zoom, bitcast<f32>(output.ref_iteration));
    z_grid_prime[buffer_index] = vec3(output.z_n_prime, output.zoom_prime);
    if output.step != 0 {
        intermediate_step[buffer_index] = output.step;
    }
}
