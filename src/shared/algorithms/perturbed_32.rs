// Perturbed version of the fractal compute step.
use super::super::types::{BufferValues, ComputeParams};
use super::super::utils::{
    DERIVATIVE_ENABLED, ESCAPE_RADIUS, FRACTEXP_SCALE_FACTOR, JULIA, ORBIT_ENABLED,
    STRIPES_ENABLED, TOTAL_ANGLE_ENABLED, aspect, get_orbit_values, get_stripe_values,
    length_squared, rotation_matrix, scaled, step_frac,
};
use crate::shared::wgsl_primitives::*;

fn iter_delta_n(delta_n: Vec2f, zoom: f32, x_n: Vec2f, delta_0: Vec2f, zoom_0: f32) -> Vec2f {
    let scale = 2.0f32.powf(zoom);
    let scale_diff = 2.0f32.powf(zoom_0 - zoom);
    return Vec2::new(
        2.0 * (x_n.x * delta_n.x - x_n.y * delta_n.y)
            + (delta_n.x * delta_n.x - delta_n.y * delta_n.y) * scale
            + delta_0.x * scale_diff,
        2.0 * (x_n.x * delta_n.y + x_n.y * delta_n.x)
            + (delta_n.x * delta_n.y + delta_n.x * delta_n.y) * scale
            + delta_0.y * scale_diff,
    );
}

fn iter_z_n_prime(y_n: Vec2f, z_n_prime: Vec2f, zoom_prime: f32) -> Vec2f {
    return Vec2::new(
        2.0 * (y_n.x * z_n_prime.x - y_n.y * z_n_prime.y) + 1.0 * 2.0f32.powf(-zoom_prime),
        2.0 * (y_n.y * z_n_prime.x + y_n.x * z_n_prime.y),
    );
}

fn rebase_fractexp(x: &mut Vec2f, exp: &mut f32) {
    let abs = (*x).abs();
    let lower_bound = Vec2::splat(2.0f32.powf(-FRACTEXP_SCALE_FACTOR));
    if abs.x < lower_bound.x && abs.y < lower_bound.y {
        *x *= 2.0f32.powf(FRACTEXP_SCALE_FACTOR);
        *exp -= FRACTEXP_SCALE_FACTOR;
    }
    let upper_bound = Vec2::splat(2.0f32.powf(FRACTEXP_SCALE_FACTOR));
    if abs.x < upper_bound.x && abs.y < upper_bound.y {
        *x *= 2.0f32.powf(-FRACTEXP_SCALE_FACTOR);
        *exp += FRACTEXP_SCALE_FACTOR;
    }
}

fn rebase_probe_mandel(
    x_n: &mut Vec2f,
    _x_0: Vec2f,
    delta_n: &mut Vec2f,
    zoom: &mut f32,
    ref_iteration: &mut u32,
    probe_len: u32,
) {
    let y_n1 = *x_n * 2.0f32.powf(-*zoom) + *delta_n;
    if length_squared(y_n1) < length_squared(*delta_n) || *ref_iteration == probe_len {
        *delta_n = y_n1;
        *ref_iteration = 0;
        *x_n = Vec2::splat(0.0);
    }
}

fn rebase_probe_julia(
    x_n: &mut Vec2f,
    x_0: Vec2f,
    delta_n: &mut Vec2f,
    zoom: &mut f32,
    ref_iteration: &mut u32,
    probe_len: u32,
) {
    let y_n1 = (*x_n - x_0) * 2.0f32.powf(-*zoom) + *delta_n;
    if length_squared(y_n1) < length_squared(*delta_n) {
        *delta_n = y_n1;
        *ref_iteration = 0;
        *x_n = x_0;
    } else if *ref_iteration == probe_len {
        let scaled: Vec2f = *delta_n * 2.0f32.powf(*zoom);
        *delta_n = (*x_n - x_0) + scaled;
        *ref_iteration = 0;
        *x_n = x_0;
        *zoom = 0.0;
    }
}

