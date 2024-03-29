# 1. Layout

The Image consists of several parts whose structure and function hopefully become clear as the examples become more
complex. Images are nothing more than files containing bytes which represent the contents of a bootable volume, such as
a hard drive, USB or CD. A build config's purpose is to describe in as much detail as possible (to allow for
customisation) how to _build_ such a disk. The build system is opinionated and optimised for building specifically
various flavours of Redox OS. While it is possible to use this system to build other OS images, your mileage may vary.

From this point on, a physical volume containing an OS will be referred to as a disk. Despite this being a somewhat
lacking term for it, it's by far the simplest and most well understood.

Most disks are divided into partitions. These are separated contiguous regions of storage with some metadata associated
to it. Most operations will treat these as completely separate from one-another, and can often even be seen as an
entirely separate disk. They have varying purposes from storing a user's or a system's files necessary for operation, to
holding binary instructions on how to boot the OS.

## 1.1 EFI

One of the most important partitions in use is the EFI partition. It is generally very small in comparison to
data-carrying partitions, as it only contains a simple known structure to describe the methods of booting into an OS.

> The term EFI stands for *Extensible Firmware Interface*. It is a standardised set of protocols used to power a
> computer before and after the control of an operating system. It (among other things) defines how data must be laid
> out in the EFI partition to bring the computer into a usable state.
>
> Before (U)EFI, computers used a system called BIOS, which despite its may flaws and virtually non-existent
> standardisation was the status-quo for a very long time. Most computers still support this mode, but refer to it as
> *Legacy boot mode* or similar.
>
> While it is recommended to account for it in older devices, it adds unnecessary complexity in simplified examples such
> as this. Therefore it will be treated as out-of-scope in favour of (U)EFI.

## 1.2 Root

The Operating System will store its kernel and most crucial configuration information on a partition known as the root
partition. It is formatted with a filesystem, to allow for addressing and organising files and groups of files, as well
as their respective access levels. Redox has defined its own format for this, aptly named `redoxfs`.

The root filesystem contains various other pieces of software, drivers, kernels, and in some circumstances also user
data. Redox typically follows the UNIX structure for organising configuration and programs around the system.

| Directory Name     | Function                                                                                                                                                                                                                                                                                                                              |
|--------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `/bin`             | Acts as a well-defined place for storing executable binaries. Most commands accessible through your shell are just invocations of these binaries.                                                                                                                                                                                     |
| `/boot`            | Contains all data necessary for booting the OS                                                                                                                                                                                                                                                                                        |
| `/dev`             | On Linux and other UNIX-like OSes, holds device files such as graphics cards, disks, displays, network adapters etc. Redox takes a different approach, and instead relies on [schemes](#scheme) to communicate hardware and other soft devices. This folder merely exists for backwards-compatibility with traditional UNIX programs. |
| `/etc`             | The exact acronym of this folder is an ongoing debate. Many refer to it as _Edit to Configure_ as the primary purpose of this directory is to hold global configuration data of various applications and services. It is often grouped by program here                                                                                |
| `/filesystem.toml` | This is a copy of the configuration file used to build the OS. This file only exists in images built using the traditional cookbook system.                                                                                                                                                                                           | 
| `/home`            | User data is stored here. A user will typically have their own dedicated folder where only they have permission to view or edit its contents                                                                                                                                                                                          |
| `/pkg`             | Contains data that the Redox package manager uses to update the system without requiring a rebuild                                                                                                                                                                                                                                    |
| `/root`            | This directory serves as the home folder for the root user. It serves the same purpose as folders in `/home`, except specifically for root.                                                                                                                                                                                           |
| `/tmp`             | The mountpoint for an in-memory filesystem, used to store information programs or users will only use for a short time, then discard. It is generally much faster than writing files to disk, and is erased at reboot or sometimes more often.                                                                                        |
| `/usr/lib`         | Contains libraries that programs can link to at runtime.                                                                                                                                                                                                                                                                              |  
| `/usr/share`       | Data that users share with each other. This may be a global index of available software, a list of preinstalled fonts, games or other programs                                                                                                                                                                                        |

## 2. Booting

_Booting_ refers to the process of bootstrapping the system from a powered-down state to a functional state. In a Redox
system as well as most UNIX systems, the root filesystem contains the kernel in the form of a binary file located in a
directory called `/boot`.

<details>
    <summary>The boot process</summary>

The boot process is often divided into several stages. Broadly the boot process can be described as follows:

1. Firmware Start - The motherboard's electronics trigger a CPU reset, effectively setting it back to fresh state. The
   UEFI system is loaded from ROM and begins running. It searches for available storage media which may contain the UEFI
   filesystem where firmware settings, upgrade files, bootloaders and various other things are stored. After a hardware
   check, UEFI will attempt to locate the bootloader to pass control to and switch to it.
2. Bootloader - A bootloader is a separate piece of firmware provided by the operating system used to initialise the
   boot process. Historically, the bootloader was constrained to the first sector of a floppy drive, a measly 512B of
   storage. It also had to contain a special 4-byte marker to indicate that it was a boot sector. The code required to
   locate the kernel of the operating system, load it and switch to it had be contained in this sector. Nowadays, UEFI
   makes this far simpler by foregoing bootloaders entirely - UEFI can load kernels directly.
3. Kernel start - Once the kernel is loaded, it begins initialising more complex hardware, such as drives, network cards
   and GPUs. The OS will typically use its own filesystem, so must bring the necessary systems into existence before
   being able to start any other system processes.
4. Chief among which is a task-management system responsible for managing services, daemons and various other user-space
   systems. A task dependency graph is interpreted based on available hardware and user configuration and brings the
   remaining processes into a functional state. At this point the exact completion of the boot process becomes unclear
   as services which may continue running into an active session may be necessary for a functional system. An example
   may be a graphical environment.

</details>

# 2. Configuration

Throughout this document, the following terms will be used when referring to structures or features within
configuration:

| Term         | Definition                                                                              |
|--------------|-----------------------------------------------------------------------------------------|
| _Partition_  | A segregated region of the final disk whose contents can be controlled by scripts       |
| _Image_      | The resulting binary file containing a virtualised hard disk                            |
| _Job_        | Parallelisable unit of work which produces zero or more artifacts                       |
| _Artifact_   | A resource (file) with some significance                                                | 
| _Dependency_ | Tasks which must yield its artifacts to another before it can run                       |
| _Component_  | A reusable structure of jobs producing an artifact which may be used in the image later |
| _Script_     | Shellcode used to generate artifacts from input variables                               |

A configuration describes in depth the steps required to produce an image. It is at a high level comparable to a
makefile in that it describes tasks, their dependencies, their artifacts and how to perform them with a number of
notable differences: config files use TOML to define build steps for a less free structure.

## 2.1 Structure

| Table                      | Function                                                                                                                            |
|----------------------------|-------------------------------------------------------------------------------------------------------------------------------------|
| `[image]`                  | Provides metadata to the build system                                                                                               |
| `[[image.partition]]`      | Creates a partition on the image                                                                                                    |
| `[[image.partition.file]]` | Produces a value which is sent to a handler in the partition                                                                        |
| `[[component]]`            | Defines a component. The primary advantage is in incremental builds, as components are by their nature unlikely to reuse each-other |

Any value may be factorised into a separate file and included by placing a relative path into the top-level `requires`
array.

## 2.2 Image

## 2.3 Partition

## 2.4 File

## 2.5 Component