use std::io::Write;

use crate::types::{Viewport, ESCAPE_RADIUS};

pub fn generated_probed(
    probed_point: Vec<(f64, f64)>,
    grid: Vec<(f64, f64)>,
    image: Viewport,
) -> Vec<u8> {
    let mut generated_image = Vec::with_capacity(image.width * image.height);
    let mut max_step = 0;
    // generate the image based on the perturbation formula from the pre-computed probed point
    for j in 0..image.height {
        for i in 0..image.width {
            let (mut delta_n_r, mut delta_n_i) = grid[i + j * image.width];
            let delta_n_0_r = delta_n_r;
            let delta_n_0_i = delta_n_i;
            let mut step = probed_point.len();
            for (i, (x_n_r, x_n_i)) in probed_point.iter().enumerate() {
                // perturbation formula
                // delta_n = 2 * x_n * delta_n - delta_n^2 + delta_n_0
                let y_n_r = x_n_r + delta_n_r;
                let y_n_i = x_n_i + delta_n_i;
                let radius_squared = y_n_r * y_n_r + y_n_i * y_n_i;
                if radius_squared > ESCAPE_RADIUS {
                    step = i;
                    // println!("step: {} ", step);
                    max_step = max_step.max(step);
                    break;
                }
                let delta_n_r_tmp = 2.0 * x_n_r * delta_n_r - 2.0 * x_n_i * delta_n_i
                    + delta_n_r * delta_n_r
                    - delta_n_i * delta_n_i
                    + delta_n_0_r;
                let delta_n_i_tmp = 2.0 * x_n_r * delta_n_i
                    + 2.0 * x_n_i * delta_n_r
                    + 2.0 * delta_n_r * delta_n_i
                    + delta_n_0_i;
                delta_n_r = delta_n_r_tmp;
                delta_n_i = delta_n_i_tmp;
            }
            generated_image.push(step);
        }
        // print a line and use terminal escape sequence to move the cursor to the beginning of the line
        // so that the line will be overwritten
        print!("\rpercent: {:.1} ", j as f64 / image.height as f64 * 100.0);
        std::io::stdout().lock().flush().unwrap();
    }
    println!("max_step: {}", max_step);
    generated_image
        .iter()
        .map(|&x| (x as f64 / max_step as f64 * 255.0) as u8)
        .collect()
}