pub fn calculate_point(
    global_id: Vec2u,
    buffer_values: BufferValues,
    flags: u32,
    params: ComputeParams,
    probed_point: &[Vec2f],
) -> BufferValues {
    let aspect_scale: Vec2f = aspect(params.width, params.height);
    let offset: Vec2f = (Vec2::new(-params.x, -params.y)
        + (Vec2::new(
            global_id.x as f32 / params.width as f32,
            global_id.y as f32 / params.height as f32,
        ) - 0.5f32))
        * 2.0f32
        * aspect_scale
        * rotation_matrix(params.angle);

    // constant iteration values
    let x_0 = probed_point[1];
    let mut delta_0 = Vec2::splat(0.0);
    if (flags & JULIA) == 0 {
        // mandelbrot
        delta_0 = offset;
    }
    let zoom_0 = -params.zoom;
    let y_0 = x_0 + scaled(delta_0, zoom_0);

    // initial iteration values
    let mut delta_n = Vec2::splat(0.0);
    if (flags & JULIA) != 0 {
        // julia
        delta_n = offset;
    }
    let mut zoom = zoom_0;
    let mut z_n_prime = Vec2::splat(0.0);
    let mut zoom_prime = 0.0;
    let mut orbits = Vec4::splat(ESCAPE_RADIUS);
    let mut stripes = Vec4::splat(0.0);

    // reference values for detecting orbit cycles
    let mut x_old = Vec2::splat(ESCAPE_RADIUS * 2.0);
    let mut delta_old = Vec2::splat(-ESCAPE_RADIUS);
    let mut zoom_old = 0.0;
    let mut ref_iteration = 0u32;

    if params.iter_offset != 0u32 {
        delta_n = buffer_values.delta_n;
        zoom = buffer_values.zoom;
        z_n_prime = buffer_values.z_n_prime;
        zoom_prime = buffer_values.zoom_prime;
        orbits = buffer_values.orbits;
        stripes = buffer_values.stripes;
        ref_iteration = buffer_values.ref_iteration;
    };

    // internal coloring values
    let mut closest = length_squared(y_0);
    let mut min_iter = 1u32;
    let mut line = -y_0.normalize();
    let mut angles = 0.0;
    let mut total_angle = 0.0;
    let mut previous = Vec2::splat(0.0);

    // stripe temporary values
    let mut stripes_started = true;
    let mut prev_stripes = Vec4::splat(0.0);

    // iteration trackers
    let mut complete = false;
    let mut outer_step = 0u32;

    // for (step = 0u32; step < params.chunk_max_iter; step = step + 1u32) {
    for step in 0u32..params.chunk_max_iter {
        outer_step = step;
        let mut x_n = probed_point[ref_iteration as usize];
        let y_n: Vec2f = x_n + scaled(delta_n, zoom);

        // track total angle for a cycle
        if (flags & TOTAL_ANGLE_ENABLED) != 0 && step + params.iter_offset > 1u32 {
            let new_normalized = (y_n - previous).normalize();
            angles += new_normalized.dot(line).acos();
            line = (previous - y_n).normalize();
            // This must be calculated in this order to avoid
            // underflowing/rounding errors. It is equivalent
            // to length_squared(y_n - y_0)
            let distance = length_squared(
                (x_n - x_0) + scaled(delta_n - scaled(delta_0, zoom_0 - zoom), zoom),
            );
            if distance < closest {
                total_angle = angles;
                min_iter = step + params.iter_offset;
                closest = distance;
            }
        }
        previous = y_n;

        let radius_squared = y_n.x * y_n.x + y_n.y * y_n.y;
        // calculate stripe averages and orbit traps
        if (flags & STRIPES_ENABLED) != 0 && (stripes_started || radius_squared > 64.0) {
            prev_stripes = stripes;
            stripes += get_stripe_values(y_n);
            if !stripes_started {
                stripes *= 1.0 - step_frac(radius_squared, 64.0);
                stripes_started = true;
            }
        }
        if (flags & ORBIT_ENABLED) != 0 && step + params.iter_offset > 1u32 {
            orbits = orbits.min(get_orbit_values(y_n));
        }

        // test if the point is already outside the escape radius
        // or that we are repeating a cycle
        let x_diff: Vec2f = scaled(x_n - x_old, -zoom);
        let delta_diff: Vec2f = delta_n - scaled(delta_old, zoom_old - zoom);
        if radius_squared > ESCAPE_RADIUS
            || (step + params.iter_offset > 100u32
                && x_diff.x == delta_diff.x
                && x_diff.y == delta_diff.y)
        {
            complete = true;
            break;
        }
        if (flags & JULIA) != 0 {
            rebase_probe_julia(
                &mut x_n,
                probed_point[0],
                &mut delta_n,
                &mut zoom,
                &mut ref_iteration,
                params.probe_len,
            );
        } else {
            rebase_probe_mandel(
                &mut x_n,
                probed_point[0],
                &mut delta_n,
                &mut zoom,
                &mut ref_iteration,
                params.probe_len,
            );
        }
        ref_iteration += 1;

        // update cycle reference
        #[expect(clippy::manual_is_multiple_of)]
        if step + params.iter_offset >= 100u32
            && (step + params.iter_offset - 100u32) % (1024u32) == 0
        {
            x_old = x_n;
            delta_old = delta_n;
            zoom_old = zoom;
        }

        // calculate the next iteration according to the perturbation formula
        if (flags & DERIVATIVE_ENABLED) != 0 {
            z_n_prime = iter_z_n_prime(y_n, z_n_prime, zoom_prime);
        }
        delta_n = iter_delta_n(delta_n, zoom, x_n, delta_0, zoom_0);
        if step % 32 == 0 {
            rebase_fractexp(&mut delta_n, &mut zoom);
            rebase_fractexp(&mut z_n_prime, &mut zoom_prime);
        }
    }

    // update the output values
    let step = 0;
    let mut output = BufferValues {
        delta_n,
        zoom,
        ref_iteration,
        z_n_prime,
        zoom_prime,
        orbits,
        stripes,
        step,
    };
    let x_n: Vec2f = probed_point[ref_iteration as usize];
    let y_n: Vec2f = x_n + scaled(delta_n, zoom);
    let radius_squared = y_n.x * y_n.x + y_n.y * y_n.y;
    let internal = radius_squared < 4.0;

    if complete || params.iter_offset + params.chunk_max_iter >= params.max_iter {
        output.delta_n = y_n;

        if internal {
            // intermediate_step[buffer_index] = -i32(min_iter);
            output.step = -(min_iter as i32);
            // z_grid_prime[buffer_index] = vec3(total_angle);
            output.z_n_prime = Vec2::splat(total_angle);
            // stripes_buffer[buffer_index] = stripes / f32(params.iter_offset + step);
            output.stripes = stripes / (params.iter_offset + outer_step) as f32;
        } else {
            // intermediate_step[buffer_index] = i32(params.iter_offset + step);
            output.step = (params.iter_offset + outer_step) as i32;
            // z_grid_prime[buffer_index] = vec3(z_n_prime * 2.0.powf( zoom_prime + zoom_0), -zoom_0);
            output.z_n_prime = z_n_prime * 2.0f32.powf(zoom_prime + zoom_0);
            output.zoom_prime = -zoom_0;
            let frac = step_frac(radius_squared, ESCAPE_RADIUS);
            // stripes_buffer[buffer_index] = stripes / f32(params.iter_offset + step) * frac + prev_stripes / f32(params.iter_offset + step - 1) * (1.0 - frac);
            output.stripes = stripes / (params.iter_offset + outer_step) as f32 * frac
                + prev_stripes / (params.iter_offset + outer_step - 1) as f32 * (1.0 - frac);
        }
    }
    return output;
}
