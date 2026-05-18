use emath::{Pos2, Vec2};
use rug::Float;
use rug::ops::{CompleteRound, PowAssign};
use serde::{Deserialize, Serialize};
use wgpu::Extent3d;

use super::Coloring;
use crate::shared::types::Transform;
use crate::shared::wgsl_primitives;
use crate::types::{LayerKind, Rotate};

pub const STRIPES_ENABLED: u32 = 0x1;
pub const TOTAL_ANGLE_ENABLED: u32 = 0x2;
pub const ORBIT_ENABLED: u32 = 0x4;
pub const DERIVATIVE_ENABLED: u32 = 0x8;
pub const JULIA: u32 = 0x1000_0000;

/// A representation of the current fractal being rendered, including
/// the fractal location, settings, coloring, and image parameters
#[derive(Debug, Clone, PartialEq)]
pub struct ImgSpec {
    pub location: Location,
    pub style: Style,
    // canvas
    pub width: u32,
    pub height: u32,
    pub samples: u8,
    pub optimization_level: OptLevel,
}

/// A representation of a particular location for a particular
/// fractal, and the associated state necessary to see that location.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    // fractal parameters
    pub fractal_kind: FractalKind,
    // viewport
    pub center: ComplexPoint,
    pub zoom: f32,
    /// relative to positive x (real), in radians
    pub angle: f32,
    pub max_iter: u32,
    // internal rendering details
    pub probe_location: ComplexPoint,
}

/// Describes how to turn the various fractal measurements into a visible color
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub external_coloring: Coloring,
    pub internal_coloring: Coloring,
}

/// A representation of the current viewed portion of the fractal,
/// Useful to track the location and size of an image or viewport
/// relative to others.
#[derive(Debug, Clone)]
pub struct View {
    pub center: ComplexPoint,
    pub zoom: f32,
    pub angle: f32,
    pub width: u32,
    pub height: u32,
}

