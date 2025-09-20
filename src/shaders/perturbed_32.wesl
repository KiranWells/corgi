import super::utils::{ESCAPE_RADIUS, isnan, isinf, STRIPES_ENABLED, TOTAL_ANGLE_ENABLED, ORBIT_ENABLED, DERIVATIVE_ENABLED, JULIA};

// inputs
@group(0) @binding(0) var<storage> probed_point : array<vec2<f32>>;

// intermediate buffers
@group(0) @binding(2) var<storage, read_write> delta_grid_iter : array<vec4<f32>>;
@group(0) @binding(3) var<storage, read_write> delta_grid_prime : array<vec3<f32>>;

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

const SCALE_FACTOR = 10.0;
override flags: u32 = 0;

fn iter_delta_n(delta_n: vec2f, zoom: f32, x_n: vec2f, delta_0: vec2f, zoom_0: f32) -> vec2f {
    // delta_n = 2 * x_n * delta_n - delta_n^2 + delta_0
    // Δₙ_real = 2×xₙ_r×Δₙ_r - 2×xₙ_i×Δₙ_i + Δₙ_r×Δₙ_r - Δₙ_i×Δₙ_i + Δ₀_r;
    // Δₙ_imag = 2×xₙ_r×Δₙ_i + 2×xₙ_i×Δₙ_r + 2×Δₙ_r×Δₙ_i + Δ₀_i;
    var scale = pow(2.0, zoom);
    var scale_0 = pow(2.0, zoom_0 - zoom);
    return vec2(
        2.0 * x_n.x * delta_n.x + delta_n.x * delta_n.x * scale - delta_n.y * delta_n.y * scale - 2.0 * x_n.y * delta_n.y + delta_0.x * scale_0,
        2.0 * x_n.x * delta_n.y + 2.0 * x_n.y * delta_n.x + (2.0 * delta_n.x * delta_n.y) * scale + delta_0.y * scale_0
    );
}

fn iter_delta_n_prime(y_n: vec2f, delta_n_prime: vec2f, zoom_prime: f32) -> vec2f {
    // delta_n_prime = 2 * x_n * delta_n_prime + 2 * x_n_prime * delta_n + 2 * delta_n_prime * delta_n
    // Δₙ'_real = (2×xₙ_r×Δₙ'_r - 2×xₙ_i×Δₙ'_i) + (2×xₙ'_r×Δₙ_r - 2×xₙ'_i×Δₙ_i) + (2×Δₙ'_r×Δₙ_r - 2×Δₙ'_i×Δₙ_i);
    // Δₙ'_imag = 2(xₙ_r×Δₙ'_i + xₙ_i×Δₙ'_r) + 2(xₙ'_r×Δₙ_i + xₙ'_i×Δₙ_r) + 2(Δₙ'_r×Δₙ_i + Δₙ'_i×Δₙ_r);
    return vec2(
        2.0 * (y_n.x * delta_n_prime.x - y_n.y * delta_n_prime.y) + 1.0 * pow(2.0, - zoom_prime),
        2.0 * (y_n.y * delta_n_prime.x + y_n.x * delta_n_prime.y)
    );
}

fn rebase_fractexp(x: ptr<function, vec2f>, exp: ptr<function, f32>) {
    if all(abs(*x) < vec2(pow(2.0, -SCALE_FACTOR))) {
        *x *= pow(2.0, SCALE_FACTOR);
        *exp -= SCALE_FACTOR;
    }
    if all(abs(*x) > vec2(pow(2.0, SCALE_FACTOR))) {
        *x *= pow(2.0, -SCALE_FACTOR);
        *exp += SCALE_FACTOR;
    }
}

