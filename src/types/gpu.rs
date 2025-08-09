
use color_eyre::{eyre::eyre, Result};

use eframe::{
    egui_wgpu::WgpuSetupExisting,
    wgpu::{self, InstanceFlags},
};
use tracing::info;
use wgpu::AdapterInfo;

pub async fn init() -> Result<WgpuSetupExisting> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        // this should work on all backends, so long as there is an adapter
        // with a compatible surface
        backends: wgpu::Backends::all(),
        flags: InstanceFlags::empty(),
        backend_options: wgpu::BackendOptions::default(),
    });

    // we prefer high-performance adapters because this
    // program does not focus on power efficiency
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        })
        .await
        .or(Err(eyre!("Failed to find an appropriate adapter")))?;

    // we want to use high precision floats if possible (currently unused)
    let use_high_precision_float = adapter.features().contains(wgpu::Features::SHADER_F64);

    let (device, queue) = {
        if let Ok(r) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: if use_high_precision_float {
                    wgpu::Features::SHADER_F64
                } else {
                    wgpu::Features::empty()
                },
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
        {
            r
        } else {
            adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: None,
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::Performance,
                    trace: wgpu::Trace::Off,
                })
                .await?
        }
    };

    let AdapterInfo { name, backend, .. } = adapter.get_info();
    info!("Running on {name} with the {backend:?} backend");

    Ok(WgpuSetupExisting {
        instance,
        adapter,
        device,
        queue,
    })
}