/// A representation of the steps that will need re-execution,
/// assuming the results of the source image are cached and
/// the destination image is being rendered.
#[derive(Clone, Copy, Debug)]
pub struct ImageDiff {
    pub reprobe: bool,
    pub recompute: bool,
    pub recolor: bool,
    pub rebuild: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ComplexPoint {
    #[serde(with = "FloatParser")]
    pub x: Float,
    #[serde(with = "FloatParser")]
    pub y: Float,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Algorithm {
    Directf32,
    Perturbedf32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OptLevel {
    /// Optimizes for preventing cache invalidation, assuming all features are needed
    #[default]
    CacheOptimized,
    /// Optimizes for the most accurate image while still being fast, ignoring potential future needs
    AccuracyOptimized,
    /// Optimizes for the fastest render possible, even at the cost of minor inaccuracy
    PerformanceOptimized,
}

#[derive(
    Debug, Default, Clone, PartialEq, Deserialize, Serialize, documented::DocumentedVariants,
)]
pub enum FractalKind {
    /// The Mandelbrot set
    #[default]
    Mandelbrot,
    /// The Julia Sets
    Julia(ComplexPoint),
}

impl Default for ImgSpec {
    fn default() -> Self {
        Self {
            location: Location::default(),
            style: Style::default(),
            width: 3840,
            height: 2160,
            samples: 1,
            optimization_level: OptLevel::AccuracyOptimized,
        }
    }
}

impl Default for Location {
    fn default() -> Self {
        Location {
            fractal_kind: FractalKind::Mandelbrot,
            zoom: -1.0,
            angle: 0.0,
            max_iter: 10000,
            center: ComplexPoint {
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
            probe_location: ComplexPoint {
                x: Float::with_val(53, -0.5),
                y: Float::with_val(53, 0.0),
            },
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self {
            external_coloring: Coloring::external_opt_default(),
            internal_coloring: Coloring::internal_opt_default(),
        }
    }
}

impl Default for ComplexPoint {
    fn default() -> Self {
        Self {
            x: Float::new(53),
            y: Float::new(53),
        }
    }
}

impl ImgSpec {
    pub fn algorithm(&self) -> Algorithm {
        match self.location.zoom {
            // TODO: This is a poor estimate for Julia sets
            x if x < 13.0 => Algorithm::Directf32,
            _ => Algorithm::Perturbedf32,
        }
    }

    /// Returns a View corresponding to this image
    pub fn view(&self) -> View {
        View {
            center: self.location.center.clone(),
            zoom: self.location.zoom,
            angle: self.location.angle,
            width: self.width,
            height: self.height,
        }
    }

    pub fn set_view(&mut self, view: View) {
        self.location.center = view.center;
        self.location.zoom = view.zoom;
        self.location.angle = view.angle;
        self.width = view.width;
        self.height = view.height;
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.width as f32, self.height as f32)
    }

    pub fn scale(&mut self, scale: f32) {
        self.width = (self.width as f32 * scale) as u32;
        self.height = (self.height as f32 * scale) as u32;
    }

    /// Compare this to another image, returning the diff between them.
    pub fn compare(&self, other: &Self) -> ImageDiff {
        // determine if we need to reallocate buffers or recompile shaders
        // (due to changing compile-time parameters)
        let rebuild = self.width != other.width
            || self.height != other.height
            // if there are more bits set, then there are more enabled features
            || (self.get_flags() & 0xFF).count_ones() > (other.get_flags() & 0xFF).count_ones()
            || self.get_flags() & 0xFF00_0000 != other.get_flags() & 0xFF00_0000
            || self.location.max_iter != other.location.max_iter;
        // if the max iteration or probe location has changed, re-run the probe
        let reprobe = self.location.max_iter != other.location.max_iter
            || self.location.probe_location != other.location.probe_location
            || self.algorithm() == Algorithm::Perturbedf32
                && other.algorithm() == Algorithm::Directf32
            || self.location.fractal_kind != other.location.fractal_kind
            || rebuild;
        // if the probe location has changed or the image viewport has changed, re-generate the delta grid
        // if the image generation parameters have changed, re-run the compute shader
        let recompute = self.location != other.location || reprobe;
        // if the image coloring parameters have changed, re-run the image render
        let recolor = self.style.external_coloring != other.style.external_coloring
            || self.style.internal_coloring != other.style.internal_coloring
            || recompute;
        ImageDiff {
            reprobe,
            recompute,
            recolor,
            rebuild,
        }
    }

    /// Adjust the location of the probe if it is no longer a good reference point
    pub fn update_probe(&mut self) {
        let mut relative_pos = self
            .view()
            .complex_to_px_delta(&self.location.probe_location);
        relative_pos = relative_pos / self.size();
        if relative_pos.x.abs() > 10.0 || relative_pos.y.abs() > 10.0 {
            // reset probe
            self.location.probe_location = self.location.center.clone();
        }
    }

    /// Returns the set of bit flags to send to the shader based on the
    /// features necessary to render this image and the optimization level.
    pub fn get_flags(&self) -> u32 {
        let kind_flags = match &self.location.fractal_kind {
            FractalKind::Mandelbrot => 0,
            FractalKind::Julia(_) => JULIA,
        };
        match self.optimization_level {
            OptLevel::CacheOptimized => {
                STRIPES_ENABLED
                    | TOTAL_ANGLE_ENABLED
                    | ORBIT_ENABLED
                    | DERIVATIVE_ENABLED
                    | kind_flags
            }
            OptLevel::AccuracyOptimized | OptLevel::PerformanceOptimized => {
                let mut flags = 0;
                if self.contains_kind(LayerKind::Stripe) {
                    flags |= STRIPES_ENABLED;
                }
                if self.style.internal_coloring.contains_kind(LayerKind::Step)
                    || self
                        .style
                        .internal_coloring
                        .contains_kind(LayerKind::SmoothStep)
                {
                    flags |= TOTAL_ANGLE_ENABLED;
                }
                if self.contains_kind(LayerKind::OrbitTrap) {
                    flags |= ORBIT_ENABLED;
                }
                if self
                    .style
                    .external_coloring
                    .contains_kind(LayerKind::Distance)
                    || self.style.external_coloring.overlays.set_outline.is_some()
                {
                    flags |= DERIVATIVE_ENABLED;
                }
                flags | kind_flags
            }
        }
    }

    fn contains_kind(&self, kind: LayerKind) -> bool {
        self.style.external_coloring.contains_kind(kind)
            || self.style.internal_coloring.contains_kind(kind)
    }

    pub fn extents(&self) -> Extent3d {
        Extent3d {
            width: (self.width as f64) as u32,
            height: (self.height as f64) as u32,
            depth_or_array_layers: 1,
        }
    }

    pub fn with_size(&self, arg: (u32, u32)) -> Self {
        let mut new = self.clone();
        new.width = arg.0;
        new.height = arg.1;
        new
    }
}

impl Style {
    pub fn opt_default() -> Self {
        Self {
            external_coloring: Coloring::external_opt_default(),
            internal_coloring: Coloring::internal_opt_default(),
        }
    }
}

impl Location {
    /// Updates the precision used for this location based on the zoom level
    pub fn update_prec(&mut self) {
        let prec = get_precision(self.zoom);
        if prec > self.center.x.prec() {
            self.center.x = Float::with_val(prec, self.center.x.clone());
            self.center.y = Float::with_val(prec, self.center.y.clone());
        }
    }
}

impl View {
    /// Derives the transforms from another viewport to this one
    pub fn transforms_from(&self, other: &Self) -> Transform {
        let scale = f32::powf(2.0, -(self.zoom - other.zoom));
        let self_aspect = self.aspect_scale();
        let aspect_scale = Vec2::splat(1.0) / other.aspect_scale();
        let offset = other.complex_to_px_delta(&self.center) / other.size() * 2.0;
        Transform {
            angle: self.angle - other.angle,
            _padding: 0.0,
            prescale: wgsl_primitives::Vec2::new(self_aspect.x, self_aspect.y),
            postscale: wgsl_primitives::Vec2::new(scale * aspect_scale.x, scale * aspect_scale.y),
            offset: offset.into(),
        }
    }

