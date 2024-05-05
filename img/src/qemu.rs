use std::fs;
use std::future::Future;
use std::io::{Read, Write};
use std::ops::Deref;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::Arc;
use std::thread::JoinHandle;

use async_trait::async_trait;
use log::debug;

use hub::config::{ImageConfig, ImageFormat};
use hub::error::*;
use hub::paths::PathManager;

use crate::DiskManager;

pub struct QCow2 {
    backing: PathBuf,
    image: Arc<ImageConfig>,
    paths: Arc<PathManager>,
    proc: Option<QemuProc>,
}

struct QemuProc {
    qmp: UnixStream,
    qemu: JoinHandle<Result<ExitStatus>>,
    socket_path: PathBuf,
}

impl QemuProc {
    pub fn init(qcow2: &mut QCow2) -> Result<Self> {
        let monitor = qcow2
            .backing
            .parent()
            .unwrap_or(qcow2.backing.as_ref())
            .join("qemu-monitor.sock");

        let file_driver = format!(
            "node-name=prot-node,driver=file,filename={}",
            qcow2.backing.to_string_lossy()
        );
        let qcow_driver = format!(
            "node-name=fmt-node,driver={},file=prot-node",
            match qcow2.image.format {
                ImageFormat::Raw => "raw",
                #[cfg(feature = "qemu")]
                ImageFormat::QCow2 => "qcow2",
            }
        );
        let qemu_monitor = format!(
            "socket,path={},id=qemu-monitor",
            monitor.to_string_lossy()
        );
        let fuse_driver = format!(
            "type=fuse,id=exp0,node-name=fmt-node,mountpoint={},writable=on",
            qcow2.backing.to_string_lossy()
        );

        if monitor.exists() {
            fs::remove_file(&monitor)?;
        }

        let server = UnixListener::bind(monitor.clone())?;
        let (mut sender, receiver) = std::sync::mpsc::channel();

        let proc = std::thread::spawn(move || {
            let exit = Command::new("qemu-storage-daemon")
                .args(["--blockdev", file_driver.as_ref()])
                .args(["--blockdev", qcow_driver.as_ref()])
                .args(["--chardev", qemu_monitor.as_ref()])
                .args(["--monitor", "chardev=qemu-monitor"])
                .args(["--export", fuse_driver.as_ref()])
                .spawn()?
                .wait()?;

            if !exit.success() { sender.send(()).unwrap_or(()) }

            Ok(exit)
        });

        let (mut socket, _) = server.accept()?;

        let mut vec = vec![0u8; 1024];
        match socket.read(&mut vec) {
            Ok(read) if read > 0 => Ok(()),
            Ok(_) => Err(Error::from(BuildError::QmpQuitWrite0)),
            Err(err) => Err(err.into()),
        }?;

        Self::qmp_send_message(&mut socket, Self::qmp_feature_negotiation())?;

        debug!("Feature Negotiation complete");

        Ok(Self {
            qmp: socket,
            qemu: proc,
            socket_path: monitor.clone(),
        })
    }

    pub fn kill(mut self) -> Result<ExitStatus> {
        Self::qmp_send_message(&mut self.qmp, Self::qmp_quit())?;
        let status = self.qemu.join().map_err(|err| Error::from(BuildError::QmpQuitFail(err)))??;
        fs::remove_file(self.socket_path)?;
        return Ok(status);
    }

    fn qmp_send_message(socket: &mut UnixStream, value: serde_json::Value) -> Result<()> {
        let mut vec = vec![0u8; 1024];

        socket.write_all(serde_json::to_string(&value)?.as_bytes())?;

        loop {
            match socket.read(&mut vec) {
                Ok(read) if read > 0 => break,
                Ok(_) => Err(Error::from(BuildError::QmpQuitWrite0))?,
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

#[async_trait(? Send)]
impl DiskManager for QCow2 {
    fn backing(&self) -> PathBuf {
        self.backing.clone()
    }

    fn image(&self) -> &ImageConfig {
        self.image.deref()
    }
    fn paths(&self) -> &PathManager {
        self.paths.deref()
    }

    fn create_disk(config: Arc<ImageConfig>, path: Arc<PathManager>) -> Result<Self> {
        let create = Command::new("qemu-img")
            .arg("create")
            .arg(path.final_image())
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
            .wait()?;

        if create.success() {
            Ok(Self {
                backing: path.final_image(),
                image: config,
                paths: Arc::clone(&path),
                proc: None,
            })
        } else {
            Err(Error::from(BuildError::FailedToCreateImage))
        }
    }

    fn mount(&mut self) -> Result<()> {
        let proc = QemuProc::init(self)?;
        self.proc = Some(proc);
        debug!("Qemu Storage Daemon running");

        Ok(())
    }

    fn unmount(&mut self) -> Result<()> {
        debug!("Killing Storage Daemon");
        if let Some(proc) = self.proc.take() {
            proc.kill()?;
        };

        Ok(())
    }
}
// 
// impl Drop for QCow2 {
//     fn drop(&mut self) {
//         self.unmount().expect("Failed to gracefully unmount")
//     }
// }