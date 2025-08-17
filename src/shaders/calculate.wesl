// a rewrite of the cpu render in WGSL

// inputs
@group(0) @binding(0) var<storage> probed_point : array<vec2<f32>>;
@group(0) @binding(1) var<storage> delta_grid : array<vec2<f32>>;
// note: delta_grid_iter is used as both input and output, as it needs to be saved for each iteration
@group(0) @binding(2) var<storage, read_write> delta_grid_iter : array<vec2<f32>>;
@group(0) @binding(3) var<storage, read_write> delta_grid_prime : array<vec2<f32>>;

// outputs
@group(0) @binding(4) var<storage, read_write> intermediate_step : array<u32>;
@group(0) @binding(5) var<storage, read_write> orbit_trap : array<f32>;
@group(0) @binding(6) var<storage, read_write> intermediate_r : array<f32>;
@group(0) @binding(7) var<storage, read_write> intermediate_dr : array<f32>;

struct Params {
    width: u32,
    height: u32,
    max_iter: u32,
    probe_len: u32,
    iter_offset: u32,
    x: f32,
    y: f32,
    zoom: f32,
};
@group(1) @binding(0) var<uniform> params : Params;

@compute @workgroup_size(16, 16, 1)
fn main_mandel(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let ESCAPE_RADIUS: f32 = 1000.0;
    let EPSILON: f32 = 1.1920929E-17;
    let i = global_id.x;
    let j = global_id.y;

    // skip if the point is outside the image 
    // (this is caused by the workgroup size not being a factor of the image size)
    if i >= params.width || j >= params.height {
        return;
    }

    // skip if the point is already outside the escape radius
    // a sentinel value of -1.0 is used to indicate that the point has already been
    // determined to be outside the escape radius
    if params.iter_offset != 0u && delta_grid_iter[i + j * params.width].x < -ESCAPE_RADIUS {
        return;
    }
    let scale = pow(2.0, -params.zoom);
    var aspect_scale = vec2<f32>(1.0, 1.0);
    let aspect = f32(params.width) / f32(params.height);
    if aspect< 1.0 {
        aspect_scale.x = aspect;
    } else {
        aspect_scale.y = 1.0 / aspect;
    }
    let offset = vec2<f32>(f32(i) / f32(params.width), f32(j) / f32(params.height));

    let delta_0 = vec2<f32>(params.x, params.y) + (offset - 0.5) * 2.0 * scale * aspect_scale;
    var delta_n: vec2<f32>;
    var delta_n_prime: vec2<f32>;
    if params.iter_offset == 0u {
        delta_n = delta_0;
        let x_n_prime = probed_point[params.probe_len];
        delta_n_prime = vec2<f32>(
            2.0 * (x_n_prime.x * delta_n.x - x_n_prime.y * delta_n.y),
            2.0 * (x_n_prime.x * delta_n.y + x_n_prime.y * delta_n.x)
        );
    } else {
        delta_n = delta_grid_iter[i + j * params.width];
        delta_n_prime = delta_grid_prime[i + j * params.width];
    };
    var orbit = 1.0;
    var y_old = vec2<f32>(0.0);

    for (var step = 0u; step < params.probe_len; step = step + 1u) {
        let x_n = probed_point[step];
        let x_n_prime = probed_point[step + params.probe_len];

        // test if the point is already outside the escape radius
        let y_n = x_n + delta_n;
        // let diff = y_n - y_old;
        // let dist = diff.x * diff.x + diff.y + diff.y;
        let radius_squared = y_n.x * y_n.x + y_n.y * y_n.y;
        if radius_squared > ESCAPE_RADIUS {
            // set the sentinel value to indicate that the point has escaped
            delta_grid_iter[i + j * params.width] = vec2<f32>(-ESCAPE_RADIUS - 1.0, 0.0);
            // update the output values
            if radius_squared > ESCAPE_RADIUS {
                intermediate_step[i + j * params.width] = params.iter_offset + step;
            } else {
                orbit_trap[i + j * params.width] = sqrt(orbit);
                intermediate_step[i + j * params.width] = params.max_iter;
            }
            intermediate_r[i + j * params.width] = sqrt(radius_squared);
            let y_n_prime = x_n_prime + delta_n_prime;
            intermediate_dr[i + j * params.width] = sqrt(y_n_prime.x * y_n_prime.x + y_n_prime.y * y_n_prime.y);
            return;
        }

        // calculate the next iteration according to the perturbation formula

        // delta_n_prime = 2 * x_n * delta_n_prime + 2 * x_n_prime * delta_n + 2 * delta_n_prime * delta_n
        // Δₙ'_real = (2×xₙ_r×Δₙ'_r - 2×xₙ_i×Δₙ'_i) + (2×xₙ'_r×Δₙ_r - 2×xₙ'_i×Δₙ_i) + (2×Δₙ'_r×Δₙ_r - 2×Δₙ'_i×Δₙ_i);
        // Δₙ'_imag = 2(xₙ_r×Δₙ'_i + xₙ_i×Δₙ'_r) + 2(xₙ'_r×Δₙ_i + xₙ'_i×Δₙ_r) + 2(Δₙ'_r×Δₙ_i + Δₙ'_i×Δₙ_r);
        delta_n_prime = vec2<f32>(
            2.0 * (x_n.x * delta_n_prime.x - x_n.y * delta_n_prime.y + x_n_prime.x * delta_n.x - x_n_prime.y * delta_n.y + delta_n_prime.x * delta_n.x - delta_n_prime.y * delta_n.y),
            2.0 * (x_n.x * delta_n_prime.y + x_n.y * delta_n_prime.x + x_n_prime.x * delta_n.y + x_n_prime.y * delta_n.x + delta_n_prime.x * delta_n.y + delta_n_prime.y * delta_n.x)
        );
        // delta_n = 2 * x_n * delta_n - delta_n^2 + delta_0
        // Δₙ_real = 2×xₙ_r×Δₙ_r - 2×xₙ_i×Δₙ_i + Δₙ_r×Δₙ_r - Δₙ_i×Δₙ_i + Δ₀_r;
        // Δₙ_imag = 2×xₙ_r×Δₙ_i + 2×xₙ_i×Δₙ_r + 2×Δₙ_r×Δₙ_i + Δ₀_i;
        delta_n = vec2<f32>(
            2.0 * x_n.x * delta_n.x - 2.0 * x_n.y * delta_n.y + delta_n.x * delta_n.x - delta_n.y * delta_n.y + delta_0.x,
            2.0 * x_n.x * delta_n.y + 2.0 * x_n.y * delta_n.x + 2.0 * delta_n.x * delta_n.y + delta_0.y
        );

        // orbit trap around origin
        orbit = min(orbit, radius_squared);
    }

    // update the output values
    orbit_trap[i + j * params.width] = sqrt(orbit);
    intermediate_step[i + j * params.width] = params.max_iter;
    let x_n = probed_point[params.probe_len - 1u];
    let x_n_prime = probed_point[params.probe_len * 2u - 1u];
    let y_n = x_n + delta_n;
    let y_n_prime = x_n_prime + delta_n_prime;
    intermediate_r[i + j * params.width] = sqrt(y_n.x * y_n.x + y_n.y * y_n.y);
    intermediate_dr[i + j * params.width] = sqrt(y_n_prime.x * y_n_prime.x + y_n_prime.y * y_n_prime.y);
    delta_grid_iter[i + j * params.width] = delta_n;
    delta_grid_prime[i + j * params.width] = delta_n_prime;
}
