/*!
# Shader Types

This module contains additional types that are passed to the GPU and
conversion logic from internal types.
 */
use crate::shared::coloring::main::{ColorParams, Light, Overlays as OverlayParams, RenderParams};
use crate::shared::wgsl_primitives::{Vec4, Vec4f};
use crate::types::{Coloring, ImgSpec, Outline, Overlays};

impl From<&Coloring> for ColorParams {
    fn from(value: &Coloring) -> Self {
        let (gradient_kind, gradient_vec) = value.gradient.decompose();
        fn bytecast_map<ArrT, MapT, OutT, const N: usize>(
            v: &[ArrT],
            f: impl Fn(&ArrT) -> MapT,
        ) -> [OutT; N]
        where
            OutT: bytemuck::AnyBitPattern,
            MapT: bytemuck::NoUninit + Default,
        {
            let mut mapped = v.iter().map(f).collect::<Vec<MapT>>();
            mapped.extend_from_slice(&vec![
                MapT::default();
                (size_of::<OutT>() * N / size_of::<MapT>())
                    .checked_sub(mapped.len())
                    .unwrap()
            ]);
            bytemuck::cast_slice::<MapT, OutT>(&mapped)
                .try_into()
                .unwrap()
        }
        ColorParams {
            saturation: value.saturation,
            brightness: value.brightness,
            color_frequency: value.color_frequency,
            color_offset: value.color_offset,
            gradient_kind,
            gradient_size: gradient_vec.len() as u32 / 4,
            lighting_kind: value.lighting_kind as u32,
            color_layer_types: bytecast_map(&value.color_layers, |x| x.kind as u8).into(),
            light_layer_types: bytecast_map(&value.light_layers, |x| x.kind as u8).into(),
            color_strengths: bytecast_map(&value.color_layers, |x| x.strength),
            color_params: bytecast_map(&value.color_layers, |x| x.param),
            light_strengths: bytecast_map(&value.light_layers, |x| x.strength),
            light_params: bytecast_map(&value.light_layers, |x| x.param),
            lights: bytecast_map(&value.lights, Light::clone),
            overlays: (&value.overlays).into(),
            padding: 0,
        }
    }
}

impl From<&Overlays> for OverlayParams {
    fn from(value: &Overlays) -> Self {
        Self {
            iteration_outline: pack_outline(&value.iteration_outline),
            set_outline: pack_outline(&value.set_outline),
        }
    }
}

fn pack_outline(value: &Option<Outline>) -> Vec4f {
    if let Some(inner) = value {
        let mut packed = inner.color.to_rgba_unmultiplied();
        packed[3] *= 0.999;
        packed[3] += inner.parameter as f32;
        packed.into()
    } else {
        Vec4::splat(0.0)
    }
}

impl From<&ImgSpec> for RenderParams {
    fn from(image: &ImgSpec) -> Self {
        RenderParams {
            width: (image.width as f64) as u32,
            height: (image.height as f64) as u32,
        }
    }
}
