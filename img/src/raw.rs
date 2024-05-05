use std::fs::OpenOptions;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use hub::config::ImageConfig;
use hub::error::*;
use hub::paths::PathManager;

use crate::DiskManager;

pub struct Raw {
    backing: PathBuf,
    img: Arc<ImageConfig>,
    paths: Arc<PathManager>,
}

impl DiskManager for Raw {
    fn backing(&self) -> PathBuf {
        self.backing.clone()
    }

    fn image(&self) -> &ImageConfig {
        &self.img
    }

    fn paths(&self) -> &PathManager {
        self.paths.deref()
    }


    fn create_disk(config: Arc<ImageConfig>, paths: Arc<PathManager>) -> Result<Self> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(paths.final_image())?
            .set_len(config.size as u64 * 1024u64.pow(2))?; // Units are probably in Bytes

        Ok(Self {
            backing: paths.final_image().to_owned(),
            img: Arc::clone(&config),
            paths,
        })
    }

    fn mount(&mut self) -> Result<()> {
        todo!()
    }

    fn unmount(&mut self) -> Result<()> {
        todo!()
    }
}

// impl Drop for Raw {
//     fn drop(&mut self) {
//         self.unmount().expect("Failed to gracefully unmount")
//     }
// }