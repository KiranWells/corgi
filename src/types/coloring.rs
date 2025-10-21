use std::sync::atomic::AtomicU64;

use egui_material_icons::icons;
use nanoserde::{DeJson, SerJson};

/// The coloring parameters for the image. These are interpreted
/// slightly differently for internal and external coloring, as
/// some coloring algorithms are incompatible between the two.
#[derive(Clone, Debug, PartialEq, DeJson, SerJson)]
pub struct Coloring {
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
    RepeatingGradient,
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

#[derive(Clone, Copy, Debug, PartialEq, DeJson, SerJson)]
pub struct Layer {
    #[nserde(skip)]
    pub id: u64,
    pub kind: LayerKind,
    pub strength: f32,
    pub param: f32,
}

impl Default for Layer {
    fn default() -> Self {
        Self {
            id: 0,
            kind: LayerKind::None,
            strength: 0.5,
            param: 0.0,
        }
    }
}

static LAYER_ID: AtomicU64 = AtomicU64::new(0);
pub fn next_layer_id() -> u64 {
    LAYER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
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
    Stripe,
}

impl LayerKind {
    pub fn text(self) -> &'static str {
        match self {
            LayerKind::None => "None",
            LayerKind::Step => "Step Count",
            LayerKind::SmoothStep => "Smooth Step Count",
            LayerKind::Distance => "Distance Estimate",
            LayerKind::OrbitTrap => "Orbit Trap",
            LayerKind::Stripe => "Stripe Average",
        }
    }
    pub fn icon_text(self) -> String {
        match self {
            LayerKind::None => format!("{} None", icons::ICON_REMOVE_SELECTION),
            LayerKind::Step => format!("{} Step Count", icons::ICON_STAIRS_2),
            LayerKind::SmoothStep => format!("{} Smooth Step Count", icons::ICON_ELEVATION),
            LayerKind::Distance => format!("{} Distance Estimate", icons::ICON_TARGET),
            LayerKind::OrbitTrap => format!("{} Orbit Trap", icons::ICON_ORBIT),
            LayerKind::Stripe => format!("{} Stripe Average", icons::ICON_AIRWAVE),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, DeJson, SerJson)]
pub enum Gradient {
    Flat([f32; 3]),
    Procedural([[f32; 3]; 4]),
    Manual([[f32; 4]; 3]),
    Hsv(f32, f32),
}
impl Default for Coloring {
    fn default() -> Self {
        Self {
            saturation: 1.0,
            brightness: 1.0,
            color_frequency: 1.0,
            color_offset: 0.0,
            gradient: Gradient::Hsv(0.7, 1.0),
            color_layers: [
                Layer {
                    id: next_layer_id(),
                    kind: LayerKind::SmoothStep,
                    strength: 2.0,
                    param: 0.0,
                },
                Layer::default(),
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
                    id: next_layer_id(),
                    kind: LayerKind::Distance,
                    strength: 0.8,
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
                Light::new([1.0, 1.0, 1.0], 1.0, [0.0, 0.5, 0.8]),
                Light::new([0.5, 0.6, 1.0], 1.0, [0.8, 0.0, 0.6]),
                Light::new([1.0, 0.8, 0.4], 1.0, [0.0, 0.8, 0.5]),
            ],
            overlays: Overlays {
                iteration_outline_color: [0.0; 4],
                set_outline_color: [0.0, 0.0, 0.0, 30.0],
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
            color_layers: [Layer::default(); 8],
            lighting_kind: LightingKind::Gradient,
            light_layers: [
                Layer {
                    id: next_layer_id(),
                    kind: LayerKind::OrbitTrap,
                    strength: 1.0,
                    param: 0.0,
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

    pub fn external_opt_default() -> Self {
        Self {
            saturation: 1.0,
            brightness: 1.0,
            color_frequency: 1.0,
            color_offset: 0.0,
            gradient: Gradient::Procedural([[0.5; 3], [0.5; 3], [1.0; 3], [0.0, 0.1, 0.2]]),
            color_layers: [
                Layer {
                    id: next_layer_id(),
                    kind: LayerKind::SmoothStep,
                    strength: 3.0,
                    param: 0.0,
                },
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
                Layer::default(),
            ],
            lighting_kind: LightingKind::Shaded,
            light_layers: [
                Layer {
                    id: next_layer_id(),
                    kind: LayerKind::Step,
                    strength: 3.0,
                    param: 0.0,
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
                Light::new([1.0, 1.0, 1.0], 3.0, [0.0, 0.1, 0.9]),
                Light::new([0.2, 0.4, 1.0], 3.0, [0.9, 0.0, 0.0]),
                Light::new([1.0, 0.6, 0.1], 3.0, [0.0, 0.8, 0.1]),
            ],
            overlays: Overlays {
                iteration_outline_color: [0.0; 4],
                set_outline_color: [0.0; 4],
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
            color_layers: [Layer::default(); 8],
            lighting_kind: LightingKind::Flat,
            light_layers: [Layer::default(); 8],
            lights: [Light::default(); 3],
            overlays: Overlays {
                iteration_outline_color: [0.0; 4],
                set_outline_color: [0.0; 4],
            },
        }
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
