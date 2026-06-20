//! This is a non-perturbed version of the fractal compute step.
//! It is used for increased performance at low zoom values, and
//! as a sanity check to ensure the perturbed algorithm is correct.
use super::super::types::{BufferValues, ComputeParams};
use super::super::utils::{
    DERIVATIVE_ENABLED, ESCAPE_RADIUS, JULIA, ORBIT_ENABLED, STRIPES_ENABLED, TOTAL_ANGLE_ENABLED,
    aspect, get_orbit_values, get_stripe_values, rotation_matrix, step_frac,
};
use crate::shared::wgsl_primitives::*;

pub fn calculate_point(
    global_id: Vec2u,
    initial_values: BufferValues,
    flags: u32,
    params: ComputeParams,
    _probed_point: &[Vec2f],
) -> BufferValues {
    let aspect_scale = aspect(params.width, params.height);
    let offset = Vec2::new(params.x, params.y)
        + (Vec2::new(
            global_id.x as f32 / params.width as f32,
            global_id.y as f32 / params.height as f32,
        ) - 0.5f32)
            * 2.0f32
            * 2.0f32.powf(-params.zoom)
            * aspect_scale
            * rotation_matrix(params.angle);

    // initial iteration values
    #[expect(unused_mut)]
    let mut z_0: Vec2f;
    if (flags & JULIA) != 0 {
        z_0 = Vec2::new(params.julia_x, params.julia_y);
    } else {
        z_0 = offset;
    }
    let mut z_n: Vec2f;
    if (flags & JULIA) != 0 {
        z_n = offset;
    } else {
        z_n = Vec2::splat(0.0);
    }
    let mut z_n_prime = Vec2::new(1.0, 0.0);
    let mut orbits = Vec4::splat(ESCAPE_RADIUS);
    let mut stripes = Vec4::splat(0.0);

    if params.iter_offset != 0u32 {
        z_n = initial_values.delta_n;
        z_n_prime = initial_values.z_n_prime;
        orbits = initial_values.orbits;
        stripes = initial_values.stripes;
    };

    // reference values for detecting orbit cycles
    let mut z_old = Vec2::splat(ESCAPE_RADIUS);
    // internal coloring values
    let mut closest = z_0.length();
    let mut min_iter = 1u32;
    let mut line = Vec2::splat(0.0);
    let mut angles = 0.0;
    let mut total_angle = 0.0;

    // stripe temporary values
    let mut stripes_started = true;
    let mut prev_stripes = Vec4f::splat(0.0);

    // iteration trackers
    let mut complete = false;
    let mut outer_step = 0u32;

    for step in 0u32..params.chunk_max_iter {
        outer_step = step;
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
        if (flags & ORBIT_ENABLED) != 0 && step + params.iter_offset > 1u32 {
            let new_orbits = get_orbit_values(z_n);
            orbits = orbits.min(new_orbits);
        }

        // test if the point is already outside the escape radius
        // or that we are repeating a cycle
        if radius_squared > ESCAPE_RADIUS || (z_n.x == z_old.x && z_n.y == z_old.y) {
            // update the output values
            complete = true;
            break;
        }
        #[expect(clippy::manual_is_multiple_of)]
        if step >= 100u32 && (step - 100u32) % 1024u32 == 0 {
            z_old = z_n;
        }

        let previous = z_n;

        // iterate values, according to z = z^2 + c
        // z' is calculated according to the standard formula (z' = 2*z*z' + 1):
        if (flags & DERIVATIVE_ENABLED) != 0 {
            z_n_prime = Vec2::new(
                2.0 * (z_n.x * z_n_prime.x - z_n.y * z_n_prime.y) + 1.0,
                2.0 * (z_n.y * z_n_prime.x + z_n.x * z_n_prime.y),
            );
        }
        z_n = Vec2::new(
            z_n.x * z_n.x - z_n.y * z_n.y + z_0.x,
            (z_n.x + z_n.x) * z_n.y + z_0.y,
        );

        // track total angle for a cycle
        if (flags & TOTAL_ANGLE_ENABLED) != 0 {
            if !(step == 0u32 && params.iter_offset == 0) {
                let new_normalized = (z_n - previous).normalize();
                angles += new_normalized.dot(line).acos();
                line = (previous - z_n).normalize();
                let distance = (z_n - z_0).length();
                if distance < closest {
                    total_angle = angles;
                    min_iter = step + params.iter_offset + 1u32;
                    closest = distance;
                }
            } else {
                line = (previous - z_n).normalize();
            }
        }
    }

    // update the output values
    let step = 0;
    let delta_n = z_n;
    let zoom = 0.0;
    let zoom_prime = 0.0;
    let ref_iteration: u32 = 0;
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
    let radius_squared = z_n.x * z_n.x + z_n.y * z_n.y;
    let internal = radius_squared < 4.0;

    if complete || params.iter_offset + params.chunk_max_iter >= params.max_iter {
        if internal {
            output.step = -(min_iter as i32);
            output.z_n_prime = Vec2::splat(total_angle);
            output.stripes = stripes / (params.iter_offset + outer_step) as f32;
        } else {
            output.step = (params.iter_offset + outer_step) as i32;
            output.z_n_prime = z_n_prime * 2.0f32.powf(-params.zoom);
            output.zoom_prime = -params.zoom;
            let frac = step_frac(radius_squared, ESCAPE_RADIUS);
            output.stripes = stripes / (params.iter_offset + outer_step) as f32 * frac
                + prev_stripes / (params.iter_offset + outer_step - 1) as f32 * (1.0 - frac);
        }
    }
    return output;
}
