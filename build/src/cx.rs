use std::cell::OnceCell;
use std::collections::HashMap;
use std::env::current_dir;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt::{Debug, Formatter};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use log::info;

use hub::config::ConfigFile;
use hub::config::ImageFormat;
use hub::error::*;
use hub::paths::PathManager;
use img::DiskManager;
use img::mnt::mount_filesystems;
use img::preload_filesystems;

pub struct Context {
    pub disk_mgr: OnceCell<Box<dyn DiskManager>>,
    pub env: Arc<HashMap<String, OsString>>,
    pub paths: Arc<PathManager>,
}

impl Context {
    pub fn env<Var: AsRef<str>>(&self, var: Var) -> Option<&OsStr> {
        self.env.get(var.as_ref()).map(|i| i.as_ref())
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        if let Some(mut mgr) = self.disk_mgr.take() {
            mgr.unmount().expect("Failed to unbind storage daemon");
        }
    }
}

pub fn mk_context(
    config: Arc<ConfigFile>,
    path: Arc<PathManager>,
) -> Result<Context> {
    let mut env = HashMap::new();

    let build_dir = path.build_dir();
    let final_image = path.final_image();

    fs::create_dir_all(&build_dir)?;
    info!("Final Build Image: {:?}", &final_image);

    for i in config
        .image
        .partitions
        .iter()
        .filter_map(|part| path.live_part(&part.label))
    {
        fs::create_dir_all(i)?;
    }

    env.insert("BUILD_DIR".to_owned(), build_dir.clone().into_os_string());
    env.insert("IMAGE".to_owned(), final_image.clone().into_os_string());

    let mut cell = OnceCell::new();
    cell.set(preload_filesystems(Arc::clone(&config.image), Arc::clone(&path))?).map_err(|err| Error::from(BuildError::FailedToCreateImage))?;

    info!("Mounting partitions");
    mount_filesystems(Arc::clone(&config.image), Arc::clone(&path))?;

    Ok(Context {
        disk_mgr: cell,
        env: Arc::new(env),
        paths: Arc::clone(&path),
    })
}