    /// Adjusts the zoom on this View to ensure the other View is
    /// visible with a small border. Assumes the other view has
    /// the same center.
    pub fn zoom_to_fit(&mut self, other: &Self) {
        self.zoom = other.zoom - self.zoom_offset_from(other) - 0.1;
    }

    /// Return the zoom difference between this and other, taking
    /// aspect ratio into account. Assumes the other view has
    /// the same center.
    pub fn zoom_offset_from(&self, other: &Self) -> f32 {
        let aspect = self.aspect_ratio() as f32;
        let other_aspect = other.aspect_ratio() as f32;
        if aspect < 1.0 {
            if other_aspect < 1.0 {
                (other_aspect / aspect).max(1.0)
            } else {
                1.0 / aspect
            }
        } else if other_aspect > 1.0 {
            (aspect / other_aspect).max(1.0)
        } else {
            aspect
        }
        .log2()
    }

    /// The aspect ratio of the viewport
    pub fn aspect_ratio(&self) -> f64 {
        self.width as f64 / self.height.max(1) as f64
    }

    pub fn aspect_scale(&self) -> Vec2 {
        let aspect = self.aspect_ratio() as f32;
        if aspect < 1.0 {
            Vec2::new(aspect, 1.0)
        } else {
            Vec2::new(1.0, 1.0 / aspect)
        }
    }

    pub fn scale(&self) -> Float {
        let mut scale = Float::with_val(get_precision(self.zoom), 2.0);
        scale.pow_assign(-self.zoom);
        scale
    }

