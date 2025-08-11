use std::fmt::Debug;

use crate::types::{ESCAPE_RADIUS, Viewport, get_precision};
use rayon::prelude::{IntoParallelIterator, ParallelIterator};
use rug::Float;

/// # FromFloat
/// A trait to convert a `rug::Float` to another type.
/// This allows being generic over the float type used.
pub trait FromFloat {
    /// Converts a `rug::Float` to this type.
    fn from_float(x: &Float) -> Self;
}

impl FromFloat for f64 {
    fn from_float(x: &Float) -> f64 {
        x.to_f64()
    }
}

impl FromFloat for f32 {
    fn from_float(x: &Float) -> f32 {
        x.to_f32()
    }
}

/// Generates a vector of iterated points for a given complex number in the mandelbrot set.
/// The resulting vector will be of length `max_iter` or less if the point escapes.
pub fn probe<T>((x, y): &(Float, Float), max_iter: usize, zoom: f64) -> (Vec<[T; 2]>, Vec<[T; 2]>)
where
    T: FromFloat + Debug,
{
    let mut probed_point = Vec::new();
    let mut probed_point_derivative = Vec::new();
    let precision = get_precision(zoom);

    // c = x + yi
    let c_real = Float::with_val(precision, x);
    let c_imag = Float::with_val(precision, y);

    // z = 0 + 0i
    let mut z_real = Float::with_val(precision, 0.0);
    let mut z_imag = Float::with_val(precision, 0.0);

    // z' = 1 + 0i
    let mut z_prime_real = Float::with_val(precision, 1.0);
    let mut z_prime_imag = Float::with_val(precision, 0.0);

    // z^2 (temp values for optimized computation)
    let mut z_squared_real = Float::with_val(precision, 0.0);
    let mut z_squared_imag = Float::with_val(precision, 0.0);

    for _step in 0..max_iter {
        // iterate values, according to z = z^2 + c
        //
        // uses an optimized computation method from wikipedia for z:
        //   z.i := 2 × z.r × z.i + c.i
        //   z.r := r2 - i2 + c.r
        //   r2 := z.r × z.r
        //   i2 := z.i × z.i

        // compute z'
        // z' = 2 × z × z' + 1
        // z'.r = 2 × (z.r × z'.r - z.i × z.r) + 1
        // z'.i = 2 × (z.r × z'.i + z.i × z'.r)
        let ac_bd = z_real.clone() * z_prime_real.clone() - z_imag.clone() * z_prime_imag.clone();
        let bc_ad = z_imag.clone() * z_prime_real.clone() + z_real.clone() * z_prime_imag.clone();
        z_prime_real = ac_bd.clone() + ac_bd.clone() + 1.0;
        z_prime_imag = bc_ad.clone() + bc_ad.clone();

        // compute z
        z_imag = (z_real.clone() + z_real.clone()) * z_imag.clone() + c_imag.clone();
        z_real = z_squared_real.clone() - z_squared_imag.clone() + c_real.clone();

        // compute z^2
        z_squared_real = z_real.clone() * z_real.clone();
        z_squared_imag = z_imag.clone() * z_imag.clone();

        probed_point.push([T::from_float(&z_real), T::from_float(&z_imag)]);
        probed_point_derivative.push([T::from_float(&z_prime_real), T::from_float(&z_prime_imag)]);

        let radius_squared = z_squared_real.to_f64() + z_squared_imag.to_f64();

        if radius_squared > ESCAPE_RADIUS {
            break;
        }
    }

    (probed_point, probed_point_derivative)
}

/// Generates a grid of complex delta values for a given complex number in the mandelbrot set.
/// The resulting vector will be of length `image.width * image.height`.
///
/// The delta values represent the difference between the `probe_point` and the complex number
/// at the corresponding pixel in the image.
pub fn generate_delta_grid<T: FromFloat + Send>(
    probe_point: &(Float, Float),
    image: &Viewport,
) -> Vec<[T; 2]> {
    let precision = get_precision(image.zoom) * 2;

    let probe_real = Float::with_val(precision, &probe_point.0);
    let probe_imag = Float::with_val(precision, &probe_point.1);

    (0..(image.width * image.height))
        .into_par_iter()
        .map(move |n| {
            let i = n % image.width;
            let j = n / image.width;

            let (z_real, z_imag) = image.get_real_coords(i as f64, j as f64);

            let delta_n_r = z_real - probe_real.clone();
            let delta_n_i = z_imag - probe_imag.clone();
            [T::from_float(&delta_n_r), T::from_float(&delta_n_i)]
        })
        .collect()
}
