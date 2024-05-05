# Redox Image Builder

This package is designed to build customised Redox OS images. It offers very powerful mechanisms to build exactly the
desired image.

## Image Customisation

Creating images involves configurations. These are complete, all-inclusive descriptions of the final image. You have
complete control over all aspects of the image from included software to boot priority.

> Please note that while this
> system is very flexible, and theoretically can be used to create images of any other OS, this is explicitly
> out-of-scope. You will notice various Redox-OS specific design decisions throughout the customisation process. This is
> very intentional.

Roughly, the configuration process is divided into three "areas". They are

| Area              | Purpose                                                                                                                                                     |
|-------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------|
| Disk Image Layout | Defining partition layout, image size, layouts, filesystems, encryption, UEFI parameters, live boot mode etc.                                               |
| Components        | A list of software which the image is to contain, often referred to as recipes, but due to their very broad application, the term _component_ is preferred. | 
| Assembly          | How the final image is structured, including standard software, filesystem structure, permissions, customisations, etc.                                     |

While these don't have direct analogues during the customisation-authoring process, it is often helpful to understand
this divide.

Image customisations are written in `TOML` or any markup which compiles to it, although this is left to the user.
Several [example configurations](./examples/) are provided within this repository.

## Reference

Despite its mission statement, `TOML` can be unintuitive to a human reader. Hence, the table below uses a slightly
modified notation to make parent-child structures very clear. Keys which belong to a specific table are annotated
using `::` as found in Rust.

#### Units

Unit consistency is a priority. Below is a mapping of data type to units.

| Data Type                | Unit                                                       |
|--------------------------|------------------------------------------------------------|
| File / Blob / Chunk Size | Megabytes (MiB i64) [1024 ** 3 Bytes]                      |
| Network Transfer Speed   | Megabits per Second (Mb/s f64) [1024 ** 3 bits per second] |
| Duration                 | Seconds [u64]                                              |
| Date / Time              | UNIX Timestamp (ms since 01/01/1970 00:00:00.000) [u64]    |

### `::`

