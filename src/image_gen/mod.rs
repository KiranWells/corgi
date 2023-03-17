/*!
# Image Generation

This module contains all logic for creating images of the mandelbrot set to display on screen.

The [`render_thread`] function is the main entry point for the image generation process.
It is responsible for receiving messages from the main thread, and sending the resulting
images back to the main thread.
 */

mod gpu_setup;
mod probe;

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::debug;
use wgpu::Extent3d;

use crate::types::{ComputeParams, Image, RenderParams, Status, Viewport, MAX_GPU_GROUP_ITER};
use probe::probe;

pub use gpu_setup::GPUData;
use probe::generate_delta_grid;

// #[cfg(debug_assertions)]
macro_rules! time {
    ($name:literal, $expression:expr) => {{
        let start = std::time::Instant::now();
        let result = $expression;
        let elapsed = start.elapsed();
        debug!("{} done in {:?}", $name, elapsed);
        result
    }};
}

// #[cfg(not(debug_assertions))]
// macro_rules! time {
//     ($name:literal, $expression:expr) => {{
//         $expression
//     }};
// }

/// Main entry point for the image generation process. This should be called in a separate thread,
/// and will run until the given message channel is closed. `status` is used to communicate the
/// current status of the render process to the main thread.
pub async fn render_thread(
    mut message_channel: mpsc::Receiver<Image>,
    status: Arc<Mutex<Status>>,
    mut gpu_data: GPUData,
) {
    let mut image_tmp = Image::default();
    image_tmp.viewport.width = 1024;
    image_tmp.viewport.height = 1024;
    let mut probed_data = (vec![], vec![]);
    let mut delta_grid = vec![];

    // values for determining whether to re-run steps
    let mut resize;
    let mut reprobe;
    let mut regenerate_delta;
    let mut recompute;
    let mut recolor;

    while let Some(mut image) = message_channel.recv().await {
        // clear the message channel queue and only process the last message
        while let Ok(image_tmp) = message_channel.try_recv() {
            image = image_tmp;
        }
        debug!("Received image: {:?}", image);

        let last_image = { status.lock().await.rendered_image.clone() };
        if let Some(last_image) = &last_image {
            // if the image has not changed, skip the render
            if &image == last_image {
                continue;
            }
            // if the viewport has changed, resize the GPU data
            resize = image.viewport.width != last_image.viewport.width
                || image.viewport.height != last_image.viewport.height;
            // if the max iteration or probe location has changed, re-run the probe
            reprobe = image.max_iter != last_image.max_iter
                || image.probe_location.0 != last_image.probe_location.0
                || image.probe_location.1 != last_image.probe_location.1;
            // if the probe location has changed or the image viewport has changed, re-generate the delta grid
            regenerate_delta = image.viewport != last_image.viewport || reprobe;
            // if the image generation parameters have changed, re-run the compute shader
            recompute = image.max_iter != last_image.max_iter || regenerate_delta || reprobe;
            // if the image coloring parameters have changed, re-run the image render
            recolor =
                image.coloring != last_image.coloring || recompute || image.misc != last_image.misc;
        } else {
            // if there is no last image, re-run everything
            resize = true;
            reprobe = true;
            regenerate_delta = true;
            recompute = true;
            recolor = true;
        }

        // the actual image generation process
        // - resize the GPU data
        // - probe the point
        // - generate the delta grid
        // - run the compute shader
        // - run the image render

        if resize {
            gpu_data.resize(&image.viewport).await;
        }

        if reprobe {
            // probe the point
            probed_data = time!(
                "Probing point",
                probe::<f32>(&image.probe_location, image.max_iter, image.viewport.zoom)
            );
        }

        if regenerate_delta {
            // generate the delta grid
            delta_grid = time!(
                "Generating delta grid",
                generate_delta_grid::<f32>(&image.probe_location, &image.viewport)
            );
        }

        if resize || regenerate_delta {
            // if the image has been resized, but the probe location has not changed, copy the delta grid to the GPU
            gpu_data.queue.write_buffer(
                &gpu_data.buffers.delta_0,
                0,
                bytemuck::cast_slice(&delta_grid),
            );
        }

        if recompute {
            time!(
                "Running compute shader",
                run_compute_step(&probed_data, &image.viewport, &gpu_data, status.clone()).await
            );
        }

        if recolor {
            time!("Running image render", run_render_step(&image, &gpu_data));
        }

        let mut status = status.lock().await;
        status.message = "Finished Rendering".to_string();
        status.progress = None;
        status.rendered_image = Some(image);
    }
}

