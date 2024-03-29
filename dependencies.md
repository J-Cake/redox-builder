# Dependencies

This file lists the various dependencies required for each feature

## `cfg(feature = "qemu")`

> Allows building for qemu targets

| Dependency | Purpose                                                        | Package      |
| ---------- | -------------------------------------------------------------- | ------------ |
| `qemu-img` | Provides tooling used to create images                         | `qemu`       |
| `qemu-nbd` | Allows mounting QCoW2 formatted disks                          | `qemu`       |
| `ndbfuse`  | Exposes the qcow2 formatted disk as a raw disk to FUSE clients | `libnbd-bin` |
