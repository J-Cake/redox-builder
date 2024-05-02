use std::env::current_dir;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use convert_case::Casing;

use crate::config::{ConfigFile, ImageFormat};

pub struct PathManager {
    config: Arc<ConfigFile>,
    requested_build_dir: Option<PathBuf>,
}

impl PathManager {
    pub fn new<BuildDir: AsRef<Path>>(config: Arc<ConfigFile>, request: Option<BuildDir>) -> Self {
        Self {
            config,
            requested_build_dir: request.map(|i| i.as_ref().to_owned()),
        }
    }

    /// The path of the final image
    pub fn final_image(&self) -> PathBuf {
        self.build_dir()
            .join(self.config.name.to_case(convert_case::Case::Kebab))
            .with_extension(match self.config.image.format {
                ImageFormat::Raw => "img",
                #[cfg(feature = "qemu")]
                ImageFormat::QCow2 => "qcow2",
            })
    }

    /// The directory in which build assets are stored.
    /// Considered the _working directory_ of the build process.
    /// All working resources are constrained to this directory, so deleting it effectively cleans the build graph.
    /// The notable exception to this are Cargo resources which are stored in the otherwise standard paths.
    pub fn build_dir(&self) -> PathBuf {
        self.requested_build_dir
            .as_ref()
            .cloned()
            .unwrap_or(current_dir().expect("Failed acquire cwd").join("build"))
    }

    /// The path where the PartFS filesystem is mounted - contains the raw partitions of the final image
    pub fn partitions(&self) -> PathBuf {
        self.build_dir().join("partitions")
    }

    /// Get the path to a particular partition's block device
    pub fn partition<Part: AsRef<str>>(&self, partition: Part) -> Option<PathBuf> {
        self.config
            .image
            .partitions
            .iter()
            .find(|part| part.label.eq(partition.as_ref()))
            .map(|part| self.partitions().join(&part.label))
    }

    /// The path containing the mounted filesystems per partition
    pub fn live(&self) -> PathBuf {
        self.build_dir().join("live")
    }

    /// The path of the mounted filesystem
    pub fn live_part<Part: AsRef<str>>(&self, partition: Part) -> Option<PathBuf> {
        self.config
            .image
            .partitions
            .iter()
            .find(|part| part.label.eq(partition.as_ref()))
            .map(|part| self.live().join(&part.label))
    }
}
