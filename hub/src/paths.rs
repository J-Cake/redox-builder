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
    pub fn final_image(&self) -> PathBuf {
        self.build_dir()
            .join(self.config.name.to_case(convert_case::Case::Kebab))
            .with_extension(match self.config.image.format {
                ImageFormat::Raw => "img",
                #[cfg(feature = "qemu")]
                ImageFormat::QCow2 => "qcow2",
            })
    }

    pub fn build_dir(&self) -> PathBuf {
        self.requested_build_dir
            .as_ref()
            .cloned()
            .unwrap_or(current_dir().expect("Failed acquire cwd").join("build"))
    }

    pub fn partitions(&self) -> PathBuf {
        self.build_dir().join("partitions")
    }

    pub fn partition<Part: AsRef<str>>(&self, partition: Part) -> Option<PathBuf> {
        self.config
            .image
            .partitions
            .iter()
            .find(|part| part.label.eq(partition.as_ref()))
            .map(|part| self.partitions().join(&part.label))
    }
}
