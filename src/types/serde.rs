//! Translation types used for reliable serialization and deserialization
//! for different image formats.

// required output formats:
//     - fractal configuration
//         - location (center + zoom)
//         - algorithm + parameters
//         - limits (max_iter)
//     - style
//         - internal coloring
//         - external coloring
//     - full spec
//         - fractal config + style
//         - width, height, sampling
// Excluded:
// - debug parameters (except in debug mode)
// - optimization level - this should always be "correct" for rendered images

#![allow(dead_code)]
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use little_exif::metadata::Metadata;
use serde::{Deserialize, Serialize};

use crate::types::{
    Coloring, ComplexPoint, FractalKind, ImgSpec, Layer, Location, Style, next_layer_id,
};

#[derive(thiserror::Error, Debug)]
pub enum SaveLoadError {
    #[error("Failed to load metadata from image file")]
    MetadataLoad(#[from] std::io::Error),
    #[error("Failed to parse spec as a valid type")]
    SerdeParse(#[from] serde_json::Error),
    #[error("Missing description image metadata tag")]
    MissingDescription,
    #[error("Error reading file")]
    FileRead(std::io::Error),
    #[error("Error writing to file")]
    FileWrite(std::io::Error),
    #[error("Only part of the file was written")]
    PartialFileWrite,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
enum SavedLocation {
    V1(SavedLocationV1),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
enum SavedStyle {
    V1(SavedStyleV1),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
enum SavedImgSpec {
    V1(ImageSpecV1),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct SavedLocationV1 {
    center: ComplexPoint,
    zoom: f32,
    fractal: FractalKind,
    max_iter: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct SavedStyleV1 {
    internal_coloring: Coloring,
    external_coloring: Coloring,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
struct ImageSpecV1 {
    center: ComplexPoint,
    zoom: f32,
    angle: f32,
    fractal: FractalKind,
    max_iter: u32,
    internal_coloring: Coloring,
    external_coloring: Coloring,
    width: u32,
    height: u32,
    samples: u8,
}

impl From<ImgSpec> for SavedLocation {
    fn from(val: ImgSpec) -> Self {
        val.location.into()
    }
}

impl From<Location> for SavedLocation {
    fn from(val: Location) -> Self {
        Self::V1(SavedLocationV1 {
            center: val.center.clone(),
            zoom: val.zoom,
            fractal: val.fractal_kind.clone(),
            max_iter: val.max_iter,
        })
    }
}

impl SavedLocation {
    pub fn apply(self, other: &mut ImgSpec) {
        #[expect(clippy::infallible_destructuring_match)]
        let latest = match self {
            SavedLocation::V1(v1) => v1,
        };
        other.location.center = latest.center;
        other.location.zoom = latest.zoom;
        other.location.fractal_kind = latest.fractal;
        other.location.max_iter = latest.max_iter;
    }
}

impl SavedStyle {
    pub fn apply(self, other: &mut ImgSpec) {
        #[expect(clippy::infallible_destructuring_match)]
        let latest = match self {
            SavedStyle::V1(v1) => v1,
        };
        other.style.internal_coloring = latest.internal_coloring;
        other.style.external_coloring = latest.external_coloring;
    }
}

impl From<ImgSpec> for SavedStyle {
    fn from(value: ImgSpec) -> Self {
        Self::V1(SavedStyleV1 {
            internal_coloring: value.style.internal_coloring.clone(),
            external_coloring: value.style.external_coloring.clone(),
        })
    }
}

impl From<ImgSpec> for SavedImgSpec {
    fn from(value: ImgSpec) -> Self {
        Self::V1(ImageSpecV1 {
            center: value.location.center,
            zoom: value.location.zoom,
            angle: value.location.angle,
            fractal: value.location.fractal_kind,
            max_iter: value.location.max_iter,
            internal_coloring: value.style.internal_coloring,
            external_coloring: value.style.external_coloring,
            width: value.width,
            height: value.height,
            samples: value.samples,
        })
    }
}

impl From<SavedImgSpec> for ImgSpec {
    fn from(value: SavedImgSpec) -> Self {
        match value {
            SavedImgSpec::V1(spec) => ImgSpec {
                location: Location {
                    fractal_kind: spec.fractal,
                    center: spec.center.clone(),
                    zoom: spec.zoom,
                    angle: spec.angle,
                    max_iter: spec.max_iter,
                    probe_location: spec.center,
                },
                width: spec.width,
                height: spec.height,
                samples: spec.samples,
                style: Style {
                    external_coloring: spec.external_coloring,
                    internal_coloring: spec.internal_coloring,
                },
                optimization_level: super::OptLevel::AccuracyOptimized,
            },
        }
    }
}

pub fn is_metadata_supported(path: &Path) -> bool {
    matches!(path.extension(), Some(x) if x == "jpg" || x == "jpeg" || x == "png" || x == "webp" || x == "avif")
}

pub trait SafeSaveLoad: Sized + Clone {
    type Proxy: Into<Self> + From<Self> + Serialize;
    fn save(&self, path: &Path) -> Result<(), SaveLoadError> {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)?;
        let serialized = self.stringify()?;
        match file.write(serialized.as_bytes()) {
            Ok(written_amt) => {
                if written_amt < serialized.len() {
                    Err(SaveLoadError::PartialFileWrite)
                } else {
                    Ok(())
                }
            }
            Err(err) => Err(SaveLoadError::FileWrite(err)),
        }
    }

    fn load(path: &Path) -> Result<Self, SaveLoadError>
    where
        for<'de> Self::Proxy: Deserialize<'de>,
    {
        std::fs::read_to_string(path)
            .map_err(SaveLoadError::FileRead)
            .and_then(|s| Self::from_str(&s))
    }

    fn stringify(&self) -> Result<String, SaveLoadError> {
        let proxy: Self::Proxy = self.clone().into();
        serde_json::to_string(&proxy).map_err(SaveLoadError::SerdeParse)
    }

    fn from_str(string: &str) -> Result<Self, SaveLoadError>
    where
        for<'de> Self::Proxy: Deserialize<'de>,
    {
        serde_json::from_str(string)
            .map_err(SaveLoadError::SerdeParse)
            .map(Self::Proxy::into)
    }
}

/// A pseudo-private wrapper type meant to allow internal types to remain private
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
pub struct _ProxyShim {
    spec: SavedImgSpec,
}

impl From<ImgSpec> for _ProxyShim {
    fn from(value: ImgSpec) -> Self {
        Self {
            spec: SavedImgSpec::from(value),
        }
    }
}
impl From<_ProxyShim> for ImgSpec {
    fn from(value: _ProxyShim) -> Self {
        ImgSpec::from(value.spec)
    }
}

impl SafeSaveLoad for ImgSpec {
    type Proxy = _ProxyShim;
    fn load(path: &Path) -> Result<Self, SaveLoadError> {
        let mut image: ImgSpec = if is_metadata_supported(path) {
            let meta = Metadata::new_from_path(path)?;
            let tag = meta
                .get_tag(&little_exif::exif_tag::ExifTag::ImageDescription(
                    String::new(),
                ))
                .next()
                .ok_or(SaveLoadError::MissingDescription)?;
            let little_exif::exif_tag::ExifTag::ImageDescription(desc) = tag else {
                return Err(SaveLoadError::MissingDescription);
            };
            Self::from_str(desc)?
        } else {
            std::fs::read_to_string(path)
                .map_err(SaveLoadError::FileRead)
                .and_then(|s| Self::from_str(&s))?
        };
        fn update_ids(layers: &mut [Layer]) {
            for layer in layers {
                layer.id = next_layer_id();
            }
        }
        update_ids(&mut image.style.internal_coloring.color_layers);
        update_ids(&mut image.style.internal_coloring.light_layers);
        update_ids(&mut image.style.external_coloring.color_layers);
        update_ids(&mut image.style.external_coloring.light_layers);
        Ok(image)
    }
}

impl SafeSaveLoad for SavedLocation {
    type Proxy = SavedLocation;
}

impl SafeSaveLoad for SavedStyle {
    type Proxy = SavedStyle;
}
