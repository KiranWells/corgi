//! This is a non-perturbed version of the fractal compute step.
//! It is used for increased performance at low zoom values, and
//! as a sanity check to ensure the perturbed algorithm is correct.
use rug::Float;
use rug::ops::Pow;

use crate::shared::types::ComputeParams;
use crate::shared::utils::{
    DERIVATIVE_ENABLED, ESCAPE_RADIUS, JULIA, ORBIT_ENABLED, STRIPES_ENABLED, TOTAL_ANGLE_ENABLED,
    aspect, rotation_matrix,
};
use crate::shared::wgsl_primitives::*;
use crate::types::{ComplexPoint, get_precision};

pub fn length_squared(v: Vec2<Float>) -> Float {
    v.x.clone() * v.x + v.y.clone() * v.y
}

pub fn get_stripe_values(z_n: Vec2<Float>) -> Vec4<Float> {
    if z_n.x == 0.0 && z_n.y == 0.0 {
        return Vec4::splat(Float::new(53));
    }
    let tan = z_n.x.clone().atan2(&z_n.y);
    Vec4::new(
        0.5 + 0.5 * Float::with_val(tan.prec(), 5.0 * tan).sin(),
        0.5 + 0.5 * z_n.normalize().x,
        0.5 + 0.5 * z_n.normalize().y,
        Float::new(53),
    )
}

pub fn get_orbit_values(z_n: Vec2<Float>) -> Vec4<Float> {
    let len = length_squared(z_n.clone());
    let abs = z_n.abs();
    Vec4::new(
        len.clone(),
        Float::with_val(len.prec(), len - 2.0).abs(),
        (abs.x.clone()).min(&abs.y),
        Float::with_val(abs.x.prec(), abs.x + abs.y - 2.0).abs(),
    )
}

pub fn step_frac(start_radius_squared: Float, end_radius_squared: Float) -> Float {
    let start_log = start_radius_squared.ln();
    let end_log = end_radius_squared.ln();
    Float::with_val(
        end_log.prec(),
        -1.0 + Float::with_val(end_log.prec(), 2.0 * end_log).log2()
            - Float::with_val(start_log.prec(), 0.5 * start_log).log2(),
    )
}

#[derive(Clone, Debug)]
pub struct BufferValues {
    pub delta_n: Vec2<Float>,
    pub zoom: f32,
    pub ref_iteration: u32,
    pub z_n_prime: Vec2<Float>,
    pub zoom_prime: f32,
    pub orbits: Vec4<Float>,
    pub stripes: Vec4<Float>,
    pub step: i32,
}

impl BufferValues {
    pub fn zero() -> Self {
        Self {
            delta_n: Vec2::splat(Float::new(53)),
            zoom: 0.0,
            ref_iteration: 0,
            z_n_prime: Vec2::splat(Float::new(53)),
            zoom_prime: 0.0,
            orbits: Vec4::splat(Float::new(53)),
            stripes: Vec4::splat(Float::new(53)),
            step: 0,
        }
    }

    pub fn to_f32(&self) -> crate::shared::types::BufferValues {
        crate::shared::types::BufferValues {
            delta_n: self.delta_n.to_f32(),
            zoom: self.zoom,
            ref_iteration: self.ref_iteration,
            z_n_prime: self.z_n_prime.to_f32(),
            zoom_prime: self.zoom_prime,
            orbits: self.orbits.to_f32(),
            stripes: self.stripes.to_f32(),
            step: self.step,
        }
    }
}

