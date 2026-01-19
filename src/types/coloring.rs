use std::sync::atomic::AtomicU64;

use serde::{Deserialize, Serialize};

/// The coloring parameters for the image. These are interpreted
/// slightly differently for internal and external coloring, as
/// some coloring algorithms are incompatible between the two.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Coloring {
    pub saturation: f32,
    pub brightness: f32,
    pub color_frequency: f32,
    pub color_offset: f32,
    pub gradient: Gradient,
    pub color_layers: Vec<Layer>,
    pub lighting_kind: LightingKind,
    pub light_layers: Vec<Layer>,
    pub lights: Vec<Light>,
    pub overlays: Overlays,
}

#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub enum LightingKind {
    Flat,
    Gradient,
    RepeatingGradient,
    Shaded,
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, PartialEq, Deserialize, Serialize, bytemuck::Pod, bytemuck::Zeroable,
)]
pub struct Light {
    pub color: [f32; 3],
    pub strength: f32,
    pub direction: [f32; 3],
    padding: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct Layer {
    #[serde(skip)]
    pub id: u64,
    pub kind: LayerKind,
    pub strength: f32,
    pub param: f32,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            id: 0,
            kind: LayerKind::SmoothStep,
            strength: 1.0,
            param: 0.0,
        }
    }
}

static LAYER_ID: AtomicU64 = AtomicU64::new(0);
pub fn next_layer_id() -> u64 {
    LAYER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub struct Overlays {
    pub iteration_outline: Option<Outline>,
    pub set_outline: Option<Outline>,
}

#[repr(C)]
#[derive(
    Clone, Copy, Debug, PartialEq, Deserialize, Serialize, bytemuck::Pod, bytemuck::Zeroable,
)]
pub struct Outline {
    pub color: ecolor::Rgba,
    pub parameter: u32,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
pub enum LayerKind {
    Step = 1,
    SmoothStep,
    Distance,
    OrbitTrap,
    Stripe,
}

impl LayerKind {
    pub fn text(self) -> &'static str {
        match self {
            LayerKind::Step => "Step Count",
            LayerKind::SmoothStep => "Smooth Step Count",
            LayerKind::Distance => "Distance Estimate",
            LayerKind::OrbitTrap => "Orbit Trap",
            LayerKind::Stripe => "Stripe Average",
        }
    }

