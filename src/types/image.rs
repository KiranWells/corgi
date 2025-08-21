use eframe::{egui::Vec2, wgpu::Extent3d};
use nanoserde::{DeJson, SerJson};
use rug::{
    Float,
    ops::{CompleteRound, PowAssign},
};

use super::get_precision;

#[derive(DeJson, SerJson)]
struct FloatParser {
    value: String,
    precision: u32,
}

impl From<&FloatParser> for Float {
    fn from(value: &FloatParser) -> Self {
        Float::parse(value.value.clone())
            .map(|val| val.complete(value.precision))
            .unwrap_or(Float::new(53))
    }
}

impl From<&Float> for FloatParser {
    fn from(val: &Float) -> Self {
        FloatParser {
            value: val.to_string_radix(10, None),
            precision: val.prec(),
        }
    }
}

/// A representation of the current viewed portion of the fractal
#[derive(Debug, Clone, PartialEq, DeJson, SerJson)]
pub struct Viewport {
    pub width: usize,
    pub height: usize,
    pub zoom: f64,
    #[nserde(proxy = "FloatParser")]
    pub x: Float,
    #[nserde(proxy = "FloatParser")]
    pub y: Float,
}

#[derive(Debug, Clone, PartialEq, DeJson, SerJson)]
pub struct ProbeLocation {
    #[nserde(proxy = "FloatParser")]
    pub x: Float,
    #[nserde(proxy = "FloatParser")]
    pub y: Float,
}

/// A representation of the current image being rendered, including
/// the viewport, coloring, and other parameters
#[derive(Debug, Clone, PartialEq, DeJson, SerJson)]
pub struct Image {
    pub viewport: Viewport,
    pub max_iter: u64,
    pub probe_location: ProbeLocation,
    pub external_coloring: Coloring2,
    pub internal_coloring: Coloring2,
    pub misc: f32,
    pub debug_shutter: f32,
}

/// The coloring parameters for the image
#[derive(Debug, Clone, Copy, PartialEq, DeJson, SerJson)]
pub struct Coloring {
    pub saturation: f32,
    pub color_frequency: f32,
    pub color_offset: f32,
    pub glow_spread: f32,
    pub glow_intensity: f32,
    pub brightness: f32,
    pub internal_brightness: f32,
}

#[derive(Clone, Debug, PartialEq, DeJson, SerJson)]
pub struct Coloring2 {
    pub saturation: f32,
    pub brightness: f32,
    pub color_frequency: f32,
    pub color_offset: f32,
    pub gradient: Gradient,
    pub color_layers: [Layer; 8],
    pub lighting_kind: LightingKind,
    pub light_layers: [Layer; 8],
    pub lights: [Light; 3],
    pub overlays: Overlays,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, DeJson, SerJson)]
pub enum LightingKind {
    Flat,
    Gradient,
    Shaded,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, DeJson, SerJson, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Light {
    pub color: [f32; 3],
    pub strength: f32,
    pub direction: [f32; 3],
    padding: f32,
}

impl Light {
    fn new(color: [f32; 3], strength: f32, direction: [f32; 3]) -> Self {
        let direction_length = (direction[0] * direction[0]
            + direction[1] * direction[1]
            + direction[2] * direction[2])
            .sqrt();
        let direction = [
            direction[0] / direction_length,
            direction[1] / direction_length,
            direction[2] / direction_length,
        ];
        Self {
            color,
            strength,
            direction,
            padding: 0.0,
        }
    }

    pub fn normalize(&mut self) {
        let direction = self.direction;
        let direction_length = (direction[0] * direction[0]
            + direction[1] * direction[1]
            + direction[2] * direction[2])
            .sqrt();
        let direction = [
            direction[0] / direction_length,
            direction[1] / direction_length,
            direction[2] / direction_length,
        ];
        self.direction = direction;
    }
}

impl Default for Light {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            strength: 1.0,
            direction: [0.0, 0.0, 1.0],
            padding: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, DeJson, SerJson)]
