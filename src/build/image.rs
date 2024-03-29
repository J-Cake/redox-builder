use std::path::{Path, PathBuf};

use log::warn;
use tokio::net::UnixStream;
use tokio::process::Command;

use crate::build::config::{ImageConfig, ImageFormat};
use crate::error::*;

#[cfg(feature = "qemu")]
#[derive(Debug)]
pub struct DiskManager<'a> {
    backing: PathBuf,
    image: &'a ImageConfig,
    qmp: Option<UnixStream>,
}

#[cfg(not(feature = "qemu"))]
#[derive(Debug)]
pub struct DiskManager<'a> {
    backing: PathBuf,
    image: &'a ImageConfig,
}

impl<'a> DiskManager<'a> {
    #[cfg(feature = "qemu")]
    pub async fn create_disk<Backing: AsRef<Path>>(
        path: Backing,
        config: &'a ImageConfig,
    ) -> Result<Self> {
        Command::new("qemu-img")
            .arg("create")
            .arg(path.as_ref())
            .args([
                format!("{}M", config.size).as_ref(),
                "-f",
                match config.format {
                    ImageFormat::Raw => "raw",
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

    #[cfg(not(feature = "qemu"))]
    pub async fn create_disk<Backing: AsRef<Path>>(
        path: Backing,
        config: &'a ImageConfig,
    ) -> Result<Self> {
        OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&build_img)
            .await?
            .set_len(config.size as u64 * 1024u64.pow(2)) // Units are probably in Bytes
            .await?;

        Ok(Self {
            backing: path.as_ref().to_owned(),
            image: config,
        })
    }

    #[cfg(feature = "qemu")]
    pub async fn mount(&mut self) -> Result<()> {
        use log::debug;

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

        DiskManager::qmp_send_message(&mut socket, DiskManager::qmp_feature_negotiation()).await?;

        self.qmp = Some(socket);

        debug!("Qemu Storage Daemon running");

        Ok(())
    }

    #[cfg(feature = "qemu")]
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

    #[cfg(not(feature = "qemu"))]
    pub async fn mount(&mut self) -> Result<()> {
        todo!()
    }

    #[cfg(feature = "qemu")]
    pub async fn unmount(self) -> Result<()> {
        use log::debug;

        if let Some(mut socket) = self.qmp {
            debug!("Killing Storage Daemon");
            DiskManager::qmp_send_message(&mut socket, DiskManager::qmp_quit()).await?;
        }

        Ok(())
    }

    #[cfg(not(feature = "qemu"))]
    pub async fn unmount(self) -> Result<()> {
        todo!()
    }

    pub async fn build(&mut self) -> Result<()> {
        self.mount().await?;

        let mut dev = libparted::Device::new(&self.backing)?;
        dev.open()?;

        Ok(())
    }

    #[cfg(feature = "qemu")]
    #[inline(always)]
    fn qmp_feature_negotiation() -> serde_json::Value {
        serde_json::json! {{ "execute": "qmp_capabilities", "arguments": { "enable": [] } }}
    }

    #[cfg(feature = "qemu")]
    #[inline(always)]
    fn qmp_quit() -> serde_json::Value {
        serde_json::json! {{ "execute": "quit" }}
    }
}

/// This function is responsible for mounting the virtual disk and all its partitions such that each can be written to as if it
pub async fn prepare_image<Backing: AsRef<Path>>(
    backing: Backing,
    config: &ImageConfig,
) -> Result<DiskManager> {
    tokio::fs::create_dir_all(
        backing
            .as_ref()
            .parent()
            .unwrap_or(backing.as_ref())
            .join("partitions"),
    )
    .await?;

    let mut disk = DiskManager::create_disk(backing.as_ref(), config).await?;
    if let Err(err) = disk.build().await {
        disk.unmount().await?;
        warn!("Failed to build disk. Killing storage daemon");
        return Err(err.into());
    };

    // LoopControl::open()
    //     .and_then(|lc| lc.next_free())
    //     .and_then(|ld| ld.attach_file(backing.as_ref()).map(|_| ld))
    //     .and_then(|ld| Device::new(ld.path().expect("Failed to attach loop device")))
    //

    Ok(disk)
}