/// Runs the compute shader on the GPU. This is the most expensive step, so the output
/// should be cached as much as possible. This step only needs to be run if the probe
/// location, max iteration, or image viewport has changed.
async fn run_compute_step(
    probed_data: &(Vec<[f32; 2]>, Vec<[f32; 2]>),
    viewport: &Viewport,
    gpu_data: &GPUData,
    status: Arc<Mutex<Status>>,
) {
    let GPUData {
        device,
        queue,
        bind_groups,
        compute_pipeline,
        buffers,
        ..
    } = gpu_data;
    let texture_size: Extent3d = viewport.into();
    let probe_len = probed_data.0.len();
    debug_assert!(probe_len == probed_data.1.len());

    // Compute passes have encountered timeouts on some GPUs, so we split the compute passes into
    // multiple smaller passes.
    for i in 0..=(probe_len / MAX_GPU_GROUP_ITER) {
        // Create encoder for CPU - GPU communication
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

        // Begin compute dispatch
        {
            let mut cpass =
                encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None });
            cpass.set_bind_group(0, &bind_groups.compute_buffers, &[]);
            cpass.set_bind_group(1, &bind_groups.compute_parameters, &[]);
            cpass.set_pipeline(compute_pipeline);
            cpass.dispatch_workgroups(
                (texture_size.width as f64 / 16.0).ceil() as u32,
                (texture_size.height as f64 / 16.0).ceil() as u32,
                1,
            );
        }

        let command_buffer = encoder.finish();
        // Update the parameters
        let parameters = ComputeParams {
            width: texture_size.width,
            height: texture_size.height,
            max_iter: probe_len as u32,
            probe_len: if probe_len >= (i + 1) * MAX_GPU_GROUP_ITER {
                MAX_GPU_GROUP_ITER as u32
            } else {
                (probe_len % MAX_GPU_GROUP_ITER) as u32
            },
            iter_offset: (i * MAX_GPU_GROUP_ITER) as u32,
        };
        if parameters.probe_len == 0 {
            break;
        }
        queue.write_buffer(
            &buffers.compute_parameters,
            0,
            bytemuck::cast_slice(&[parameters]),
        );

        // update the probe buffer
        queue.write_buffer(
            &buffers.probe,
            0,
            bytemuck::cast_slice(
                &probed_data.0[i * MAX_GPU_GROUP_ITER
                    ..i * MAX_GPU_GROUP_ITER + parameters.probe_len as usize],
            ),
        );
        queue.write_buffer(
            &buffers.probe,
            MAX_GPU_GROUP_ITER as u64 * 8,
            bytemuck::cast_slice(
                &probed_data.1[i * MAX_GPU_GROUP_ITER
                    ..i * MAX_GPU_GROUP_ITER + parameters.probe_len as usize],
            ),
        );

        // submit the compute shader command buffer
        time!("queue submit", queue.submit(Some(command_buffer)));
        let mut status = status.lock().await;
        status.message = format!(
            "Rendering {} of {} iterations",
            i * MAX_GPU_GROUP_ITER + parameters.probe_len as usize,
            probe_len
        );
        status.progress = Some(
            (i * MAX_GPU_GROUP_ITER + parameters.probe_len as usize) as f64 / probe_len as f64,
        );
    }
}

/// Runs the render shader on the GPU
fn run_render_step(image: &Image, gpu_data: &GPUData) {
    let GPUData {
        device,
        queue,
        bind_groups,
        buffers,
        render_pipeline_layout,
        ..
    } = gpu_data;
    let color_params: RenderParams = image.into();
    queue.write_buffer(
        &buffers.render_parameters,
        0,
        bytemuck::cast_slice(&[color_params]),
    );

    // select the render shader
    let render_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Render Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/color.wgsl").into()),
    });

    // create the render pipeline
    let render_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(render_pipeline_layout),
        module: &render_shader,
        entry_point: "main_color",
    });

    // create encoder for CPU - GPU communication
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    // begin render dispatch
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None });
        cpass.set_bind_group(0, &bind_groups.render_buffers, &[]);
        cpass.set_bind_group(1, &bind_groups.render_texture, &[]);
        cpass.set_bind_group(2, &bind_groups.render_parameters, &[]);
        cpass.set_pipeline(&render_pipeline);
        cpass.dispatch_workgroups(
            (image.viewport.width as f64 / 16.0).ceil() as u32,
            (image.viewport.height as f64 / 16.0).ceil() as u32,
            1,
        );
    }

    // submit the render command queue
    queue.submit(Some(encoder.finish()));
}
