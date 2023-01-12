pub mod image_gen;
pub mod types;
mod ui;

use std::sync::{mpsc::Sender, Arc, Mutex};

use egui_wgpu::renderer::ScreenDescriptor;
use ui::CorgiUI;
use winit::window::Window;
use winit::{
    event::*,
    event_loop::{ControlFlow, EventLoop},
};

use types::{GPUData, Image, PreviewRenderResources, Status};

struct Debouncer {
    wait_time: std::time::Duration,
    last_triggered: Option<std::time::Instant>,
    // cb: Option<Box<dyn FnOnce()>>,
}

impl Debouncer {
    fn new(wait: std::time::Duration) -> Self {
        Self {
            wait_time: wait,
            last_triggered: None,
            // cb: None,
        }
    }

    fn trigger(&mut self) {
        // println!("debouncer triggered");
        self.last_triggered = Some(std::time::Instant::now());
        // self.cb = Some(Box::new(cb));
    }

    fn poll(&mut self) -> bool {
        if let Some(v) = self.last_triggered {
            let now = std::time::Instant::now();
            if now - v >= self.wait_time {
                self.last_triggered = None;
                return true;
            }
        }
        false
    }

    fn reset(&mut self) {
        self.last_triggered = None;
    }
}

struct EguiData {
    pub state: egui_winit::State,
    pub ctx: egui::Context,
    pub renderer: egui_wgpu::Renderer,
    pub needs_rerender: bool,
}

struct CorgiState {
    gpu_data: GPUData,
    egui: EguiData,
    ui_state: CorgiUI,
    last_rendered: Image,
    sender: Sender<Image>,
    size: winit::dpi::PhysicalSize<u32>,
    debouncer: Debouncer,
    window: Window,
}

impl CorgiState {
    async fn init(
        window: Window,
        event_loop: &EventLoop<()>,
        sender: Sender<Image>,
        status: Arc<Mutex<Status>>,
        preview_resources: PreviewRenderResources,
        gpu_data: GPUData,
    ) -> Self {
        let mut renderer =
            egui_wgpu::Renderer::new(&gpu_data.device, gpu_data.surface_config.format, None, 1);
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(window.scale_factor() as f32);
        let mut state = egui_winit::State::new(event_loop);
        state.set_pixels_per_point(window.scale_factor() as f32);

        // Because the graphics pipeline must have the same lifetime as the egui render pass,
        // instead of storing the pipeline in our `Custom3D` struct, we insert it into the
        // `paint_callback_resources` type map, which is stored alongside the render pass.
        renderer.paint_callback_resources.insert(preview_resources);

        let ui_state = CorgiUI::new(status);

        sender.send(ui_state.image().clone()).unwrap();

        Self {
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
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.size = new_size;
        // println!("Resizing to {:?}", new_size);
        self.gpu_data.resize(new_size);
    }

    // processes state and returns true if the event has been processed
    fn input(&mut self, event: &WindowEvent) -> bool {
        let er = self.egui.state.on_event(&self.egui.ctx, event);
        self.egui.needs_rerender = er.repaint;
        er.consumed
    }

    fn update(&mut self) {}

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
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
                    self.sender.send(image.clone()).unwrap();
                    self.debouncer.reset();
                } else {
                    self.debouncer.trigger();
                }
                self.last_rendered = image;
            } else if self.debouncer.poll() {
                self.sender.send(self.ui_state.image().clone()).unwrap();
            }
        }

        let output = self.gpu_data.surface.get_current_texture()?;

        let output_view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // draw the UI frame
        let input = self.egui.state.take_egui_input(window);

        self.egui.ctx.begin_frame(input);
        self.ui_state.generate_ui(&self.egui.ctx);

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

        let egui_rpass = &mut self.egui.renderer;

        for (id, image_delta) in &egui_output.textures_delta.set {
            egui_rpass.update_texture(device, queue, *id, image_delta);
        }
        for id in &egui_output.textures_delta.free {
            egui_rpass.free_texture(id);
        }
        egui_rpass.update_buffers(device, queue, &mut encoder, &paint_jobs, &screen_descriptor);

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
            egui_rpass.render(&mut render_pass, &paint_jobs, &screen_descriptor);
        }

        // Submit the commands.
        queue.submit([encoder.finish()]);

        // Redraw egui
        output.present();

        Ok(())
    }
}

pub async fn run(
    message_sender: Sender<Image>,
    status: Arc<Mutex<Status>>,
    window: Window,
    event_loop: EventLoop<()>,
    preview_resources: PreviewRenderResources,
    gpu_data: GPUData,
) {
    env_logger::init();
    let mut state = CorgiState::init(
        window,
        &event_loop,
        message_sender,
        status,
        preview_resources,
        gpu_data,
    )
    .await;

    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::WindowEvent {
                ref event,
                window_id,
            } if window_id == state.window.id() => {
                state.window.request_redraw();
                if !state.input(event) {
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
                                println!("Window minimized");
                                return;
                            }
                            state.resize(*physical_size);
                        }
                        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                            // new_inner_size is &&mut so we have to dereference it twice
                            state.resize(**new_inner_size);
                        }
                        _ => {}
                    }
                }
            }
            Event::RedrawRequested(window_id) if window_id == state.window.id() => {
                state.update();
                match state.render() {
                    Ok(_) => {}
                    // Reconfigure the surface if lost
                    Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                    // The system is out of memory, we should probably quit
                    Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                    // All other errors (Outdated, Timeout) should be resolved by the next frame
                    Err(e) => eprintln!("{:?}", e),
                }
            }
            Event::MainEventsCleared => {
                // RedrawRequested will only trigger once, unless we manually
                // request it.
                state.window.request_redraw();
                // the render is not complete, update the screen as it changes
            }
            _ => {}
        }
    });
}
