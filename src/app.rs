use std::sync::Arc;

use color_eyre::{Report, Result};
use egui_wgpu::renderer::ScreenDescriptor;
use tokio::{
    runtime::Handle,
    sync::{mpsc, Mutex},
    task::block_in_place,
};
use tracing::{error, warn};
use winit::{event::*, event_loop::EventLoop};
use winit::{event_loop::ControlFlow, window::Window};

use crate::{
    types::{Debouncer, EguiData, GPUData, Image, PreviewRenderResources, RenderErr, Status},
    ui::CorgiUI,
};

pub struct CorgiState {
    gpu_data: GPUData,
    egui: EguiData,
    ui_state: CorgiUI,
    last_rendered: Image,
    sender: mpsc::Sender<Image>,
    size: winit::dpi::PhysicalSize<u32>,
    debouncer: Debouncer,
    window: Window,
}

impl CorgiState {
    pub async fn init(
        window: Window,
        event_loop: &EventLoop<()>,
        sender: mpsc::Sender<Image>,
        status: Arc<Mutex<Status>>,
        preview_resources: PreviewRenderResources,
        gpu_data: GPUData,
    ) -> Result<Self> {
        let mut renderer =
            egui_wgpu::Renderer::new(&gpu_data.device, gpu_data.surface_config.format, None, 1);
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(window.scale_factor() as f32);
        let mut state = egui_winit::State::new(event_loop);
        state.set_pixels_per_point(window.scale_factor() as f32);

        renderer.paint_callback_resources.insert(preview_resources);

        let ui_state = CorgiUI::new(status);

        sender.send(ui_state.image().clone()).await?;

        Ok(Self {
            gpu_data,
            egui: EguiData {
                state,
                ctx,
                renderer,
                needs_rerender: true,
            },
            last_rendered: ui_state.image().clone(),
            ui_state,
            sender,
            size: window.inner_size(),
            debouncer: Debouncer::new(std::time::Duration::from_millis(100)),
            window,
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        self.gpu_data.resize(new_size);
    }

    // processes state and returns true if the event has been processed
    pub fn input(&mut self, event: &WindowEvent) -> bool {
        let er = self.egui.state.on_event(&self.egui.ctx, event);
        self.egui.needs_rerender = er.repaint;
        er.consumed
    }

    pub fn update(&mut self) {}

    pub async fn render(&mut self) -> Result<(), RenderErr> {
        let window = &self.window;

        let image = self.ui_state.image().clone();
        //  sanity check on image size
        if !(image.viewport.width < 10
            || image.viewport.height < 10
            || image.viewport.width * image.viewport.height > 20_000_000
            || self.ui_state.mouse_down)
        {
            if &self.last_rendered != self.ui_state.image() {
                if image.viewport.zoom == self.last_rendered.viewport.zoom {
                    self.sender
                        .send(image.clone())
                        .await
                        .map_err(Into::<Report>::into)?;
                    self.debouncer.reset();
                } else {
                    self.debouncer.trigger();
                }
                self.last_rendered = image;
            } else if self.debouncer.poll() {
                self.sender
                    .send(self.ui_state.image().clone())
                    .await
                    .map_err(Into::<Report>::into)?;
            }
        }

        let output = self.gpu_data.surface.get_current_texture()?;

        let output_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // draw the UI frame
        let input = self.egui.state.take_egui_input(window);

        self.egui.ctx.begin_frame(input);
        self.ui_state.generate_ui(&self.egui.ctx).await?;

        let egui_output = self.egui.ctx.end_frame();
        let paint_jobs = self.egui.ctx.tessellate(egui_output.shapes);

        let device = &self.gpu_data.device;
        let queue = &self.gpu_data.queue;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("encoder"),
        });

        // Upload all resources for the GPU.
        let screen_descriptor = ScreenDescriptor {
            size_in_pixels: [self.size.width, self.size.height],
            pixels_per_point: window.scale_factor() as f32,
        };

        let egui_r_pass = &mut self.egui.renderer;

        for (id, image_delta) in &egui_output.textures_delta.set {
            egui_r_pass.update_texture(device, queue, *id, image_delta);
        }
        for id in &egui_output.textures_delta.free {
            egui_r_pass.free_texture(id);
        }
        egui_r_pass.update_buffers(device, queue, &mut encoder, &paint_jobs, &screen_descriptor);

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &output_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: true,
                    },
                })],
                depth_stencil_attachment: None,
            });
            egui_r_pass.render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }

        // Submit the commands.
        queue.submit([encoder.finish()]);

        // Redraw egui
        output.present();

        Ok(())
    }

    pub fn start(mut self, event_loop: EventLoop<()>) -> ! {
        event_loop.run(move |event, _, control_flow| {
            match event {
                Event::WindowEvent {
                    ref event,
                    window_id,
                } if window_id == self.window.id() => {
                    self.window.request_redraw();
                    if !self.input(event) {
                        match event {
                            WindowEvent::CloseRequested
                            | WindowEvent::KeyboardInput {
                                input:
                                    KeyboardInput {
                                        state: ElementState::Pressed,
                                        virtual_keycode: Some(VirtualKeyCode::Escape),
                                        ..
                                    },
                                ..
                            } => *control_flow = ControlFlow::Exit,
                            WindowEvent::Resized(physical_size) => {
                                if physical_size.width == 0 || physical_size.height == 0 {
                                    *control_flow = ControlFlow::Wait;
                                    return;
                                }
                                self.resize(*physical_size);
                            }
                            WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                                // new_inner_size is &&mut so we have to dereference it twice
                                self.resize(**new_inner_size);
                            }
                            _ => {}
                        }
                    }
                }
                Event::RedrawRequested(window_id) if window_id == self.window.id() => {
                    self.update();
                    match block_in_place(|| Handle::current().block_on(self.render())) {
                        Ok(_) => {}
                        // Reconfigure the surface if lost
                        Err(RenderErr::Resize) => self.resize(self.size),
                        // The system is out of memory, we should probably quit
                        Err(RenderErr::Quit(e)) => {
                            // print error and exit
                            error!("Error encountered while rendering: {:?}", e);
                            *control_flow = ControlFlow::Exit
                        }
                        // All other errors (Outdated, Timeout) should be resolved by the next frame
                        Err(e) => warn!("Error encountered redraw: {:?}", e),
                    }
                }
                Event::MainEventsCleared => {
                    // RedrawRequested will only trigger once, unless we manually
                    // request it.
                    self.window.request_redraw();
                    // the render is not complete, update the screen as it changes
                }
                _ => {}
            }
        })
    }
}
