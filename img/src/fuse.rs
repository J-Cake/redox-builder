use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use bufreaderwriter::rand::BufReaderWriterRand;
use fuser::{BackgroundSession, KernelConfig, mount2, MountOption, ReplyData, ReplyEmpty, ReplyIoctl, ReplyOpen, ReplyStatfs, ReplyWrite};
use fuser::FileAttr;
use fuser::Filesystem;
use fuser::FileType;
use fuser::ReplyAttr;
use fuser::ReplyDirectory;
use fuser::ReplyEntry;
use fuser::Request;
use fuser::spawn_mount2;
use libc::c_int;
use libparted::{Device, Disk, Partition};
use log::{debug, info};

use hub::error::*;
use hub::global::Inner;
use hub::paths::PathManager;

use crate::DiskManager;

#[link(name = "c")]
extern "C" {
    fn geteuid() -> u32;
    fn getegid() -> u32;
}

const MIN_BLOCK_SIZE: u64 = 4096;
const MAX_BLOCK_SIZE: u64 = MIN_BLOCK_SIZE.pow(2);

/// This filesystem exposes the partitions of a virtual disk to the host system via FUSE.
/// Since the partitions on the disk is set beforehand, it's safe to keep the file structure in memory.
pub(crate) struct PartitionFS {
    paths: Arc<PathManager>,
    mount_time: SystemTime,

    sector_size: u64,

    backing: BufReaderWriterRand<File>,
    root: DirItem,

    buffer: Vec<u8>,

    fd_map: HashMap<u64, u64>,
}

struct Attrs {
    mtime: SystemTime,
    size: u64,
    block_size: u64,
    kind: FileType,
    perm: u16,
}

impl Attrs {
    fn into_fileattrs(self, inode: u64) -> FileAttr {
        FileAttr {
            ino: inode,
            size: self.size,
            blocks: self.size / self.block_size.max(1),
            atime: self.mtime,
            mtime: self.mtime,
            ctime: self.mtime,
            crtime: self.mtime,
            kind: self.kind,
            perm: self.perm,
            nlink: 1,
            uid: unsafe { geteuid() },
            gid: unsafe { getegid() },
            rdev: 0,
            blksize: self.block_size as u32,
            flags: 0,
        }
    }

    fn dir() -> Self {
        Self {
            mtime: SystemTime::now(),
            size: 0,
            block_size: 0,
            kind: FileType::Directory,
            perm: 0x744,
        }
    }
}

#[derive(Debug)]
struct DirItem {
    parent: Option<u64>,
    name: String,
    inode: u64,
    node: FsNode,
    mtime: SystemTime,
    sector_size: u64,
}

type Sector = u64;

#[derive(Debug)]
enum FsNode {
    Dir(Vec<DirItem>),
    Partition {
        name: String,
        start: Sector,
        len: Sector,
    },
}

impl DirItem {
    fn getattr(&self) -> FileAttr {
        match &self.node {
            FsNode::Dir(_) => Attrs::dir(),
            FsNode::Partition { len, .. } => {
                Attrs {
                    mtime: self.mtime,
                    size: *len as u64 * self.sector_size,
                    block_size: self.sector_size,
                    kind: FileType::RegularFile,
                    perm: 0o744,
                }
            }
        }
            .into_fileattrs(self.inode)
    }

    fn readdir(&self) -> Result<impl Iterator<Item=&DirItem>> {
        if let FsNode::Dir(children) = &self.node {
            Ok(children.iter())
        } else {
            Err(Error::from(std::io::Error::from_raw_os_error(libc::EINVAL)))
        }
    }

    fn contains_inode(&self, inode: u64) -> bool {
        if self.inode == inode {
            return true;
        }

        return if let FsNode::Dir(dir) = &self.node {
            dir.iter().any(|i| i.contains_inode(inode))
        } else {
            false
        };
    }

    fn get_by_inode(&self, inode: u64) -> Option<&DirItem> {
        return if self.inode == inode {
            Some(self)
        } else if let FsNode::Dir(dir) = &self.node {
            dir.iter()
                .filter_map(|dir_item| dir_item.get_by_inode(inode))
                .next()
        } else if let Some(item) = self.get_by_inode(inode) {
            Some(item)
        } else {
            None
        };
    }
}

impl PartitionFS {
    pub(crate) fn new(paths: Arc<PathManager>) -> Result<Self> {
        let mount_time = SystemTime::now();

        let (root, sector_size) = {
            let mut dev = Device::new(&paths.final_image())?;
            dev.open()?;
            let sector = dev.sector_size();
            let mut disk = Disk::new(&mut dev)?;

            (
                DirItem {
                    parent: None,
                    name: "".into(),
                    inode: 1,
                    mtime: mount_time,
                    sector_size: sector,
                    node: FsNode::Dir(
                        disk.parts()
                            .filter_map(|part| part.name().map(|i| (i, part)))
                            .into_iter()
                            .enumerate()
                            .map(|(inode, (name, partition))| DirItem {
                                name: name.clone(),
                                parent: Some(0),
                                sector_size: sector,
                                inode: inode as u64 + 2,
                                node: FsNode::Partition {
                                    name: name.clone(),
                                    start: partition.geom_start() as u64,
                                    len: partition.geom_length() as u64,
                                },
                                mtime: mount_time,
                            })
                            .collect(),
                    ),
                },
                sector,
            )
        };

        Ok(Self {
            mount_time,
            root,
            sector_size,
            paths: Arc::clone(&paths),
            buffer: Vec::with_capacity(MIN_BLOCK_SIZE as usize),
            fd_map: HashMap::new(),
            backing: BufReaderWriterRand::new_reader(
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(paths.final_image())?,
            ),
        })
    }