    /// Transform a position in viewport pixel coordinates into a position
    /// in -1..1 space (0 at center)
    pub fn px_to_relative(&self, pos: Pos2, scaling: f32) -> Vec2 {
        Vec2::new(pos.x, self.height as f32 / scaling - pos.y) / self.size() * scaling * 2.0
            - Vec2::splat(1.0)
    }

    /// Gets the fractal coordinates of a pixel from viewport coordinates
    pub fn px_to_complex(&self, pos: Pos2, scaling: f32) -> ComplexPoint {
        let rotated_position =
            (self.px_to_relative(pos, scaling) * self.aspect_scale()).rotated(self.angle);

        let scale = self.scale();
        let r = rotated_position.x * scale.clone() + self.center.x.clone();
        let i = rotated_position.y * scale + self.center.y.clone();
        ComplexPoint { x: r, y: i }
    }

    /// Translates a delta from pixel units to complex units
    pub fn px_delta_to_complex_delta(&self, delta: Vec2, scaling: f32) -> ComplexPoint {
        let rotated_position =
            (delta * Vec2::new(1.0, -1.0) / self.size() * scaling * 2.0 * self.aspect_scale())
                .rotated(self.angle);

        let scale = self.scale();
        let r = rotated_position.x * scale.clone();
        let i = rotated_position.y * scale;
        ComplexPoint { x: r, y: i }
    }

    /// Returns the offset in pixels from the center of this viewport to
    /// the given location in fractal coordinates
    pub fn complex_to_px_delta(&self, point: &ComplexPoint) -> Vec2 {
        let scale = self.scale();
        let relative_position = Vec2::new(
            ((point.x.clone() - self.center.x.clone()) / scale.clone()).to_f32(),
            ((point.y.clone() - self.center.y.clone()) / scale).to_f32(),
        )
        .rotated(-self.angle);

        relative_position / self.aspect_scale() * 0.5 * self.size()
    }

    pub fn extents(&self) -> Extent3d {
        Extent3d {
            width: (self.width as f64) as u32,
            height: (self.height as f64) as u32,
            depth_or_array_layers: 1,
        }
    }

    pub fn size(&self) -> Vec2 {
        Vec2::new(self.width.max(1) as f32, self.height.max(1) as f32)
    }
}

impl ImageDiff {
    pub fn full() -> Self {
        ImageDiff {
            rebuild: true,
            reprobe: true,
            recompute: true,
            recolor: true,
        }
    }
}

impl ComplexPoint {
    pub fn new(x: Float, y: Float) -> Self {
        Self { x, y }
    }

    pub fn to_vec2(&self) -> Vec2 {
        Vec2 {
            x: self.x.to_f32(),
            y: self.y.to_f32(),
        }
    }

    pub fn rotate(&mut self, angle: f32) {
        self.x = self.x.clone() * angle.cos() - self.y.clone() * angle.sin();
        self.y = self.x.clone() * angle.sin() + self.y.clone() * angle.cos();
    }
}

// We use a custom implementation for serde
// of Float to get a radix of 10. This increases
// the space it takes on disk, but that is a smaller
// concern for this app.
#[derive(Deserialize, Serialize)]
#[serde(remote = "Float")]
struct FloatParser {
    #[serde(getter = "Float::value")]
    value: String,
    #[serde(getter = "Float::prec")]
    precision: u32,
}

trait Translate {
    fn value(&self) -> String;
}

impl Translate for Float {
    fn value(&self) -> String {
        self.to_string_radix(10, None)
    }
}

impl From<FloatParser> for Float {
    fn from(value: FloatParser) -> Self {
        Float::parse(value.value.clone())
            .map(|val| val.complete(value.precision))
            .unwrap_or(Float::new(53))
    }
}

/// Get the precision for a given zoom level
pub fn get_precision(zoom: f32) -> u32 {
    ((zoom * 1.25) as u32).max(53)
}
