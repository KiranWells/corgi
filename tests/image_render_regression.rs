use std::path::Path;
use std::sync::LazyLock;

use corgi_lib::image_gen::{Constants, Engine, ProgressUpdate, SharedState, get_device_and_queue};
use corgi_lib::types::serde::SafeSaveLoad;
use corgi_lib::types::{ImgSpec, OptLevel};
use image::{DynamicImage, GenericImage, ImageBuffer};
use parking_lot::Mutex;
use pollster::FutureExt;
use rayon::iter::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefIterator, ParallelIterator,
};

use crate::common::test_output_dir;

mod common;

const TEST_IMAGE_SIZE: u32 = 100;
static TEST_ENGINE: LazyLock<Mutex<Engine>> = LazyLock::new(|| {
    let (device, queue) = get_device_and_queue().block_on().unwrap();
    Mutex::new(Engine::init(
        SharedState::new(device, queue),
        "cli renderer",
        wgpu::Extent3d {
            width: TEST_IMAGE_SIZE,
            height: TEST_IMAGE_SIZE,
            depth_or_array_layers: 1,
        },
        10_000,
        Constants {
            iter_batch_size: 10_000,
        },
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    ))
});

macro_rules! regression_test {
    ($(#[$ignore:meta])? $name:ident) => {
        $(#[$ignore])?
        #[test]
        fn $name() {
            let image_path = Path::new("tests/regression_images")
                .join(stringify!($name))
                .with_added_extension("png");
            let img = render_img(&image_path, OptLevel::AccuracyOptimized);
            let reference_img = image::open(image_path).unwrap();
            test_image_error(img, reference_img, stringify!($name));
        }
    };
}

regression_test!(anodized_traces);
regression_test!(blue_pearls);
regression_test!(engraved_arcs);
regression_test!(ion_flame);
regression_test!(map_lines);
regression_test!(mint_swirls);
regression_test!(neon_rainbow);
regression_test!(rusting_terraces);
regression_test!(sample_fractal);
regression_test!(violet_flows);
// TODO: enable after implementing support for multisampling
regression_test!(
    #[ignore = "image is too noisy for image comparison"]
    v0_splash
);

fn render_img(image_path: &Path, opt_level: OptLevel) -> DynamicImage {
    let mut image = ImgSpec::load(image_path).unwrap();
    image.optimization_level = opt_level;
    let mut status_callback = |pu: ProgressUpdate| {
        if let Some(percent) = pu.progress {
            println!("{:>6.2}% | {}", percent * 100.0, pu.message);
        } else {
            println!("------- | {}", pu.message);
        }
    };
    let mut engine = TEST_ENGINE.lock();
    let timings = engine.render_image(&image, &mut status_callback).unwrap();
    print!("\n------- | Rendering timings: {}", timings);
    engine.get_image().unwrap()
}

fn test_image_error(img: DynamicImage, reference_img: DynamicImage, test_name: &str) {
    let width = img.width();
    let height = img.height();
    let comp_data = img
        .clone()
        .into_rgba8()
        .par_pixels()
        .into_par_iter()
        .zip(
            reference_img
                .clone()
                .into_rgba8()
                .par_pixels()
                .into_par_iter(),
        )
        .flat_map(|(p, rp)| {
            [
                rp.0[0] / 2 + 128 - (p.0[0] / 2),
                rp.0[1] / 2 + 128 - (p.0[1] / 2),
                rp.0[2] / 2 + 128 - (p.0[2] / 2),
            ]
        })
        .collect::<Vec<u8>>();
    let error = comp_data
        .par_iter()
        .map(|x| x.abs_diff(128) as u64)
        .sum::<u64>() as f64
        / (height * width * 3 * 255) as f64;
    if error > 0.002 {
        let comp_image = image::DynamicImage::ImageRgb8(
            ImageBuffer::from_raw(width, height, comp_data).unwrap(),
        );
        let mut combined_buf = image::ImageBuffer::new(width * 3, height);
        combined_buf
            .copy_from(&reference_img.into_rgb8(), 0, 0)
            .unwrap();
        combined_buf.copy_from(&img.into_rgb8(), width, 0).unwrap();
        combined_buf
            .copy_from(comp_image.as_rgb8().unwrap(), width * 2, 0)
            .unwrap();
        let path = test_output_dir().join(format!("{test_name}_test_failure.png"));
        image::DynamicImage::ImageRgb8(combined_buf)
            .save(&path)
            .unwrap();
        panic!(
            "===\nImages don't match! Error: {:.2}% > 0.1%\n\nImage diff written to `{}`\n===",
            error * 100.,
            path.to_string_lossy()
        );
    }
}
