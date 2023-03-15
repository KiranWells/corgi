/*!
# Image Generation

This module contains all logic for creating images of the mandelbrot set to display on screen.

## Render Process

- set the initial probe to the center of the image (if not already set)
- Probe the given point, and store the resulting iterations

    ```
    probe::probe(x, y, max_iter)
    ```

- Generate a grid of initial values for the perturbation formula

    ```
    probe::generate_delta_grid(x, y, image)
    ```

- Run a computer render for the perturbation formula (either on the CPU or GPU)

    ```
    gpu_render::run_compute_stage(...)
    ```

    **GPU Render**

    - Initial setup needs to be completed when the program starts
        - device, queue, encoder, buffers, shaders, etc.
    - copy the probe data and delta grid to the GPU in buffers
    - Run the compute shader on the GPU
        - This shader is pre-compiled and stored in the binary, as it will not change
        - The compute shader will store the results in a buffer
    - make the buffers available to bake the image (TODO)

- Recalculate the probe as the point in the grid with the greatest iteration and smallest orbit (TODO)
    - Repeat the compute process with the new probe
- Run an image render for the coloring step (ideally on the GPU)
    - Use the buffers saved in the compute step
    - select the coloring shader (TODO)
    - Run the compute shader on the GPU
    - return the image texture for use in the GUI or saving to disk

The compute shader is the most complex part of the render process, and the result can be cached as long as
the viewport and max_iteration does not change. This means coloring can be changed without re-running the
full process, making it much faster.

 */

mod gpu_setup;
mod probe;

use color_eyre::{eyre::eyre, Result};
use std::sync::{mpsc, Arc, Mutex};

#[cfg(debug_assertions)]
use tracing::debug;

use crate::types::{ComputeParams, Image, Status, MAX_GPU_GROUP_ITER};
use probe::probe;

pub use gpu_setup::GPUData;
use probe::generate_delta_grid;

#[cfg(debug_assertions)]
macro_rules! time {
    ($name:literal, $expression:expr) => {{
        let start = std::time::Instant::now();
        let result = $expression;
        let elapsed = start.elapsed();
        debug!("{} done in {:?}", $name, elapsed);
        result
    }};
}

#[cfg(not(debug_assertions))]
macro_rules! time {
    ($name:literal, $expression:expr) => {{
        $expression
    }};
}

pub async fn render_thread(
    message_channel: mpsc::Receiver<Image>,
    status: Arc<Mutex<Status>>,
    mut gpu_data: GPUData,
) -> Result<()> {
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

    while let Ok(mut image) = message_channel.recv() {
        // clear the message channel queue and only process the last message
        while let Ok(image_tmp) = message_channel.try_recv() {
            image = image_tmp;
        }
        debug!("Received image: {:?}", image);

        let last_image = {
            status
                .lock()
                .map_err(|e| eyre!("Failed to lock image: {:?}", e))?
                .rendered_image
                .clone()
        };
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

        if resize {
            gpu_data.resize(&image.viewport)?;
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
                run_compute_step(&probed_data, &image, &gpu_data, status.clone())?
            );
        }

        if recolor {
            time!("Running image render", run_render_step(&image, &gpu_data));
        }

        let mut status = status
            .lock()
            .map_err(|e| eyre!("Failed to lock status: {:?}", e))?;
        status.message = "Finished Rendering".to_string();
        status.progress = None;
        status.rendered_image = Some(image);
    }

    Ok(())
}

fn run_compute_step(
    probed_data: &(Vec<[f32; 2]>, Vec<[f32; 2]>),
    image: &Image,
    gpu_data: &GPUData,
    status: Arc<Mutex<Status>>,
) -> Result<()> {
    let GPUData {
        device,
        queue,
        bind_groups,
        compute_pipeline,
        buffers,
        ..
    } = gpu_data;
    let texture_size = GPUData::get_texture_size(&image.viewport);
    let probe_len = probed_data.0.len();
    debug_assert!(probe_len == probed_data.1.len());
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
            width: image.viewport.width as u32,
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
        let mut status = status
            .lock()
            .map_err(|e| eyre!("Failed to lock status: {:?}", e))?;
        status.message = format!(
            "Rendering {} of {}",
            i * MAX_GPU_GROUP_ITER + parameters.probe_len as usize,
            probe_len
        );
        status.progress = Some(
            (i * MAX_GPU_GROUP_ITER + parameters.probe_len as usize) as f64 / probe_len as f64,
        );
    }

    Ok(())
}

fn run_render_step(image: &Image, gpu_data: &GPUData) {
    let GPUData {
        device,
        queue,
        bind_groups,
        buffers,
        render_pipeline_layout,
        ..
    } = gpu_data;
    let texture_size = GPUData::get_texture_size(&image.viewport);
    let color_params = image.to_render_params();
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

    // create a render command queue

    // Create encoder for CPU - GPU communication
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

    // Begin render dispatch
    {
        let mut cpass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor { label: None });
        cpass.set_bind_group(0, &bind_groups.render_buffers, &[]);
        cpass.set_bind_group(1, &bind_groups.render_texture, &[]);
        cpass.set_bind_group(2, &bind_groups.render_parameters, &[]);
        cpass.set_pipeline(&render_pipeline);
        cpass.dispatch_workgroups(
            (texture_size.width as f64 / 16.0).ceil() as u32,
            (texture_size.height as f64 / 16.0).ceil() as u32,
            1,
        );
    }

    // submit the render command queue
    queue.submit(Some(encoder.finish()));
}