fn rebase_probe(x_n: ptr<function, vec2f>, x_0: vec2f, delta_n: ptr<function, vec2f>, zoom: ptr<function, f32>, ref_iteration: ptr<function, u32>) {
    if (flags & JULIA) > 0 {
        let y_n1 = (*x_n - x_0) * pow(2.0, -*zoom) + *delta_n;
        if length(y_n1) < length(*delta_n) {
            *delta_n = y_n1;
            *ref_iteration = 0;
            *x_n = x_0;
        } else if *ref_iteration == params.probe_len {
            *delta_n = (*x_n - x_0) + *delta_n * pow(2.0, *zoom);
            *ref_iteration = 0;
            *x_n = x_0;
            *zoom = 0.0;
        }
    } else {
        let y_n1 = *x_n * pow(2.0, -*zoom) + *delta_n;
        if length(y_n1) < length(*delta_n) || *ref_iteration == params.probe_len {
            *delta_n = y_n1;
            *ref_iteration = 0;
            *x_n = vec2f(0.0);
        }
    }
}

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
    var aspect_scale = vec2<f32>(1.0, 1.0);
    let aspect = f32(params.width) / f32(params.height);
    if aspect< 1.0 {
        aspect_scale.x = aspect;
    } else {
        aspect_scale.y = 1.0 / aspect;
    }
    let offset = vec2(-params.x, -params.y) + vec2<f32>(f32(i) / f32(params.width), f32(j) / f32(params.height));

    var delta_0 = ((offset - 0.5) * 2.0) * aspect_scale;
    var zoom_0 = -params.zoom;
    var zoom = zoom_0;
    var zoom_prime = 0.0;
    let c = vec2(params.cx, params.cy);
    var delta_n: vec2<f32>;
    var delta_n_prime: vec2<f32>;
    var x_old = vec2f(ESCAPE_RADIUS * 2);
    var delta_old = vec2f(-ESCAPE_RADIUS);
    var zoom_old = 0.0;
    var orbits = vec4f(1.0);
    var stripes = vec4f(0.0);
    var prev_stripes: vec4f;
    var ref_iteration = 0u;
    if params.iter_offset == 0u {
        if (flags & JULIA) > 0 {
            delta_n = delta_0;
            delta_0 = vec2(0.0);
        } else {
            delta_n = vec2(0.0);
        }
        delta_n_prime = vec2(0.0);
        intermediate_step[buffer_index] = 0;
    } else {
        if (flags & JULIA) > 0 {
            delta_0 = vec2(0.0);
        }
        delta_n = delta_grid_iter[buffer_index].xy;
        zoom = delta_grid_iter[buffer_index].z;
        ref_iteration = bitcast<u32>(delta_grid_iter[buffer_index].w);
        delta_n_prime = delta_grid_prime[buffer_index].xy;
        zoom_prime = delta_grid_prime[buffer_index].z;
        orbits = orbit_traps[buffer_index];
        stripes = stripes_buffer[buffer_index];
    };
    let y_0 = c + delta_0 * pow(2.0, zoom_0);
    var complete = false;
    var step = 0u;
    var closest = length(y_0);
    var min_iter = 1u;
    var line = normalize(-y_0);
    var angles = 0.0;
    var total_angle = 0.0;
    var previous = vec2(0.0);
    var stripes_started = true;

    for (step = 0u; step < params.chunk_max_iter; step = step + 1u) {
        var x_n = probed_point[ref_iteration];
        var scale = pow(2.0, zoom);


        let y_n = x_n + delta_n * pow(2.0, zoom);
        if (flags & TOTAL_ANGLE_ENABLED) > 0 {
            if step + params.iter_offset > 1u && step + params.iter_offset < min_iter *2 {
                angles += acos(dot(normalize(y_n - previous), line));
                line = normalize(previous - y_n);
                let distance = length(x_n + delta_n * scale - (c + delta_0 * pow(2.0, zoom_0)));
                if distance < closest {
                    total_angle = angles;
                    min_iter = step + params.iter_offset;
                    closest = distance;
                }
            }
        }
        previous = y_n;
        let radius_squared = y_n.x * y_n.x + y_n.y * y_n.y;
        // calculate orbit traps and stripe averages
        if (flags & STRIPES_ENABLED) > 0 && (stripes_started || radius_squared > 64.0) {
            prev_stripes = stripes;
            stripes.x += 0.5+0.5*sin(5.0*atan2(y_n.x, y_n.y));
            if length(y_n) != 0.0 {
                stripes.y += 0.5+0.5*normalize(y_n).x;
                stripes.z += 0.5+0.5*normalize(y_n).y;
            }
            if !stripes_started {
                let frac = -1.0 + log2(2.0 * log(64.0)) - log2(0.5 * log(radius_squared));
                stripes *= 1.0-frac;
                stripes_started = true;
            }
        }
        if (flags & ORBIT_ENABLED) > 0 && step + params.iter_offset > 1u {
            orbits.x = min(orbits.x, length(y_n));
            orbits.z = min(orbits.z, abs(y_n.y - 0.5));
            orbits.z = min(orbits.z, abs(y_n.x + 0.5));
        }

        // test if the point is already outside the escape radius
        if radius_squared > ESCAPE_RADIUS || (step > 100u && all((x_n - x_old) * pow(2.0, -zoom) == delta_n - delta_old * pow(2.0, zoom_old-zoom))) {
            complete = true;
            break;
        }
        rebase_probe(&x_n, probed_point[0], &delta_n, &zoom, &ref_iteration);
        ref_iteration += 1;
        if (step + params.iter_offset >= 100u && (step - 100u) % 1024u == 0) {
            x_old = x_n;
            delta_old = delta_n;
            zoom_old = zoom;
        }

        // calculate the next iteration according to the perturbation formula
        if (flags & DERIVATIVE_ENABLED) > 0 {
            delta_n_prime = iter_delta_n_prime(y_n, delta_n_prime, zoom_prime);
        }
        delta_n = iter_delta_n(delta_n, zoom, x_n, delta_0, zoom_0);
        if step % 32 == 0  {
            rebase_fractexp(&delta_n, &zoom);
            rebase_fractexp(&delta_n_prime, &zoom_prime);
        }
    }

    // update the output values
    orbit_traps[buffer_index] = orbits;
    stripes_buffer[buffer_index] = stripes;
    let x_n = probed_point[ref_iteration];
    let y_n = x_n + delta_n * pow(2.0, zoom);
    let radius_squared = y_n.x * y_n.x + y_n.y * y_n.y;
    let internal = radius_squared < 4.0;

    if complete || params.iter_offset + params.chunk_max_iter >= params.max_iter {
        if internal {
            intermediate_step[buffer_index] = -i32(min_iter);
        } else {
            intermediate_step[buffer_index] = i32(params.iter_offset + step);
        }
        delta_grid_iter[buffer_index] = vec4(y_n, zoom, 0.0);
        delta_grid_prime[buffer_index] = vec3(delta_n_prime * pow(2.0, zoom_prime + zoom_0), -zoom_0);

        if internal {
            stripes_buffer[buffer_index] = stripes;
        } else {
            let frac = -1.0 + log2(2.0 * log(ESCAPE_RADIUS)) - log2(0.5 * log(radius_squared));
            stripes_buffer[buffer_index] = stripes / f32(params.iter_offset + step) * frac + prev_stripes / f32(params.iter_offset + step - 1) * (1.0 - frac);
        }
    } else {
        delta_grid_iter[buffer_index] = vec4(delta_n, zoom, bitcast<f32>(ref_iteration));
        delta_grid_prime[buffer_index] = vec3(delta_n_prime, zoom_prime);
    }
}
