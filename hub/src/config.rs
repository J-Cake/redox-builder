use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub name: String,
    pub description: Option<String>,

    #[serde(default)]
    pub requires: Vec<PathBuf>,
    pub image: Arc<ImageConfig>,
    #[serde(default, rename = "component")]
    pub components: Vec<Component>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    pub label: String,
    pub description: Option<String>,
    pub size: i64,

    #[serde(default)]
    pub format: ImageFormat,

    #[serde(default, rename = "partition")]
    pub partitions: Vec<Partition>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageFormat {
    Raw,

    #[cfg(feature = "qemu")]
    QCow2,
}

impl Default for ImageFormat {
    fn default() -> Self {
        Self::Raw
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Partition {
    #[serde(default)]
    pub requires: Vec<String>,
    pub label: String,
    pub size: i64,

    #[serde(default)]
    pub setup: String,

    pub filesystem: Option<String>,

    /// This parameter is only useful if `filesystem` is defined.
    /// If set, will mount the filesystem and place all resources into it.
    #[serde(default, rename = "file")]
    pub files: Vec<File>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub path: PathBuf,

    #[serde(flatten)]
    pub content: FilesystemEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilesystemEntry {
    Symlink(PathBuf),
    Text(String),
    Artifact {
        component: String,
        artifact: PathBuf,
    },
    FromShell(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    pub name: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub yields: Vec<PathBuf>,

    #[serde(default, rename = "caching")]
    pub cache_mode: CacheMode,

    #[serde(flatten)]
    pub build_mode: BuildMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildMode {
    Cargo(Vec<String>),
    Shell(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CacheMode {
    /// Cached items persevere as long as possible.
    ///
    /// These can be cleared by a) explicitly requesting this item's cache be cleared or b) by performing a clean build.
    Aggressive,
    /// Cached items obey regular preservation rules.
    ///
    /// Components will refetch their sources if no prebuilt or existing build of the component exists.
    Normal,
    /// This component holds caches for the lifetime of the build only.
    ///
    /// Component source will be refetched at each build. Useful for automatically incrementing version numbers or including CI results etc.
    Transient,
}

impl Default for CacheMode {
    fn default() -> Self {
        Self::Normal
    }
}

// --- Modules ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportableModule {
    #[serde(rename = "component", default)]
    pub components: Vec<Component>,
}
