use std::sync::Arc;

use color_eyre::{eyre::eyre, Result};

use tracing::info;
use wgpu::{AdapterInfo, Device, Dx12Compiler, Queue, Surface, SurfaceConfiguration};
use winit::window::Window;

/// A struct containing all of the GPU handles for the application
pub struct GPUHandles {
    // general GPU Handles
    /// An Arc to the device
    pub device: Arc<Device>,
    /// An Arc to the queue
    pub queue: Arc<Queue>,
    // Windowing handles
    /// The surface for the window
    pub surface: Surface,
    /// The surface configuration for the window
    pub surface_config: SurfaceConfiguration,
}

impl GPUHandles {
    /// Initializes the GPU handles for the application. This function will
    /// find the best adapter for the system and create a device and queue as
    /// well as a surface for the window.
    ///
    /// # Errors
    ///
    /// This function will return an error if it is unable to create any of the
    /// GPU handles, usually because there is no compatible GPU.
    pub async fn init(window: Window) -> Result<(Self, &'static mut Window)> {
        let size = window.inner_size();

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            // this should work on all backends, so long as there is an adapter
            // with a compatible surface
            backends: wgpu::Backends::all(),
            dx12_shader_compiler: Dx12Compiler::default(),
        });

        let window: &'static mut Window = Box::leak(Box::new(window));
        // Safety: The window is now a static reference, so it will not be
        // dropped until the end of the program
        let surface = unsafe { instance.create_surface(window) }?;

        // we prefer high-performance adapters because this
        // program does not focus on power efficiency
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or(eyre!("Failed to find an appropriate adapter"))?;

        // we want to use high precision floats if possible (currently unused)
        let use_high_precision_float = adapter.features().contains(wgpu::Features::SHADER_FLOAT64);

        let (device, queue) = if let Ok(r) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: None,
                    features: if use_high_precision_float {
                        wgpu::Features::SHADER_FLOAT64
                    } else {
                        wgpu::Features::empty()
                    },
                    limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
        {
            r
        } else {
            adapter
                .request_device(
                    &wgpu::DeviceDescriptor {
                        label: None,
                        features: wgpu::Features::empty(),
                        limits: wgpu::Limits::default(),
                    },
                    None,
                )
                .await?
        };

        let surface_config = surface
            .get_default_config(&adapter, size.width, size.height)
            .ok_or(eyre!(
                "Failed to get default surface config, is this surface supported?"
            ))?;
        surface.configure(&device, &surface_config);

        let AdapterInfo { name, backend, .. } = adapter.get_info();
        info!("Running on {name} with the {backend:?} backend");

        Ok((
            Self {
                device: Arc::new(device),
                queue: Arc::new(queue),
                surface,
                surface_config,
            },
            window,
        ))
    }

    /// Resizes the surface to the new size
    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        // recreate the surface with the new size
        self.surface_config.width = new_size.width;
        self.surface_config.height = new_size.height;
        self.surface.configure(&self.device, &self.surface_config);
    }
}