pub fn calculate_point(
    global_id: Vec2u,
    initial_values: BufferValues,
    flags: u32,
    params: ComputeParams,
    center: ComplexPoint,
) -> BufferValues {
    let precision = get_precision(params.zoom);
    let aspect_scale = aspect(params.width, params.height);
    let offset = (Vec2::new(
        Float::with_val(precision, (global_id.x as f64 + 0.5) / params.width as f64),
        Float::with_val(precision, (global_id.y as f64 + 0.5) / params.height as f64),
    ) - 0.5_f64)
        * 2.0_f64
        * aspect_scale
        * rotation_matrix(params.angle);
    let offset =
        Vec2::new(center.x, center.y) + offset * Float::with_val(precision, 2.0).pow(-params.zoom);
    let one = Float::with_val(precision, 1.0);
    let zero = Float::new(precision);

    // initial iteration values
    let z_0 = if (flags & JULIA) != 0 {
        Vec2::new(
            Float::with_val(precision, params.julia_x),
            Float::with_val(precision, params.julia_y),
        )
    } else {
        offset.clone()
    };
    let mut z_n = if (flags & JULIA) != 0 {
        offset
    } else {
        Vec2::splat(Float::new(precision))
    };
    let mut z_n_prime = Vec2::new(one.clone(), zero.clone());
    let mut orbits = Vec4::splat(Float::with_val(precision, ESCAPE_RADIUS));
    let mut stripes = Vec4::splat(zero.clone());

    if params.iter_offset != 0u32 {
        z_n = initial_values.delta_n;
        z_n_prime = initial_values.z_n_prime;
        orbits = initial_values.orbits;
        stripes = initial_values.stripes;
    };

    // reference values for detecting orbit cycles
    let mut z_old = Vec2::splat(Float::with_val(precision, ESCAPE_RADIUS));
    // internal coloring values
    let mut closest = z_0.length();
    let mut min_iter = 1u32;
    let mut line = Vec2::splat(zero.clone());
    let mut angles = Float::new(53);
    let mut total_angle = Float::new(53);

    // stripe temporary values
    let mut stripes_started = true;
    let mut prev_stripes = Vec4::splat(zero.clone());

    // iteration trackers
    let mut complete = false;
    let mut outer_step = 0u32;

    for step in 0u32..params.chunk_max_iter {
        outer_step = step;
        let radius_squared = Float::with_val(precision, &z_n.x * &z_n.x + &z_n.y * &z_n.y);
        // calculate stripe averages and orbit traps
        if (flags & STRIPES_ENABLED) != 0 && (stripes_started || radius_squared > 64.0) {
            prev_stripes = stripes.clone();
            stripes += get_stripe_values(z_n.clone());
            if !stripes_started {
                stripes *= 1.0 - step_frac(radius_squared.clone(), Float::with_val(53, 64.0));
                stripes_started = true;
            }
        }
        if (flags & ORBIT_ENABLED) != 0 && step + params.iter_offset > 1u32 {
            let new_orbits = get_orbit_values(z_n.clone());
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
            z_old = z_n.clone();
        }

        let previous = z_n.clone();

        // iterate values, according to z = z^2 + c
        // z' is calculated according to the standard formula (z' = 2*z*z' + 1):
        if (flags & DERIVATIVE_ENABLED) != 0 {
            z_n_prime = Vec2::new(
                2.0 * (z_n.x.clone() * z_n_prime.x.clone() - z_n.y.clone() * z_n_prime.y.clone())
                    + 1.0,
                2.0 * (z_n.y.clone() * z_n_prime.x.clone() + z_n.x.clone() * z_n_prime.y.clone()),
            );
        }
        z_n = Vec2::new(
            z_n.x.clone() * z_n.x.clone() - z_n.y.clone() * z_n.y.clone() + z_0.x.clone(),
            (z_n.x.clone() + z_n.x.clone()) * z_n.y.clone() + z_0.y.clone(),
        );

        // track total angle for a cycle
        if (flags & TOTAL_ANGLE_ENABLED) != 0 {
            if !(step == 0u32 && params.iter_offset == 0) {
                let new_normalized = (z_n.clone() - previous.clone()).normalize();
                angles += new_normalized.dot(line).acos();
                line = (previous - z_n.clone()).normalize();
                let distance = (z_n.clone() - z_0.clone()).length();
                if distance < closest {
                    total_angle = angles.clone();
                    min_iter = step + params.iter_offset + 1u32;
                    closest = distance;
                }
            } else {
                line = (previous - z_n.clone()).normalize();
            }
        }
    }

    // update the output values
    let step = 0;
    let delta_n = z_n.clone();
    let zoom = 0.0;
    let zoom_prime = 0.0;
    let ref_iteration: u32 = 0;
    let mut output = BufferValues {
        delta_n,
        zoom,
        ref_iteration,
        z_n_prime: z_n_prime.clone(),
        zoom_prime,
        orbits,
        stripes: stripes.clone(),
        step,
    };
    let radius_squared = z_n.x.clone() * z_n.x.clone() + z_n.y.clone() * z_n.y.clone();
    let internal = radius_squared < 4.0;

    if complete || params.iter_offset + params.chunk_max_iter >= params.max_iter {
        if internal {
            output.step = -(min_iter as i32);
            output.z_n_prime = Vec2::splat(total_angle);
            output.stripes = stripes / (params.iter_offset + outer_step) as f32;
        } else {
            output.step = (params.iter_offset + outer_step) as i32;
            let prec = z_n_prime.x.prec();
            output.z_n_prime = z_n_prime * Float::with_val(prec, 2.0).pow(-params.zoom);
            output.zoom_prime = -params.zoom;
            let frac = step_frac(radius_squared, Float::with_val(53, ESCAPE_RADIUS));
            output.stripes = stripes / (params.iter_offset + outer_step) as f32 * frac.clone()
                + prev_stripes / (params.iter_offset + outer_step - 1) as f32 * (1.0 - frac);
        }
    }
    output
}