    #[cfg(feature = "binary-deps")]
    pub fn icon_text(self) -> String {
        use egui_material_icons::icons;
        match self {
            LayerKind::Step => format!("{} Step Count", icons::ICON_STAIRS_2),
            LayerKind::SmoothStep => format!("{} Smooth Step Count", icons::ICON_ELEVATION),
            LayerKind::Distance => format!("{} Distance Estimate", icons::ICON_TARGET),
            LayerKind::OrbitTrap => format!("{} Orbit Trap", icons::ICON_ORBIT),
            LayerKind::Stripe => format!("{} Stripe Average", icons::ICON_AIRWAVE),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub enum Gradient {
    Flat([f32; 3]),
    Procedural([[f32; 3]; 4]),
    Manual(Vec<[f32; 4]>),
    Hsv(f32, f32),
    Oklch(f32, f32),
}
impl Gradient {
    pub fn decompose(&self) -> (u32, Vec<f32>) {
        match self {
            Gradient::Flat(data) => (0, [data[0], data[1], data[2], 1.0].to_vec()),
            // the values here are remapped to rgba to make the handling logic in the shader easier
            Gradient::Procedural(data) => (1, data.map(|x| [x[0], x[1], x[2], 1.0]).concat()),
            Gradient::Manual(data) => {
                let mut v = data.clone();
                v.sort_by(|a, b| {
                    a[3].fract()
                        .partial_cmp(&b[3].fract())
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                (2, v.concat())
            }
            Gradient::Hsv(saturation, value) => (3, vec![*saturation, *value, 1.0, 1.0]),
            Gradient::Oklch(lightness, chroma) => (4, vec![*lightness, *chroma, 1.0, 1.0]),
        }
    }
}
impl Default for Coloring {
    fn default() -> Self {
        Self {
            saturation: 1.0,
            brightness: 1.0,
            color_frequency: 1.0,
            color_offset: 0.0,
            gradient: Gradient::Hsv(0.7, 1.0),
            color_layers: vec![Layer {
                id: next_layer_id(),
                kind: LayerKind::SmoothStep,
                strength: 1.0,
                param: 0.0,
            }],
            lighting_kind: LightingKind::Gradient,
            light_layers: vec![Layer {
                id: next_layer_id(),
                kind: LayerKind::Distance,
                strength: 1.0,
                param: 0.0,
            }],
            lights: vec![],
            overlays: Overlays {
                iteration_outline: None,
                set_outline: None,
            },
        }
    }
}

impl Coloring {
    pub fn internal_default() -> Self {
        Self {
            saturation: 1.0,
            brightness: 1.0,
            color_frequency: 1.0,
            color_offset: 0.0,
            gradient: Gradient::Flat([1.0; 3]),
            color_layers: vec![],
            lighting_kind: LightingKind::Gradient,
            light_layers: vec![Layer {
                id: next_layer_id(),
                kind: LayerKind::OrbitTrap,
                strength: 1.0,
                param: 0.0,
            }],
            lights: vec![],
            overlays: Overlays {
                iteration_outline: None,
                set_outline: None,
            },
        }
    }

    pub fn external_opt_default() -> Self {
        Self {
            saturation: 0.9,
            brightness: 1.0,
            color_frequency: 1.0,
            color_offset: 0.0,
            gradient: Gradient::Procedural([[0.5; 3], [0.5; 3], [1.0; 3], [0.0, 0.1, 0.2]]),
            color_layers: vec![Layer {
                id: next_layer_id(),
                kind: LayerKind::SmoothStep,
                strength: 3.0,
                param: 0.0,
            }],
            lighting_kind: LightingKind::Shaded,
            light_layers: vec![Layer {
                id: next_layer_id(),
                kind: LayerKind::Step,
                strength: 3.0,
                param: 0.0,
            }],
            lights: vec![
                Light::new([1.0, 1.0, 1.0], 1.0, [0.0, 0.0, 1.0]),
                Light::new([0.5, 0.8, 1.0], 1.0, [0.7, 0.7, 0.0]),
                Light::new([1.0, 0.8, 0.4], 1.0, [-0.7, -0.7, 0.0]),
            ],
            overlays: Overlays {
                iteration_outline: None,
                set_outline: None,
            },
        }
    }

    pub fn internal_opt_default() -> Self {
        Self {
            saturation: 1.0,
            brightness: 1.0,
            color_frequency: 1.0,
            color_offset: 0.0,
            gradient: Gradient::Flat([0.1; 3]),
            color_layers: vec![],
            lighting_kind: LightingKind::Flat,
            light_layers: vec![],
            lights: vec![],
            overlays: Overlays {
                iteration_outline: None,
                set_outline: None,
            },
        }
    }

    pub fn contains_kind(&self, kind: LayerKind) -> bool {
        self.color_layers.iter().filter(|x| x.kind == kind).count() > 0
            || self.light_layers.iter().filter(|x| x.kind == kind).count() > 0
    }
}

impl Default for Light {
    fn default() -> Self {
        Self {
            color: [1.0; 3],
            strength: 0.0,
            direction: [0.0, 0.0, 1.0],
            padding: 0.0,
        }
    }
}

impl Light {
    pub fn new(color: [f32; 3], strength: f32, direction: [f32; 3]) -> Self {
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
        // prevent instability where calling normalize
        // repeatedly results in different values
        if (direction_length - 1.0).abs() < 1e-4 {
            return;
        }
        let direction = [
            direction[0] / direction_length,
            direction[1] / direction_length,
            direction[2] / direction_length,
        ];
        self.direction = direction;
    }
}
