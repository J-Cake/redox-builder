use std::ffi::CString;
use std::fs::File;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;

use log::{debug, info};

use hub::BuildError;
use hub::config::ImageConfig;
use hub::paths::PathManager;

pub fn mount_filesystems(image: Arc<ImageConfig>, path: Arc<PathManager>) -> hub::Result<MountHandle> {
    let mut files = vec![];

    for part in image.partitions.iter() {
        if let Some(fs) = part.filesystem.as_ref() {
            let source = path.partition(&part.label).expect("No blockdev defined for partition");
            let mount = path.live_part(&part.label).expect("No blockdev defined for partition");

            match fs.to_lowercase().as_str() {
                "fat32" => {
                    info!("Mounting FAT32: {:?} => {:?}", &source, &mount);
                    // TODO: Mount Fat32
                }
                "redoxfs" => {
                    info!("Mounting RedoxFS: {:?} => {:?}", &source, &mount);

                    let disk = redoxfs::DiskFile::open(source)?;
                    let fs = redoxfs::FileSystem::open(disk, None, None, false)?; // TODO: forward opts to mount

                    redoxfs::mount(fs, mount, |mount_path| {
                        debug!("RedoxFS mounted on: {:?}", mount_path);
                    })?;
                }
                _ => todo!()
            }
        }
    }

    Ok(MountHandle {
        partitions: files
    })
}

pub struct MountHandle {
    partitions: Vec<PathBuf>,
}

impl MountHandle {
    fn unmount_all(self) -> hub::Result<()> {
        for part in self.partitions {
            let cstr = CString::new(part.into_os_string().as_bytes())?;

            if unsafe { libc::umount2(cstr.as_ptr(), 0) } != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }

        Ok(())
    }
}