pub struct Layer {
    pub kind: LayerKind,
    pub strength: f32,
    pub param: f32,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            kind: LayerKind::None,
            strength: 0.5,
            param: 0.0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, DeJson, SerJson, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Overlays {
    pub iteration_outline_color: [f32; 4],
    pub set_outline_color: [f32; 4],
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, DeJson, SerJson)]
pub enum LayerKind {
    None = 0,
    Step,
    SmoothStep,
    Distance,
    OrbitTrap,
    // Normal,
    Stripe,
    // Step(StepLayer),
    // SmoothStep(SmoothStepLayer),
    // Distance(DistanceLayer),
    // OrbitTrap(OrbitTrapLayer),
    // Normal(NormalLayer),
    // Stripe(StripeLayer),
}

// pub struct StepLayer {}
// pub struct SmoothStepLayer {}
// pub struct DistanceLayer {}
// pub struct OrbitTrapLayer {}
// pub struct NormalLayer {}
// pub struct StripeLayer {}

#[derive(Clone, Debug, PartialEq, DeJson, SerJson)]
pub enum Gradient {
    Flat([f32; 3]),
    Procedural([[f32; 3]; 4]),
    Manual([[f32; 4]; 3]),
    Hsv(f32, f32),
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ColorParams {
    pub saturation: f32,
    pub brightness: f32,
    pub color_frequency: f32,
    pub color_offset: f32,
    pub gradient_kind: u32,
    pub lighting_kind: u32,
    padding: [u32; 2],
    pub gradient: [f32; 12],
    pub color_layer_types: [u8; 8],
    pub light_layer_types: [u8; 8],
    pub color_strengths: [f32; 8],
    pub color_params: [f32; 8],
    pub light_strengths: [f32; 8],
    pub light_params: [f32; 8],
    pub lights: [Light; 3],
    pub overlays: Overlays,
}

impl From<&Coloring2> for ColorParams {
    fn from(value: &Coloring2) -> Self {
        let (gradient_kind, gradient_vec) = match value.gradient {
            Gradient::Flat(data) => {
                let mut new_data = data.to_vec();
                new_data.extend_from_slice(&[0.0; 9]);
                (0, new_data)
            }
            Gradient::Procedural(data) => (1, data.concat()),
            Gradient::Manual(data) => (2, data.concat()),
            Gradient::Hsv(saturation, value) => {
                let mut new_data = vec![saturation, value];
                new_data.extend_from_slice(&[0.0; 10]);
                (3, new_data)
            }
        };
        let mut gradient = [0.0; 12];
        gradient.copy_from_slice(&gradient_vec);
        ColorParams {
            saturation: value.saturation,
            brightness: value.brightness,
            color_frequency: value.color_frequency,
            color_offset: value.color_offset,
            gradient_kind,
            lighting_kind: value.lighting_kind as u32,
            gradient,
            color_layer_types: value.color_layers.map(|x| x.kind as u8),
            light_layer_types: value.light_layers.map(|x| x.kind as u8),
            color_strengths: value.color_layers.map(|x| x.strength),
            color_params: value.color_layers.map(|x| x.param),
            light_strengths: value.light_layers.map(|x| x.strength),
            light_params: value.light_layers.map(|x| x.param),
            lights: value.lights,
            overlays: value.overlays,
            padding: [0; 2],
        }
    }
}

/// The parameters for the compute shader. This is sent as a uniform
/// to the compute shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ComputeParams {
    pub width: u32,
    pub height: u32,
    pub max_iter: u32,
    pub probe_len: u32,
    pub iter_offset: u32,
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

/// The parameters for the render shader. This is sent as a uniform
/// to the render shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct RenderParams {
    pub image_width: u32,
    pub max_step: u32,
    pub zoom: f32,
    pub misc: f32,
    pub debug_shutter: f32,
}

/// The parameters for the preview shader. This is sent as a uniform
/// to the preview shader.
#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Transform {
    pub angle: f32,
    pub scale: f32,
    pub offset: [f32; 2],
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            angle: 0.0,
            scale: 1.0,
            offset: [0.0, 0.0],
        }
    }
}

