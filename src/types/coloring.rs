/*!
# Style/Coloring Types
*/
use std::sync::atomic::AtomicU64;

use documented::{DocumentedFieldsOpt, DocumentedVariants};
use serde::{Deserialize, Serialize};

use crate::shared::coloring::main::Light;

/// The coloring parameters for the image. These are interpreted
/// slightly differently for internal and external coloring, as
/// some coloring algorithms are incompatible between the two.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, DocumentedFieldsOpt)]
#[serde(default)]
pub struct Coloring {
    pub saturation: f32,
    pub brightness: f32,
    /// How often the colors in the gradient repeat. This acts like a global 'strength' value.
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
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, DocumentedVariants)]
pub enum LightingKind {
    /// Full brightness over the entire image
    Flat,
    /// Layers are added, then used directly as the brightness value.
    Gradient,
    /// Layers are added, then adjusted to repeat from 0.0 to 1.0 brightness (using cosine).
    RepeatingGradient,
    /// Mimics a 3D shape with lighting.
    Shaded,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, DocumentedFieldsOpt)]
pub struct Layer {
    /// A unique ID used to ensure consistent UI state tracking. This should always be initialized with [`next_layer_id`]
    #[serde(skip)]
    pub id: u64,
    /// The type of the layer
    pub kind: LayerKind,
    /// How strongly this layer influences the end value
    pub strength: f32,
    /// An extra parameter value for the layer. The purpose varies for each type.
    pub param: f32,
}

impl PartialEq for Layer {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind && self.strength == other.strength && self.param == other.param
    }
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
#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, DocumentedVariants)]
pub enum LayerKind {
    /// Uses the number of iterations required for the point to escape. Has hard lines between colors.
    Step = 1,
    /// A version of step without hard lines. Logarithmic instead of linear.
    SmoothStep,
    /// Distance estimation to the edge of the set. Scales depending on the zoom level.
    Distance,
    /// Distance estimation to the edge of the set. Scales depending on the zoom level.
    OrbitTrap,
    /// Draws effects radiating from the edges of the fractal.
    Stripe,
}

impl LayerKind {
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

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, DocumentedVariants)]
pub enum Gradient {
    /// A single color
    Flat([f32; 3]),
    /// Generates a gradient using a procedural equation based on Inigo Quilez's simple color palettes. The result is seamless if the third parameter's values are whole numbers.
    Procedural([[f32; 3]; 4]),
    /// A repeating linear gradient with manual colors and gradient stops
    Manual(Vec<[f32; 4]>),
    /// A gradient with rotating hue using the HSV color space
    Hsv(f32, f32),
    /// A gradient with rotating hue using the OKLCH color space
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
