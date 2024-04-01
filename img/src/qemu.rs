use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use log::debug;
use tokio::net::UnixStream;
use tokio::process::Command;

use hub::config::{ImageConfig, ImageFormat};
use hub::error::*;

use crate::DiskManager;

#[cfg(feature = "qemu")]
#[derive(Debug)]
pub struct QCow2 {
    backing: PathBuf,
    image: Arc<ImageConfig>,
    qmp: Option<UnixStream>,
}

#[cfg(feature = "qemu")]
impl QCow2 {
    async fn qmp_send_message(socket: &mut UnixStream, value: serde_json::Value) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        let mut vec = vec![0u8; 1024];

        socket.writable().await?;
        socket
            .write_all(serde_json::to_string(&value)?.as_bytes())
            .await?;

        loop {
            socket.readable().await?;
            match socket.try_read(&mut vec) {
                Ok(read) if read > 0 => break,
                Ok(_) => Err(Error::from(BuildError::QmpQuitFail))?,
                Err(ref err) if err.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(err) => Err(Error::from(err))?,
            };
        }

        Ok(())
    }

    #[inline(always)]
    fn qmp_feature_negotiation() -> serde_json::Value {
        serde_json::json! {{ "execute": "qmp_capabilities", "arguments": { "enable": [] } }}
    }

    #[inline(always)]
    fn qmp_quit() -> serde_json::Value {
        serde_json::json! {{ "execute": "quit" }}
    }
}

#[cfg(feature = "qemu")]
#[async_trait(? Send)]
impl DiskManager for QCow2 {
    fn backing(&self) -> PathBuf {
        self.backing.clone()
    }

    fn image(&self) -> &ImageConfig {
        self.image.deref()
    }

    #[cfg(feature = "qemu")]
    async fn create_disk<Backing: AsRef<Path>>(
        path: Backing,
        config: Arc<ImageConfig>,
    ) -> Result<Self> {
        Command::new("qemu-img")
            .arg("create")
            .arg(path.as_ref())
            .args([
                format!("{}M", config.size).as_ref(),
                "-f",
                match config.format {
                    ImageFormat::Raw => "raw",
                    #[cfg(feature = "qemu")]
                    ImageFormat::QCow2 => "qcow2",
                },
            ])
            .spawn()?
            .wait()
            .await?;

        Ok(Self {
            backing: path.as_ref().to_owned(),
            qmp: None,
            image: config,
        })
    }

    async fn mount(&mut self) -> Result<()> {
        let monitor = self
            .backing
            .parent()
            .unwrap_or(self.backing.as_ref())
            .join("qemu-monitor.sock");

        Command::new("qemu-storage-daemon")
            .args([
                "--blockdev",
                format!(
                    "node-name=prot-node,driver=file,filename={}",
                    self.backing.to_string_lossy()
                )
                    .as_ref(),
            ])
            .args([
                "--blockdev",
                format!(
                    "node-name=fmt-node,driver={},file=prot-node",
                    match self.image.format {
                        ImageFormat::Raw => "raw",
                        #[cfg(feature = "qemu")]
                        ImageFormat::QCow2 => "qcow2",
                    }
                )
                    .as_ref(),
            ])
            .args([
                "--chardev",
                format!(
                    "socket,path={},server=on,wait=off,id=qemu-monitor",
                    monitor.to_string_lossy()
                )
                    .as_ref(),
            ])
            .args(["--monitor", "chardev=qemu-monitor"])
            .args([
                "--export",
                format!(
                    "type=fuse,id=exp0,node-name=fmt-node,mountpoint={},writable=on",
                    self.backing.to_string_lossy()
                )
                    .as_ref(),
            ])
            .arg("--daemonize")
            .spawn()?
            .wait()
            .await?;

        let mut socket = UnixStream::connect(monitor).await?;
        socket.readable().await?;

        let mut vec = vec![0u8; 1024];
        match socket.try_read(&mut vec) {
            Ok(read) if read > 0 => Ok(()),
            Ok(_) => Err(Error::from(BuildError::QmpQuitFail)),
            Err(err) => Err(err.into()),
        }?;

        Self::qmp_send_message(&mut socket, Self::qmp_feature_negotiation()).await?;

        self.qmp = Some(socket);

        debug!("Qemu Storage Daemon running");

        Ok(())
    }

    async fn unmount(&mut self) -> Result<()> {
        if let Some(socket) = self.qmp.as_mut() {
            debug!("Killing Storage Daemon");
            Self::qmp_send_message(socket, Self::qmp_quit()).await?;
        }

        Ok(())
    }
}
