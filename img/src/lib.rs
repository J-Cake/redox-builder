use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use libparted::Constraint;
use libparted::Disk;
use libparted::FileSystemType;
use libparted::Geometry;
use libparted::Partition;
use libparted::PartitionTableType;
use libparted::PartitionType;
use log::{debug, warn};
use tokio::net::UnixStream;
use tokio::process::Command;

use hub::config::{ImageConfig, ImageFormat};
use hub::error::*;

#[cfg(feature = "qemu")]
use crate::qemu::QCow2;
use crate::raw::Raw;

pub mod qemu;
pub mod raw;

// TODO: Refactor to Traits

#[async_trait(? Send)]
pub trait DiskManager {
    fn backing(&self) -> PathBuf;
    fn image(&self) -> &ImageConfig;

    async fn create_disk<Backing: AsRef<Path>>(path: Backing, config: Arc<ImageConfig>) -> Result<Self>
        where
            Self: Sized;

    async fn mount(&mut self) -> Result<()>;
    async fn build(&mut self) -> Result<()> {
        self.mount().await?;

        let mut dev = libparted::Device::new(&self.backing())?;
        dev.open()?;

        let img = self.image();

        let total = img.size * 1024i64.pow(2) / dev.sector_size() as i64;
        let partitions = img
            .partitions
            .iter()
            .scan(total, |remaining, partition| {
                let start = total - *remaining;
                let len = match partition.size * 1024i64.pow(2) / dev.sector_size() as i64 {
                    x if x < 0 => *remaining - x.abs(),
                    x => x,
                };

                *remaining = *remaining - len;
                Some(Geometry::new(&dev, start, len).map_err(Error::from))
            })
            .collect::<Result<Vec<Geometry>>>()?;

        let mut disk = Disk::new_with_partition_table(&mut dev, PartitionTableType::GPT)?;

        for (geometry, partition) in partitions.into_iter().zip(img.partitions.iter()) {
            let fs = partition
                .filesystem
                .as_ref()
                .and_then(|fs| FileSystemType::get(fs.as_ref()));

            debug!("Found Filesystem: {}", fs.is_some());

            let mut real_partition = Partition::new(
                &disk,
                PartitionType::PED_PARTITION_NORMAL,
                fs.as_ref(),
                geometry.start(),
                geometry.end(),
            )?;

            real_partition.set_name(&partition.label)?;

            let constraint = Constraint::new_from_max(&geometry)?;

            disk.add_partition(&mut real_partition, &constraint)?;
        }

        disk.commit()?;
        Ok(())
    }
    async fn unmount(&mut self) -> Result<()>;
}

async fn get_disk_manager<Backing: AsRef<Path>>(
    path: Backing,
    img: Arc<ImageConfig>,
) -> Result<Box<dyn DiskManager>> {
    #[cfg(feature = "qemu")]
    return Ok(Box::new(QCow2::create_disk(path, img).await?));

    #[cfg(not(feature = "qemu"))]
    return Ok(Box::new(Raw::create_disk(path, img).await?));
}

/// This function is responsible for mounting the virtual disk and all its partitions such that each can be written to as if it
pub async fn prepare_image<Backing: AsRef<Path>>(
    backing: Backing,
    config: Arc<ImageConfig>,
) -> Result<Box<dyn DiskManager>> {
    tokio::fs::create_dir_all(
        backing
            .as_ref()
            .parent()
            .unwrap_or(backing.as_ref())
            .join("partitions"),
    )
        .await?;

    // let mut disk = DiskManager::create_disk(backing.as_ref(), config).await?;
    let mut disk = get_disk_manager(backing, config).await?;
    if let Err(err) = disk.build().await {
        disk.unmount().await?;
        warn!("Failed to build disk. Killing storage daemon");
        return Err(err.into());
    };

    Ok(disk)
}