| Key               | Type      | Description                                                                                                                                                                                                                                                                                                                                                         |
|-------------------|-----------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `::name`          | string    | A friendly name for your image. This value is not used during the build process, and serves mainly as a way to identify the image                                                                                                                                                                                                                                   |
| `::description`   | string    | A longer friendly description of your image's purpose, selling-points etc.                                                                                                                                                                                                                                                                                          |
| `::requires`      | [string]  | A list of files (optional `.toml` extension) to be included in the image. Each mentioned file is _appended_ to the parent after the parent's content is finished parsing. All keys described in this table are valid here. Any duplicate keys specified within the parent file are treated with a higher precedence, and override values defined in imported files. |
| `::[[component]]` | Component | A software component converting a list of sources to a list of artifacts. See [`Components`](#component) for more                                                                                                                                                                                                                                                   |

### `::component`

A component is a reproducible set of artifacts which may be referenced throughout the image to place software, blobs or
files into the final image. They obey a set of caching rules to ensure sources are always up-to-date. This is optional
and version-locks can be introduced by altering the source URL.

You may define as many of these as needed. Note though that each must be uniquely named. Components with duplicate names
are overridden according to the priority rules outlined above.

| Key                         | Type                                                       | Description                                                                                                                                                                                                                               |
|-----------------------------|------------------------------------------------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `::[[component]]::name`     | string                                                     | An identifier used to refer back to the component within the image.                                                                                                                                                                       |
| `::[[component]]::requires` | [string]                                                   | A list of components or URLs to fetch sources from. Permitted URL types are [documented here](#url-types)                                                                                                                                 |
| `::[[component]]::yields`   | [string]                                                   | A list of artifacts the component emits. Each can be referred to by concatenating the component's name with `::` and the artifact's name,                                                                                                 |
| `::[[component]]::caching`  | `aggresive` \| `normal` \| `transient` (default: `normal`) | How artifacts are preserved and reused. See [caching rules](#caching) for more info                                                                                                                                                       |
| `::[[component]]::shell`    | shell                                                      | A [shell script](#shell) to build the component and produce the artifacts. If an artifact mentioned in  `::[[component]]::yields` cannot be found, the build is considered to have failed. Mutually exclusive to `::[[component]]::cargo` |
| `::[[component]]::cargo`    | [string]                                                   | Arguments passed to `cargo build`. Mutually exclusive to `::[[component]]::shell`                                                                                                                                                         |

### `::image`

A description of the disk's layout, including partitions, encryption, format, size etc.

| Key                          | Type                              | Description                                                                                                                                                                                |
|------------------------------|-----------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `::[image]::label`           | string                            | A friendly identifier to make the image identifiable to humans or within the image itself.                                                                                                 |
| `::[image]::size`            | MiB                               | The size of the disk. Error if negative                                                                                                                                                    |
| `::[image]::format`          | `qcow2` \| `raw` (default: `raw`) | Which format the resulting image should contain. The `qcow2` format is feature-gated under `qemu` . While it is a standard feature, you may have to [compile](#Building) this in yourself. |
| `::[image]::partition_table` | `gpt` \| `mbr` (default: `gpt`)   | Which partition table type to use. It is strongly recommended to use `GPT`                                                                                                                 |

### `::image::partition`

A large segment on the disk image used to segregate major parts of the image. Often contains a filesystem.

| Key                                    | Type                  | Description                                                                                                                                                                                                                                                                                                 |
|----------------------------------------|-----------------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `::[image]::[[partition]]::label`      | string                | The name the partition will receive. This can be seen for example when `lsblk <image>`. Must obey the partition naming rules for the partition table type                                                                                                                                                   |
| `::[image]::[[partition]]::size`       | MiB                   | The size of the partition. If negative, subtracts from the remaining disk size.                                                                                                                                                                                                                             |
| `::[image]::[[partition]]::requires`   | [component::name]     | The list of components which must be built before the partition can be assembled. Component builds are parallelised where possible, so build-order is not guaranteed.                                                                                                                                       |
| `::[image]::[[partition]]::filesystem` | filesystem (optional) | Whether the partition should be formatted with a filesystem. If defined, the filesystem will be automatically mounted. If the user does not have superuser access or the `--fuse` argument is provided, the filesystem will be mounted with [`FUSE`](https://en.wikipedia.org/wiki/Filesystem_in_Userspace) |

### `::image::partition::file`

A resource which will be written to the filesystem.

| Key                                            | Type       | Description                                                                                |
|------------------------------------------------|------------|--------------------------------------------------------------------------------------------|
| `::[image]::[[partition]]::[[file]]::path`     | path       | Where the file is to be placed                                                             |
| `::[image]::[[partition]]::[[file]]::text`     | string     | The contents of the file. Mutually exclusive to `#::shell`, `#::symlink` and `#::artifact` |
| `::[image]::[[partition]]::[[file]]::symlink`  | path       | A file to symlink                                                                          |
| `::[image]::[[partition]]::[[file]]::artifact` | [artifact] | An artifact. Artifacts use the above-described naming convention                           |
| `::[image]::[[partition]]::[[file]]::shell`    | shell      | A [shell script](#shell). Only the `stdout` of the shell process is written to the file    |

## `::filesystem`

| Key | Type | Description |
|-----|------|-------------|

## Shell

A shell script is any [`NuShell`](https://www.nushell.sh/) script which produces a value. Depending on the requirements
of the script, you may be required to define a set of functions. NuShell supports function visibility, and you are
encouraged to encapsulate your functionality as you see fit.

For example, you may wish to use a non-standard filesystem. In this case, you will need to provide a well-known list of
methods which can be called to mount, stat and unmount your filesystem.

```nu
// Using [ext4fuse](https://github.com/gerard/ext4fuse.git)

def mount [source: path, dest: path] { sudo -A (kdialog --password "Elevation required to mount custom ext4 filesystem") ext4fuse $source $dest }
def umount [_source: path, dest: path] { sudo -A (kdialog --password "Elevation requried to unmount filesystem") umount $dest }
def stat [source: path] { {
    size: 1024 * 1024 * 1024 * 12 // 12 MiB
    free: 1024 * 1024 * 1024 * 4.5 // 4.5 MiB free
    ...
} | to json }
```

The best reference are the [docs themselves](https://www.nushell.sh/book/scripts.html).

## Caching

Artifacts (**not components**) are cached based on a caching rule.

| Caching Mode | Purpose                                                                                                                                                                                               |
|--------------|-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `aggressive` | Cached items persevere as long as possible. These can be cleared by a) explicitly requesting this item's cache be cleared or b) by performing a clean build.                                          |
| `normal`     | Cached items obey regular preservation rules. Components will re-fetch their sources if no prebuilt or existing build of the component exists.                                                        |
| `transient`  | This component holds caches for the lifetime of the build only. Component source will be re-fetched at each build. Useful for automatically incrementing version numbers or including CI results etc. |

You can of course explicitly request a component's cache to be invalidated using the `cache` subcommand.

```shell
$ builder cache remove <component>
```

Unless the cache mode is `transient`, if a component is specified identically in another configuration and a cache of it
exists, it will be reused.

## URL Types

The following URL schemes are understood:

* `http` / `https`: Makes a web request.
* `git`: Fetches a git repository. Accepts commit hashes, branches etc
* `file`: Uses a file or directory on the local device
* `art`: Sets a dependency on a component in the current configuration. Must follow the
  pattern `art://[component]::[artifact]`