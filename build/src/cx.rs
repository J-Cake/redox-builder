use std::collections::HashMap;
use std::env::current_dir;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fmt::{Debug, Formatter};
use std::path::PathBuf;
use std::sync::Arc;

use convert_case::Casing;
use futures::executor::block_on;
use log::info;
use tokio::sync::OnceCell;

use hub::config::ConfigFile;
use hub::config::ImageFormat;
use hub::error::*;
use img::DiskManager;
use img::prepare_image;

// #[derive(Debug)]
pub struct Context {
    pub final_img: PathBuf,
    pub disk_mgr: OnceCell<Box<dyn DiskManager>>,
    pub env: Arc<HashMap<String, OsString>>,
}

impl Debug for Context {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context")
            .field("final_img", &self.final_img)
            .field("env", &self.env)
            .field_with("disk_mgr", |f| write!(f, "({})", self.disk_mgr.get().map(|i| match i.image().format {
                ImageFormat::Raw => ".iso",
                #[cfg(feature = "qemu")]
                ImageFormat::QCow2 => ".qcow2"
            }).unwrap_or("")))
            .finish()
    }
}

impl Context {
    pub fn env<Var: AsRef<str>>(&self, var: Var) -> Option<&OsStr> {
        self.env.get(var.as_ref()).map(|i| i.as_ref())
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        if let Some(mut mgr) = self.disk_mgr.take() {
            block_on(mgr.unmount()).expect("Failed to unbind storage daemon");
        }
    }
}

pub async fn mk_context(
    config: &ConfigFile,
    build_dir: Option<&PathBuf>,
) -> Result<Context> {
    let mut env = HashMap::new();

    let build_dir = build_dir
        .cloned()
        .unwrap_or(current_dir().expect("Failed acquire cwd").join("build"));
    let final_image = build_dir
        .join(config.name.to_case(convert_case::Case::Kebab))
        .with_extension(match config.image.format {
            ImageFormat::Raw => "img",
            #[cfg(feature = "qemu")]
            ImageFormat::QCow2 => "qcow2",
        });

    tokio::fs::create_dir_all(&build_dir).await?;
    info!("Final Build Image: {:?}", &final_image);

    let disk = prepare_image(&final_image, Arc::clone(&config.image)).await?;

    env.insert("BUILD_DIR".to_owned(), build_dir.clone().into_os_string());
    env.insert("IMAGE".to_owned(), final_image.clone().into_os_string());

    Ok(Context {
        disk_mgr: OnceCell::new_with(Some(disk)),
        env: Arc::new(env),
        final_img: final_image.clone(),
    })
}
