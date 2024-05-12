# Dependencies

This file lists the various dependencies required for each feature

## `cfg(feature = "qemu")`

> Allows building for qemu targets

| Dependency  | Purpose                                                         | Package         |
|-------------|-----------------------------------------------------------------|-----------------|
| `qemu-img`  | Provides tooling used to create images                          | `qemu`          |
| `libparted` | Allows direkt disk partition table manipulation without a shell | `libparted-dev` |
| `fusefat`   | Mounts FAT filesystems in FUSE                                  | `fusefat`       |
