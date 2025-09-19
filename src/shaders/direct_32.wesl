import super::utils::{ESCAPE_RADIUS};

// inputs
@group(0) @binding(0) var<storage> probed_point : array<vec2<f32>>;
@group(0) @binding(1) var<storage> probed_point_prime : array<vec2<f32>>;

// intermediate buffers
@group(0) @binding(2) var<storage, read_write> z_grid_iter : array<vec3<f32>>;
@group(0) @binding(3) var<storage, read_write> z_grid_prime : array<vec3<f32>>;

// outputs
@group(0) @binding(4) var<storage, read_write> intermediate_step : array<i32>;
@group(0) @binding(5) var<storage, read_write> orbit_traps : array<vec4f>;
@group(0) @binding(6) var<storage, read_write> stripes_buffer : array<vec4f>;

struct Params {
    width: u32,
    height: u32,
    max_iter: u32,
    probe_len: u32,
    chunk_max_iter: u32,
    iter_offset: u32,
    x: f32,
    y: f32,
    cx: f32,
    cy: f32,
    zoom: f32,
};
@group(1) @binding(0) var<uniform> params : Params;

@compute @workgroup_size(16, 16, 1)
fn main_mandel(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let i = global_id.x;
    let j = global_id.y;
    let buffer_index = i + j * params.width;

    // skip if the point is outside the image 
    // (this is caused by the workgroup size not being a factor of the image size)
    if i >= params.width || j >= params.height {
        return;
    }

    // skip if the point is already outside the escape radius
    // The step buffer is only set to non-zero if the point is
    // done being iterated
    if params.iter_offset != 0u && intermediate_step[buffer_index] != 0 {
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
    var z_old = vec2<f32>(ESCAPE_RADIUS);
    var orbits = vec4f(1.0);
    var stripes = vec4f(0.0);
    var prev_stripes: vec4f;
    if params.iter_offset == 0u {
        z_n = vec2f(0.0);
        z_n_prime = vec2f(1.0, 0.0);
        intermediate_step[buffer_index] = 0;
    } else {
        z_n = z_grid_iter[buffer_index].xy;
        z_n_prime = z_grid_prime[buffer_index].xy;
        orbits = orbit_traps[buffer_index];
        stripes = stripes_buffer[buffer_index];
    };
    var closest = length(z_0);
    var min_iter = 1u;
    var line = vec2(0.0);
    var angles = 0.0;
    var total_angle = 0.0;
    var stripes_started = true;

    for (var step = 0u; step < params.chunk_max_iter; step = step + 1u) {
        // test if the point is already outside the escape radius
        let radius_squared = z_n.x * z_n.x + z_n.y * z_n.y;

        if stripes_started || radius_squared > 64.0 {
            prev_stripes = stripes;
            stripes.x += 0.5+0.5*sin(5.0*atan2(z_n.x, z_n.y));
            stripes.y += 0.5+0.5*normalize(z_n).x;
            stripes.y += 0.5+0.5*normalize(z_n).y;
            if !stripes_started {
                let frac = -1.0 + log2(2.0 * log(64.0)) - log2(0.5 * log(radius_squared));
                stripes *= 1.0-frac;
            }
        }

        if radius_squared > ESCAPE_RADIUS || all(z_n == z_old) {
            // update the output values
            if radius_squared > ESCAPE_RADIUS {
                intermediate_step[buffer_index] = i32(params.iter_offset + step);
                let frac = -1.0 + log2(2.0 * log(ESCAPE_RADIUS)) - log2(0.5 * log(radius_squared));
                stripes_buffer[buffer_index] = stripes / f32(params.iter_offset + step) * frac + prev_stripes / f32(params.iter_offset + step - 1) * (1.0 - frac);
                z_grid_prime[buffer_index] = vec3(z_n_prime * pow(2.0, -params.zoom), params.zoom);
            } else {
                intermediate_step[buffer_index] = -i32(min_iter);
                stripes_buffer[buffer_index] = stripes / f32(params.iter_offset + step);
                z_grid_prime[buffer_index] = vec3(total_angle);
            }
            z_grid_iter[buffer_index] = vec3(z_n, 0.0);
            orbit_traps[buffer_index] = orbits;
            return;
        }
        if (step >= 100u && (step - 100u) % 1024u == 0) {
            z_old = z_n;
        }

        let previous = z_n;
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
        if !(step == 0u && params.iter_offset == 0) {
            orbits.x = min(orbits.x, length(z_n));
            orbits.z = min(orbits.z, abs(z_n.y - 0.5));
            orbits.z = min(orbits.z, abs(z_n.x + 0.5));
            angles += acos(dot(normalize(z_n - previous), line));
            line = normalize(previous - z_n);
            let distance = length(z_n - z_0);
            if distance < closest {
                total_angle = angles;
                min_iter = step + params.iter_offset + 1u;
                closest = distance;
            }
        } else {
            line = normalize(previous - z_n);
        }
    }

    // update the output values
    orbit_traps[buffer_index] = orbits;
    stripes_buffer[buffer_index] = stripes;
    z_grid_iter[buffer_index] = vec3(z_n, 0.0);
    z_grid_prime[buffer_index] = vec3(z_n_prime, 0.0);
    if params.iter_offset + params.chunk_max_iter >= params.max_iter {
        // this is the last iter
        intermediate_step[buffer_index] = -i32(min_iter);
        stripes_buffer[buffer_index] = stripes / f32(params.max_iter);
        z_grid_prime[buffer_index] = vec3(total_angle);
    }
}
