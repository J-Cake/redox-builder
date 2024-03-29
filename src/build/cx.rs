use std::collections::HashMap;
use std::env::current_dir;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use convert_case::Casing;
use futures::executor::block_on;
use log::info;
use tokio::sync::OnceCell;

use crate::build::config::ConfigFile;
use crate::build::config::ImageFormat;
use crate::error::*;

use super::image::prepare_image;
use super::image::DiskManager;

#[derive(Debug)]
pub(crate) struct Context<'a> {
    pub final_img: PathBuf,
    pub disk_mgr: OnceCell<DiskManager<'a>>,
    pub env: Arc<HashMap<String, OsString>>,
}

impl<'a> Context<'a> {
    pub fn env<Var: AsRef<str>>(&self, var: Var) -> Option<&OsStr> {
        self.env.get(var.as_ref()).map(|i| i.as_ref())
    }
}

impl<'a> Drop for Context<'a> {
    fn drop(&mut self) {
        if let Some(mgr) = self.disk_mgr.take() {
            block_on(mgr.unmount()).expect("Failed to unbind storage daemon");
        }
    }
}

pub(crate) async fn mk_context<'a>(
    config: &'a ConfigFile,
    build_dir: Option<&'a PathBuf>,
) -> Result<Context<'a>> {
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

    let disk = prepare_image(&final_image, &config.image).await?;

    env.insert("BUILD_DIR".to_owned(), build_dir.clone().into_os_string());
    env.insert("IMAGE".to_owned(), final_image.clone().into_os_string());

    Ok(Context {
        disk_mgr: OnceCell::new_with(Some(disk)),
        env: Arc::new(env),
        final_img: final_image.clone(),
    })
}
