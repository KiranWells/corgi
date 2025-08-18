// a rewrite of the cpu render in WGSL

// inputs
@group(0) @binding(0) var<storage> probed_point : array<vec2<f32>>;
@group(0) @binding(1) var<storage> delta_grid : array<vec2<f32>>;
// note: delta_grid_iter is used as both input and output, as it needs to be saved for each iteration
@group(0) @binding(2) var<storage, read_write> z_grid_iter : array<vec2<f32>>;
@group(0) @binding(3) var<storage, read_write> z_grid_prime : array<vec2<f32>>;

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
    if params.iter_offset != 0u && z_grid_iter[i + j * params.width].x < -ESCAPE_RADIUS {
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

    let z_0 = vec2<f32>(params.x, params.y) + (offset - 0.5) * 2.0 * scale * aspect_scale;
    var z_n: vec2<f32>;
    var z_n_prime: vec2<f32>;
    if params.iter_offset == 0u {
        z_n = z_0;
        z_n_prime = vec2<f32>(2.0 * z_n.x + 1.0, 2.0 * z_n.y);
    } else {
        z_n = z_grid_iter[i + j * params.width];
        z_n_prime = z_grid_prime[i + j * params.width];
    };
    var orbit = 1.0;
    var z_old = vec2<f32>(0.0);

    for (var step = 0u; step < params.probe_len; step = step + 1u) {
        // test if the point is already outside the escape radius
        let radius_squared = z_n.x * z_n.x + z_n.y * z_n.y;
        if radius_squared > ESCAPE_RADIUS || all(z_n == z_old) {
            // set the sentinel value to indicate that the point has escaped
            z_grid_iter[i + j * params.width] = vec2<f32>(-ESCAPE_RADIUS - 1.0, 0.0);
            // update the output values
            if radius_squared > ESCAPE_RADIUS {
                intermediate_step[i + j * params.width] = params.iter_offset + step;
            } else {
                orbit_trap[i + j * params.width] = sqrt(orbit);
                intermediate_step[i + j * params.width] = params.max_iter;
            }
            intermediate_r[i + j * params.width] = sqrt(radius_squared);
            intermediate_dr[i + j * params.width] = sqrt(z_n_prime.x * z_n_prime.x + z_n_prime.y * z_n_prime.y);
            return;
        }
        if (step % 1024u == 0) {
            z_old = z_n;
        }

        // iterate values, according to z = z^2 + c
        //
        // uses an optimized computation method from wikipedia for z:
        //   z.i := 2 × z.r × z.i + c.i
        //   z.r := r2 - i2 + c.r
        //   r2 := z.r × z.r
        //   i2 := z.i × z.i
        //
        // z' is calculated according to the standard formula (z' = 2*z*z' + 1):
        //   z'.r = 2 * (z.r * z'.r - z.i * z'.i) + 1
        //   z'.i = 2 * (z.i * z'.r + z.r * z'.i)

        z_n_prime = vec2<f32>(2.0 * (z_n.x * z_n_prime.x - z_n.y * z_n_prime.y) + 1.0, 2.0 * (z_n.y * z_n_prime.x + z_n.x * z_n_prime.y));
        z_n = vec2<f32>(z_n.x * z_n.x - z_n.y * z_n.y + z_0.r, (z_n.x + z_n.x) * z_n.y + z_0.y);

        // orbit trap around origin
        orbit = min(orbit, radius_squared);
    }

    // update the output values
    orbit_trap[i + j * params.width] = sqrt(orbit);
    intermediate_step[i + j * params.width] = params.max_iter;
    intermediate_r[i + j * params.width] = sqrt(z_n.x * z_n.x + z_n.y * z_n.y);
    intermediate_dr[i + j * params.width] = sqrt(z_n_prime.x * z_n_prime.x + z_n_prime.y * z_n_prime.y);
    z_grid_iter[i + j * params.width] = z_n;
    z_grid_prime[i + j * params.width] = z_n_prime;
}
