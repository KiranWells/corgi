use std::fmt::Debug;

use crate::types::{ComplexPoint, ESCAPE_RADIUS, get_precision};
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
pub fn probe<T>(
    ComplexPoint { x, y }: &ComplexPoint,
    max_iter: u64,
    zoom: f64,
    julia_point: Option<&ComplexPoint>,
) -> Vec<[T; 2]>
where
    T: FromFloat + Debug,
{
    let mut probed_point = Vec::new();
    let precision = get_precision(zoom);

    // c = x + yi
    let mut c_real = Float::with_val(precision, x);
    let mut c_imag = Float::with_val(precision, y);

    // z = 0 + 0i
    let mut z_real = Float::with_val(precision, 0.0);
    let mut z_imag = Float::with_val(precision, 0.0);
    if let Some(ComplexPoint { x: r, y: i }) = julia_point {
        z_real = c_real.clone();
        z_imag = c_imag.clone();
        c_real = Float::with_val(precision, r);
        c_imag = Float::with_val(precision, i);
    }

    // z^2 (temp values for optimized computation)
    let mut z_squared_real = z_real.clone() * z_real.clone();
    let mut z_squared_imag = z_imag.clone() * z_imag.clone();

    probed_point.push([T::from_float(&z_real), T::from_float(&z_imag)]);
    for _step in 0..max_iter - 1 {
        // iterate values, according to z = z^2 + c
        //
        // uses an optimized computation method from wikipedia for z:
        //   z.i := 2 × z.r × z.i + c.i
        //   z.r := r2 - i2 + c.r
        //   r2 := z.r × z.r
        //   i2 := z.i × z.i

        // compute z
        z_imag = (z_real.clone() + z_real.clone()) * z_imag.clone() + c_imag.clone();
        z_real = z_squared_real.clone() - z_squared_imag.clone() + c_real.clone();

        // compute z^2
        z_squared_real = z_real.clone() * z_real.clone();
        z_squared_imag = z_imag.clone() * z_imag.clone();

        probed_point.push([T::from_float(&z_real), T::from_float(&z_imag)]);

        let radius_squared = z_squared_real.to_f64() + z_squared_imag.to_f64();

        if radius_squared > ESCAPE_RADIUS {
            break;
        }
    }

    probed_point
}
