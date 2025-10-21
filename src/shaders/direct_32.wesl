import super::utils::{
    ESCAPE_RADIUS,
    DERIVATIVE_ENABLED,
    JULIA,
    ORBIT_ENABLED,
    STRIPES_ENABLED,
    TOTAL_ANGLE_ENABLED,
    aspect,
    get_orbit_values,
    get_stripe_values,
    length_squared,
    step_frac,
};

// inputs
@group(0) @binding(0) var<storage> probed_point : array<vec2<f32>>;

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
    chunk_max_iter: u32,
    probe_len: u32,
    iter_offset: u32,
    x: f32,
    y: f32,
    cx: f32,
    cy: f32,
    zoom: f32,
    julia_x: f32,
    julia_y: f32,
};
@group(1) @binding(0) var<uniform> params : Params;

override flags: u32 = 0;

@compute @workgroup_size(16, 16, 1)
fn main_mandel(@builtin(global_invocation_id) global_id: vec3<u32>) {
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

    let aspect_scale = aspect(params.width, params.height);
    let offset = vec2(params.x, params.y) + (
            vec2<f32>(
                f32(global_id.x) / f32(params.width),
                f32(global_id.y) / f32(params.height)
            ) - 0.5
        ) * 2.0 * pow(2.0, -params.zoom) * aspect_scale;

    // initial iteration values
    var z_0 = vec2f(0.0);
    if (flags & JULIA) != 0 {
        z_0 = vec2(params.julia_x, params.julia_y);
    } else {
        z_0 = offset;
    }
    var z_n: vec2<f32>;
    if (flags & JULIA) != 0 {
        z_n = offset;
    } else {
        z_n = vec2f(0.0);
    }
    var z_n_prime = vec2f(1.0, 0.0);
    var orbits = vec4f(ESCAPE_RADIUS);
    var stripes = vec4f(0.0);

    if params.iter_offset != 0u {
        z_n = z_grid_iter[buffer_index].xy;
        z_n_prime = z_grid_prime[buffer_index].xy;
        orbits = orbit_traps[buffer_index];
        stripes = stripes_buffer[buffer_index];
    };

    // reference values for detecting orbit cycles
    var z_old = vec2<f32>(ESCAPE_RADIUS);
    // internal coloring values
    var closest = length(z_0);
    var min_iter = 1u;
    var line = vec2(0.0);
    var angles = 0.0;
    var total_angle = 0.0;

    // stripe temporary values
    var stripes_started = true;
    var prev_stripes: vec4f;

    // iteration trackers
    var complete = false;
    var step = 0u;

    for (step = 0u; step < params.chunk_max_iter; step = step + 1u) {
        let radius_squared = z_n.x * z_n.x + z_n.y * z_n.y;
        // calculate stripe averages and orbit traps
        if (flags & STRIPES_ENABLED) != 0 && (stripes_started || radius_squared > 64.0) {
            prev_stripes = stripes;
            stripes += get_stripe_values(z_n);
            if !stripes_started {
                stripes *= 1.0 - step_frac(radius_squared, 64.0);
                stripes_started = true;
            }
        }
        if (flags & ORBIT_ENABLED) != 0 && step + params.iter_offset > 1u {
            orbits = min(orbits, get_orbit_values(z_n));
        }

        // test if the point is already outside the escape radius
        // or that we are repeating a cycle
        if radius_squared > ESCAPE_RADIUS || all(z_n == z_old) {
            // update the output values
            complete = true;
            break;
        }
        if (step >= 100u && (step - 100u) % 1024u == 0) {
            z_old = z_n;
        }

        let previous = z_n;

        // iterate values, according to z = z^2 + c
        // z' is calculated according to the standard formula (z' = 2*z*z' + 1):
        if (flags & DERIVATIVE_ENABLED) != 0 {
            z_n_prime = vec2<f32>(
                2.0 * (z_n.x * z_n_prime.x - z_n.y * z_n_prime.y) + 1.0,
                2.0 * (z_n.y * z_n_prime.x + z_n.x * z_n_prime.y)
            );
        }
        z_n = vec2<f32>(z_n.x * z_n.x - z_n.y * z_n.y + z_0.r, (z_n.x + z_n.x) * z_n.y + z_0.y);

        // track total angle for a cycle
        if (flags & TOTAL_ANGLE_ENABLED) != 0 {
            if !(step == 0u && params.iter_offset == 0) {
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
    }

    // update the output values
    orbit_traps[buffer_index] = orbits;
    stripes_buffer[buffer_index] = stripes;
    z_grid_iter[buffer_index] = vec3(z_n, 0.0);
    z_grid_prime[buffer_index] = vec3(z_n_prime, 0.0);
    let radius_squared = z_n.x * z_n.x + z_n.y * z_n.y;
    let internal = radius_squared < 4.0;

    if complete || params.iter_offset + params.chunk_max_iter >= params.max_iter {
        z_grid_iter[buffer_index] = vec3(z_n, 0.0);

        if internal {
            intermediate_step[buffer_index] = -i32(min_iter);
            z_grid_prime[buffer_index] = vec3(total_angle);
            stripes_buffer[buffer_index] = stripes / f32(params.iter_offset + step);
        } else {
            intermediate_step[buffer_index] = i32(params.iter_offset + step);
            z_grid_prime[buffer_index] = vec3(z_n_prime * pow(2.0, -params.zoom), params.zoom);
            let frac = step_frac(radius_squared, ESCAPE_RADIUS);
            stripes_buffer[buffer_index] = stripes / f32(params.iter_offset + step) * frac + prev_stripes / f32(params.iter_offset + step - 1) * (1.0 - frac);
        }
    }
}