pub fn generated_probed_abc(probed_point: Vec<(f64, f64)>, image: Viewport) -> Vec<u8> {
    let mut generated_image = Vec::new();
    let scale = 2.0_f64.powf(-image.zoom);
    let mut max_step = 0;
    // generate the image based on the perturbation formula from the pre-computed probed point
    for j in 0..image.height {
        for i in 0..image.width {
            let x = (i as f64 / image.width as f64 - 0.5) * scale + image.x;
            let y = (j as f64 / image.height as f64 - 0.5)
                * scale
                * (image.height as f64 / image.width as f64)
                + image.y;
            let mut delta_n_r = x - probed_point[0].0;
            let mut delta_n_i = y - probed_point[0].1;
            let delta_r = delta_n_r;
            let delta_i = delta_n_i;
            // delta values:
            let delta_ri = delta_r * delta_i;
            let delta_rr = delta_r * delta_r;
            let delta_ii = delta_i * delta_i;
            let delta_rrr = delta_rr * delta_r;
            let delta_iii = delta_ii * delta_i;
            let delta_rii = delta_ii * delta_r;
            let delta_rri = delta_rr * delta_i;

            let mut y_n_r_actual = x;
            let mut y_n_i_actual = y;

            let mut a_n_r = 1.0;
            let mut a_n_i = 1.0;
            let mut b_n_r = 0.0;
            let mut b_n_i = 0.0;
            let mut c_n_r = 0.0;
            let mut c_n_i = 0.0;

            let mut step = probed_point.len();
            for (n, (x_n_r, x_n_i)) in probed_point.iter().enumerate() {
                let y_n_r = x_n_r + delta_n_r;
                let y_n_i = x_n_i + delta_n_i;

                // let radius_squared = y_n_r_actual * y_n_r_actual + y_n_i_actual * y_n_i_actual;
                let radius_squared = y_n_r * y_n_r + y_n_i * y_n_i;
                if radius_squared > ESCAPE_RADIUS {
                    step = n;
                    // println!("step: {} ", step);
                    break;
                }

                if i == 0 && j == 48 {
                    // println!("y_{n:04}_r (y_{n:04}_r_actual) = x_{n:04}_r + delta_{n:04}_r -> {y_n_r} ({y_n_r_actual}) = {x_n_r} + {delta_n_r}");
                    // println!("{y_n_r} ({y_n_r_actual}),\t {y_n_i} ({y_n_i_actual})");
                    println!(
                        "Delta: {delta_n_r}, {delta_n_i} Err: {err_r}, {err_i}",
                        err_r = (y_n_r - y_n_r_actual).abs(),
                        err_i = (y_n_i - y_n_i_actual).abs()
                    );
                    // dump all other values
                    // println!("a_n_r: {a_n_r} a_n_i: {a_n_i} b_n_r: {b_n_r} b_n_i: {b_n_i} c_n_r: {c_n_r} c_n_i: {c_n_i}", a_n_r = a_n_r, a_n_i = a_n_i, b_n_r = b_n_r, b_n_i = b_n_i, c_n_r = c_n_r, c_n_i = c_n_i);
                }

                // calculate a check
                let y_n_r_actual_tmp =
                    y_n_r_actual * y_n_r_actual - y_n_i_actual * y_n_i_actual + x;
                let y_n_i_actual_tmp = 2.0 * y_n_r_actual * y_n_i_actual + y;
                y_n_r_actual = y_n_r_actual_tmp;
                y_n_i_actual = y_n_i_actual_tmp;

                // perturbation formula
                // a_n = 2 * x_n * a_n + 1
                let a_n_r_temp = 2.0 * x_n_r * a_n_r - 2.0 * x_n_i * a_n_i + 1.0;
                let a_n_i_temp = 2.0 * x_n_r * a_n_i + 2.0 * x_n_i * a_n_r;

                // b_n = 2 * x_n * b_n + a_n^2
                let b_n_r_temp =
                    2.0 * x_n_r * b_n_r - 2.0 * x_n_i * b_n_i + a_n_r * a_n_r - a_n_i * a_n_i;
                let b_n_i_temp = 2.0 * x_n_r * b_n_i + 2.0 * x_n_i * b_n_r + 2.0 * a_n_r * a_n_i;

                // c_n = 2 * x_n * c_n + 2 * a_n * b_n
                let c_n_r_temp = 2.0 * x_n_r * c_n_r - 2.0 * x_n_i * c_n_i + 2.0 * a_n_r * b_n_r
                    - 2.0 * a_n_i * b_n_i;
                let c_n_i_temp = 2.0 * x_n_r * c_n_i
                    + 2.0 * x_n_i * c_n_r
                    + 2.0 * a_n_r * b_n_i
                    + 2.0 * a_n_i * b_n_r;

                a_n_r = a_n_r_temp;
                a_n_i = a_n_i_temp;
                b_n_r = b_n_r_temp;
                b_n_i = b_n_i_temp;
                c_n_r = c_n_r_temp;
                c_n_i = c_n_i_temp;

                // delta = delta_n_0
                // delta_n = a_n * delta + b_n * delta^2 + c_n * delta^3 + ...

                // this is a big one, so we need to calculate some intermediate values first

                // a_n values:
                let delta_a_n_r = a_n_r * delta_r - a_n_i * delta_i;
                let delta_a_n_i = a_n_r * delta_i + a_n_i * delta_r;

                // b_n values:
                let delta_b_n_r = b_n_r * (delta_rr - delta_ii) - b_n_i * (2.0 * delta_ri);
                let delta_b_n_i = b_n_r * (2.0 * delta_ri) + b_n_i * (delta_rr - delta_ii);

                // c_n values:
                let delta_c_n_r =
                    c_n_r * (delta_rrr - 3.0 * delta_rii) - c_n_i * (3.0 * delta_rri - delta_iii);
                let delta_c_n_i =
                    c_n_r * (3.0 * delta_rri - delta_iii) + c_n_i * (delta_rrr - 3.0 * delta_rii);

                delta_n_r = delta_a_n_r + delta_b_n_r + delta_c_n_r;
                delta_n_i = delta_a_n_i + delta_b_n_i + delta_c_n_i;
            }
            max_step = max_step.max(step);
            generated_image.push(step);
        }
    }
    println!("max_step: {}", max_step);
    generated_image
        .iter()
        .map(|&x| (x as f64 / max_step as f64 * 255.0) as u8)
        .collect()
}