impl Default for Coloring {
    fn default() -> Self {
        Self {
            color_frequency: 1.0,
            color_offset: 0.0,
            glow_spread: 1.0,
            glow_intensity: 0.5,
            brightness: 2.0,
            internal_brightness: 0.5,
            saturation: 1.0,
        }
    }
}

impl Default for Coloring2 {
    fn default() -> Self {
        Self {
            saturation: 1.0,
            brightness: 1.0,
            color_frequency: 1.0,
            color_offset: 0.0,
            gradient: Gradient::Procedural([[0.5; 3], [0.5; 3], [1.0; 3], [0.0, 0.1, 0.2]]),
            color_layers: [
                Layer {
                    kind: LayerKind::SmoothStep,
                    strength: 1.0,
                    param: 0.0,
                },
                Layer {
                    kind: LayerKind::Stripe,
                    strength: 1.0,
                    param: 0.5,
                },
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
            ],
            lighting_kind: LightingKind::Gradient,
            light_layers: [
                Layer {
                    kind: LayerKind::Distance,
                    strength: 1.0,
                    param: 2.0,
                },
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
            ],
            lights: [
                Light::new([1.0, 1.0, 1.0], 1.0, [0.0, 0.5, 0.8]),
                Light::new([0.5, 0.6, 1.0], 1.0, [0.8, 0.0, 0.6]),
                Light::new([1.0, 0.8, 0.4], 1.0, [0.0, 0.8, 0.5]),
            ],
            overlays: Overlays {
                iteration_outline_color: [0.0; 4],
                set_outline_color: [0.0; 4],
            },
        }
    }
}

impl Coloring2 {
    pub fn internal_default() -> Self {
        Self {
            saturation: 1.0,
            brightness: 1.0,
            color_frequency: 1.0,
            color_offset: 0.0,
            gradient: Gradient::Flat([1.0; 3]),
            color_layers: [Layer::default(); 8],
            lighting_kind: LightingKind::Shaded,
            light_layers: [
                Layer {
                    kind: LayerKind::Stripe,
                    strength: 1.0,
                    param: 0.5,
                },
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
            ],
            lights: [
                Light::new([1.0, 1.0, 1.0], 1.5, [0.0, 0.6, 0.4]),
                Light::new([0.5, 0.6, 1.0], 1.5, [0.0, 0.3, 1.0]),
                Light::new([1.0, 0.8, 0.4], 1.0, [0.0, 0.3, 1.0]),
            ],
            overlays: Overlays {
                iteration_outline_color: [0.0; 4],
                set_outline_color: [0.0; 4],
            },
        }
    }
}

impl Viewport {
    /// Derives the transforms from another viewport to this one
    pub fn transforms_from(&self, other: &Self) -> Transform {
        let scale = f32::powf(2.0, -(self.zoom - other.zoom) as f32);
        let mut this_scale = Float::with_val(get_precision(self.zoom), 2.0);
        this_scale.pow_assign(-self.zoom);
        let aspect_scale = self.aspect_scale();
        let offset: [Float; 2] = [
            (self.x.clone() - other.x.clone()) / this_scale.clone() / aspect_scale.x,
            (self.y.clone() - other.y.clone()) / this_scale / aspect_scale.y,
        ];
        Transform {
            angle: 0.0,
            scale,
            offset: [offset[0].to_f32(), offset[1].to_f32()],
        }
    }

    /// The aspect ratio of the viewport
    pub fn aspect_ratio(&self) -> f64 {
        self.width as f64 / self.height as f64
    }

    pub fn aspect_scale(&self) -> Vec2 {
        let aspect = self.aspect_ratio() as f32;
        if aspect < 1.0 {
            Vec2::new(aspect, 1.0)
        } else {
            Vec2::new(1.0, 1.0 / aspect)
        }
    }

    /// Gets the fractal coordinates of a pixel from viewport coordinates
    pub fn get_real_coords(&self, x: f64, y: f64) -> (Float, Float) {
        let precision = get_precision(self.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-self.zoom);
        let aspect_scale = self.aspect_scale();

        let r = ((x / self.width as f64) * 2.0 - 1.0) * scale.clone() * aspect_scale.x
            + Float::with_val(precision, &self.x);
        let i = ((y / self.height as f64) * 2.0 - 1.0) * scale.clone() * aspect_scale.y
            + Float::with_val(precision, &self.y);
        (r, i)
    }