    fn read_partition(&mut self, inode: u64, offset: u64, size: u64) -> Result<&[u8]> {
        let e_no_ent = Error::from(std::io::Error::from_raw_os_error(libc::ENOENT));
        let e_inval: Error = Error::from(std::io::Error::from_raw_os_error(libc::EINVAL));

        let FsNode::Partition { start, len, .. } =
            self.root.get_by_inode(inode).ok_or(e_inval)?.node
            else {
                return Err(e_no_ent);
            };

        let _ = self
            .backing
            .seek(SeekFrom::Start(start * self.sector_size + offset))?;

        // TODO: Ensure offset + size doesn't run over the partition's end
        let len = size.min(len * self.sector_size - offset);
        let cap = self.buffer.capacity() as u64;
        let read = self
            .backing
            .read(&mut self.buffer[0..len.max(MAX_BLOCK_SIZE).max(cap) as usize])?;

        Ok(&self.buffer[0..read])
    }

    fn write_partition(&mut self, inode: u64, offset: u64, data: &[u8]) -> Result<usize> {
        let e_no_ent = Error::from(std::io::Error::from_raw_os_error(libc::ENOENT));
        let e_inval: Error = Error::from(std::io::Error::from_raw_os_error(libc::EINVAL));

        let FsNode::Partition { start, len, .. } =
            self.root.get_by_inode(inode).ok_or(e_inval)?.node
            else {
                return Err(e_no_ent);
            };

        let _ = self
            .backing
            .seek(SeekFrom::Start(start * self.sector_size + offset))?;

        // TODO: Ensure offset + size doesn't run over the partition's end
        let written = self
            .backing
            .write(&data[0..data.len().min((len * self.sector_size - offset) as usize)])?;

        Ok(written)
    }
}

const TTL: Duration = Duration::from_secs(1);

impl Filesystem for PartitionFS {
    fn init(
        &mut self, _req: &Request<'_>, config: &mut KernelConfig,
    ) -> std::result::Result<(), c_int> {
        // config.add_capabilities()?;
        Ok(())
    }

    fn lookup(&mut self, _req: &Request<'_>, parent: u64, name: &OsStr, reply: ReplyEntry) {
        if let Some(DirItem {
                        node: FsNode::Dir(children),
                        ..
                    }) = self.root.get_by_inode(parent)
        {
            if let Some(child) = children
                .iter()
                .find(|child| name.eq(&OsString::from(&child.name)))
            {
                return reply.entry(&TTL, &child.getattr(), 0);
            }
        }

        return reply.error(libc::ENOENT);
    }

    fn getattr(&mut self, _req: &Request<'_>, inode: u64, mut reply: ReplyAttr) {
        if let Some(item) = self.root.get_by_inode(inode) {
            reply.attr(&TTL, &item.getattr());
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn open(&mut self, _req: &Request<'_>, inode: u64, _flags: i32, reply: ReplyOpen) {
        let fd = self.fd_map.keys().cloned().max().unwrap_or(0) + 1;

        debug!("Opened Partition - FD: {}", fd);

        self.fd_map.insert(fd, inode);

        reply.opened(0, 0);
    }

    fn read(
        &mut self, _req: &Request<'_>, inode: u64, _fh: u64, offset: i64, size: u32, flags: i32,
        lock_owner: Option<u64>, reply: ReplyData,
    ) {
        let data = match self
            .read_partition(inode, offset as u64, size as u64)
            .map_err(Error::into_inner)
        {
            Ok(len) => len,
            Err(Inner::IoError(io_err)) => {
                return reply.error(io_err.raw_os_error().unwrap_or(libc::EINVAL));
            }
            Err(_) => return reply.error(libc::EINVAL),
        };

        reply.data(data);
    }

    fn write(
        &mut self, _req: &Request<'_>, inode: u64, fh: u64, offset: i64, data: &[u8],
        write_flags: u32, flags: i32, lock_owner: Option<u64>, reply: ReplyWrite,
    ) {
        let written = match self
            .write_partition(inode, offset as u64, data)
            .map_err(Error::into_inner)
        {
            Ok(len) => len,
            Err(Inner::IoError(io_err)) => {
                return reply.error(io_err.raw_os_error().unwrap_or(libc::EINVAL));
            }
            Err(_) => return reply.error(libc::EINVAL),
        };

        reply.written(written as u32);
    }

    fn readdir(
        &mut self, _req: &Request<'_>, inode: u64, _fh: u64, offset: i64, mut reply: ReplyDirectory,
    ) {
        if let Some(parent) = self.root.get_by_inode(inode) {
            match parent.readdir().map_err(|err| err.into_inner()) {
                Ok(dir) => {
                    for (offset, dir_item) in dir.enumerate().skip(offset as usize) {
                        if reply.add(
                            dir_item.inode,
                            (offset + 3) as i64,
                            match dir_item.node {
                                FsNode::Dir(_) => FileType::Directory,
                                FsNode::Partition { .. } => FileType::BlockDevice,
                            },
                            OsString::from(&dir_item.name),
                        ) {
                            break;
                        }
                    }
                    reply.ok()
                }
                Err(Inner::IoError(io_err)) => {
                    reply.error(io_err.raw_os_error().unwrap_or(libc::EINVAL))
                }
                Err(_) => reply.error(libc::EINVAL),
            }
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn statfs(&mut self, _req: &Request<'_>, inode: u64, reply: ReplyStatfs) {
        debug!("FS STAT: {}", inode);

        // reply.statfs();
    }
}
