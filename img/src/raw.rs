use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::fs::OpenOptions;

use hub::config::ImageConfig;
use hub::error::*;

use crate::DiskManager;

pub struct Raw {
    backing: PathBuf,
    img: Arc<ImageConfig>,
}

#[async_trait(? Send)]
impl DiskManager for Raw {
    fn backing(&self) -> PathBuf {
        self.backing.clone()
    }

    fn image(&self) -> &ImageConfig {
        &self.img
    }

    async fn create_disk<Backing: AsRef<Path>>(
        path: Backing,
        config: Arc<ImageConfig>,
    ) -> Result<Self> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path.as_ref())
            .await?
            .set_len(config.size as u64 * 1024u64.pow(2)) // Units are probably in Bytes
            .await?;

        Ok(Self {
            backing: path.as_ref().to_owned(),
            img: Arc::clone(&config),
        })
    }

    async fn mount(&mut self) -> Result<()> {
        todo!()
    }

    async fn unmount(&mut self) -> Result<()> {
        todo!()
    }
}
