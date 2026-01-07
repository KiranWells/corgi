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

use crate::types::{Coloring, ComplexPoint, FractalKind, Image, Layer, Parameters, next_layer_id};

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
enum ImageSpec {
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
    probe_location: ComplexPoint,
    zoom: f32,
    fractal: FractalKind,
    max_iter: u32,
    internal_coloring: Coloring,
    external_coloring: Coloring,
    width: u32,
    height: u32,
    samples: u8,
}

impl From<Image> for SavedLocation {
    fn from(val: Image) -> Self {
        val.parameters.into()
    }
}

impl From<Parameters> for SavedLocation {
    fn from(val: Parameters) -> Self {
        Self::V1(SavedLocationV1 {
            center: val.center.clone(),
            zoom: val.zoom,
            fractal: val.fractal_kind.clone(),
            max_iter: val.max_iter,
        })
    }
}

impl SavedLocation {
    pub fn apply(self, other: &mut Image) {
        #[expect(clippy::infallible_destructuring_match)]
        let latest = match self {
            SavedLocation::V1(v1) => v1,
        };
        other.parameters.center = latest.center;
        other.parameters.zoom = latest.zoom;
        other.parameters.fractal_kind = latest.fractal;
        other.parameters.max_iter = latest.max_iter;
    }
}

impl SavedStyle {
    pub fn apply(self, other: &mut Image) {
        #[expect(clippy::infallible_destructuring_match)]
        let latest = match self {
            SavedStyle::V1(v1) => v1,
        };
        other.internal_coloring = latest.internal_coloring;
        other.external_coloring = latest.external_coloring;
    }
}

impl From<Image> for SavedStyle {
    fn from(value: Image) -> Self {
        Self::V1(SavedStyleV1 {
            internal_coloring: value.internal_coloring.clone(),
            external_coloring: value.external_coloring.clone(),
        })
    }
}

impl From<Image> for ImageSpec {
    fn from(value: Image) -> Self {
        Self::V1(ImageSpecV1 {
            center: value.parameters.center,
            probe_location: value.parameters.probe_location,
            zoom: value.parameters.zoom,
            fractal: value.parameters.fractal_kind,
            max_iter: value.parameters.max_iter,
            internal_coloring: value.internal_coloring,
            external_coloring: value.external_coloring,
            width: value.parameters.width,
            height: value.parameters.height,
            samples: value.parameters.samples,
        })
    }
}

impl From<ImageSpec> for Image {
    fn from(value: ImageSpec) -> Self {
        match value {
            ImageSpec::V1(spec) => Image {
                parameters: Parameters {
                    width: spec.width,
                    height: spec.height,
                    samples: spec.samples,
                    fractal_kind: spec.fractal,
                    center: spec.center,
                    zoom: spec.zoom,
                    max_iter: spec.max_iter,
                    probe_location: spec.probe_location,
                },
                external_coloring: spec.external_coloring,
                internal_coloring: spec.internal_coloring,
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
    spec: ImageSpec,
}

impl From<Image> for _ProxyShim {
    fn from(value: Image) -> Self {
        Self {
            spec: ImageSpec::from(value),
        }
    }
}
impl From<_ProxyShim> for Image {
    fn from(value: _ProxyShim) -> Self {
        Image::from(value.spec)
    }
}

impl SafeSaveLoad for Image {
    type Proxy = _ProxyShim;
    fn load(path: &Path) -> Result<Self, SaveLoadError> {
        let mut image: Image = if is_metadata_supported(path) {
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
        update_ids(&mut image.internal_coloring.color_layers);
        update_ids(&mut image.internal_coloring.light_layers);
        update_ids(&mut image.external_coloring.color_layers);
        update_ids(&mut image.external_coloring.light_layers);
        Ok(image)
    }
}

impl SafeSaveLoad for SavedLocation {
    type Proxy = SavedLocation;
}

impl SafeSaveLoad for SavedStyle {
    type Proxy = SavedStyle;
}
