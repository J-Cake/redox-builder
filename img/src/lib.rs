use std::ffi::CString;
use std::fs;
use std::fs::File;
use std::os::unix::prelude::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
// use std::sync::mpsc::Receiver;
use std::time::Duration;

use fuser::{BackgroundSession, mount2, MountOption, spawn_mount2};
use libparted::Constraint;
use libparted::Disk;
use libparted::FileSystemType;
use libparted::Geometry;
use libparted::Partition;
use libparted::PartitionTableType;
use libparted::PartitionType;
use log::{debug, error, info, warn};
use rayon::prelude::*;
use redoxfs::DiskFile;

use hub::config::ImageConfig;
use hub::error::*;
use hub::paths::PathManager;

use crate::fuse::PartitionFS;
#[cfg(feature = "qemu")]
use crate::qemu::QCow2;
#[cfg(not(feature = "qemu"))]
use crate::raw::Raw;

pub mod fuse;
#[cfg(feature = "qemu")]
pub mod qemu;
pub mod raw;
pub mod mnt;

// pub type AbortSignal = Receiver<()>;

pub trait DiskManager {
    fn backing(&self) -> PathBuf;
    fn image(&self) -> &ImageConfig;
    fn paths(&self) -> &PathManager;

    fn create_disk(config: Arc<ImageConfig>, path: Arc<PathManager>) -> Result<Self>
        where
            Self: Sized;

    fn mount(&mut self) -> Result<()>;
    fn build(&mut self) -> Result<()> {
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
    fn unmount(&mut self) -> Result<()>;
}

fn get_disk_manager(img: Arc<ImageConfig>, path: Arc<PathManager>) -> Result<Box<dyn DiskManager>> {
    #[cfg(feature = "qemu")]
    return Ok(Box::new(QCow2::create_disk(img, path)?));

    #[cfg(not(feature = "qemu"))]
    return Ok(Box::new(Raw::create_disk(path, img)?));
}

/// This function is responsible for mounting the virtual disk and all its partitions such that each can be written to as if it
/// TODO: This function is currently implemented using FUSE. If the user has superuser access or simply prefers not to, we should use system-level APIs like loopbacks because they're faster.
pub fn preload_filesystems(config: Arc<ImageConfig>, path: Arc<PathManager>) -> Result<Box<dyn DiskManager>> {
    if !path.partitions().exists() {
        fs::create_dir_all(path.partitions())?;
    }

    let mut disk = get_disk_manager(Arc::clone(&config), Arc::clone(&path))?;
    disk.mount()?;
    if let Err(err) = disk.build() {
        error!("Failed to build disk. Killing storage daemon");
        disk.unmount()?;
        return Err(err.into());
    };

    let paths = Arc::clone(&path);

    let pfs = PartitionFS::new(Arc::clone(&paths))?;
    let opt = &[
        MountOption::DefaultPermissions,
        MountOption::FSName("PartitionFS".to_owned()),
        MountOption::RW,
    ];

    let _fuse = spawn_mount2(pfs, paths.partitions(), opt)?;

    let _ = config
        .partitions
        .par_iter()
        .map(|partition| -> Result<()> {
            let partition_blockdev = path.partition(&partition.label).expect(
                format!("No blockdev defined for partition '{}'", &partition.label).as_ref(),
            );

            match partition.filesystem.as_ref().map(|i| i.as_str()) {
                Some("redoxfs") => {
                    let disk = DiskFile::open(&partition_blockdev)?;
                    let ctime = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap();

                    redoxfs::FileSystem::create_reserved(
                        disk,
                        None,
                        &[],
                        ctime.as_secs(),
                        ctime.subsec_nanos(),
                    )?;

                    Ok(())
                }
                Some("fat32") => {
                    if Command::new("mkfs.fat")
                        .arg(partition_blockdev)
                        .spawn()?
                        .wait()?
                        .success()
                    {
                        Ok(())
                    } else {
                        return Err(BuildError::FailedToCreateFilesystem("FAT32".into()).into());
                    }
                }
                // TODO: look up user-defined filesystems from parent config
                Some(fs) => Err(Error::from(BuildError::UnrecognisedFilesystem(
                    fs.to_owned(),
                ))),
                _ => Ok(()),
            }
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(disk)
}