    /// Returns the offset in pixels from the center of this viewport to
    /// the given location in fractal coordinates
    pub fn coords_to_px_offset(&self, r: &Float, i: &Float) -> (f64, f64) {
        let precision = get_precision(self.zoom);
        let mut scale = Float::with_val(precision, 2.0);
        scale.pow_assign(-self.zoom);
        let aspect_scale = self.aspect_scale();

        let x = ((r.clone() - self.x.clone()) / scale.clone()).to_f64() / aspect_scale.x as f64;
        let y = ((i.clone() - self.y.clone()) / scale).to_f64() / aspect_scale.y as f64;
        (x * 0.5 * self.width as f64, y * 0.5 * self.height as f64)
    }

    pub fn algorithm(&self) -> Algorithm {
        match self.zoom {
            x if x < 13.0 => Algorithm::Directf32,
            _ => Algorithm::Perturbedf32,
        }
    }
}

impl From<&Viewport> for Extent3d {
    fn from(viewport: &Viewport) -> Self {
        Self {
            width: viewport.width as u32,
            height: viewport.height as u32,
            depth_or_array_layers: 1,
        }
    }
}

impl From<&Image> for RenderParams {
    fn from(image: &Image) -> Self {
        RenderParams {
            image_width: image.viewport.width as u32,
            max_step: image.max_iter as u32,
            zoom: image.viewport.zoom as f32,
            misc: image.misc,
            debug_shutter: image.debug_shutter,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Directf32,
    Perturbedf32,
}

#[derive(Clone, Debug)]
pub struct ImageDiff {
    pub reprobe: bool,
    pub recompute: bool,
    pub recolor: bool,
    pub resize: bool,
}

impl ImageDiff {
    pub fn full() -> Self {
        ImageDiff {
            resize: true,
            reprobe: true,
            recompute: true,
            recolor: true,
        }
    }
}

impl Image {
    pub fn algorithm(&self) -> Algorithm {
        self.viewport.algorithm()
    }

    pub fn comp(&self, other: &Self) -> ImageDiff {
        // if the viewport has changed, resize the GPU data
        let resize = self.viewport.width != other.viewport.width
            || self.viewport.height != other.viewport.height;
        // if the max iteration or probe location has changed, re-run the probe
        let reprobe = self.max_iter != other.max_iter
            || self.probe_location.x != other.probe_location.x
            || self.probe_location.y != other.probe_location.y;
        // if the probe location has changed or the image viewport has changed, re-generate the delta grid
        // if the image generation parameters have changed, re-run the compute shader
        let recompute =
            self.max_iter != other.max_iter || self.viewport != other.viewport || reprobe;
        // if the image coloring parameters have changed, re-run the image render
        let recolor = self.external_coloring != other.external_coloring
            || self.internal_coloring != other.internal_coloring
            || recompute
            || self.misc != other.misc
            || self.debug_shutter != other.debug_shutter;
        ImageDiff {
            reprobe,
            recompute,
            recolor,
            resize,
        }
    }

    pub fn estimate_calc_time(&self, previous: Option<&Self>) -> std::time::Duration {
        let diff = previous
            .map(|img| self.comp(img))
            .unwrap_or(ImageDiff::full());
        let mut calc_time_ms = 0;
        if diff.reprobe {
            calc_time_ms += self.max_iter / 1000;
        }
        if diff.recompute {
            calc_time_ms += self.max_iter / 1000 + (self.viewport.zoom / 2.0) as u64;
        }
        if diff.resize {
            calc_time_ms += 1;
        }
        std::time::Duration::from_millis(calc_time_ms)
    }
}

impl Default for Image {
    fn default() -> Self {
        Self {
            viewport: Viewport {
                width: 512,
                height: 512,
                zoom: -2.0,
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
            probe_location: ProbeLocation {
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
            max_iter: 10000,
            external_coloring: Coloring2::default(),
            internal_coloring: Coloring2::internal_default(),
            misc: 1.0,
            debug_shutter: 0.0,
        }
    }
}
