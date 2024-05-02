use std::any::Any;

pub use global::Error;

macro_rules! multi_error {
    ($name:ident($($manual:ident),*); $($err:ident = $obj:ty);*) => {
        pub mod $name {
            use backtrace::Backtrace;

            #[derive(Debug)]
            pub enum Inner {
                $($err($obj),)*
                $($manual),*
            }

            impl std::fmt::Display for Inner { fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { std::fmt::Debug::fmt(self, f) } }
            impl std::error::Error for Inner {}

            $(impl From<$obj> for Inner { fn from(value: $obj) -> Self { Self::$err(value) } })*

            pub struct Error {
                inner: Inner,
                backtrace: Backtrace
            }

            impl Error {
                pub fn into_inner(self) -> Inner {
                    self.inner
                }
            }

            impl<Err> From<Err> for Error where Err: Into<Inner> {
                fn from(err: Err) -> Self {
                    Self {
                        inner: err.into(),
                        backtrace: Backtrace::new()
                    }
                }
            }

            impl std::error::Error for Error {}
            impl std::fmt::Display for Error {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { std::fmt::Debug::fmt(self, f) }
            }

            impl std::fmt::Debug for Error {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "{:?}\n", &self.inner)?;
                    match std::env::var("RUST_BACKTRACE").as_ref().map(|i| i.as_ref()) {
                        Ok("full") => write!(f, "{:#?}", self.backtrace),
                        Ok("1") => write!(f, "{:?}", self.backtrace),
                        _ => write!(f, ""),
                    }
                }
            }
        }
    }
}

multi_error! { global();
    BuildError = crate::error::BuildError;
    IoError = std::io::Error;
    TomlParseError = toml::de::Error;
    JsonError = serde_json::Error;
    JoinError = tokio::task::JoinError;
    Anyhow = anyhow::Error;
    Syscall = syscall::error::Error
}

pub type Result<T> = ::std::result::Result<T, global::Error>;

#[derive(Debug)]
pub enum BuildError {
    DuplicateComponentName(String),
    ReferenceDropped,
    // Happens when the upgrade of a weak pointer fails. Shouldn't ever come up, but handle it anyway
    InvalidBuildDir(std::path::PathBuf),
    FailedDependency(String),
    LoopError,
    FailedToCreateImage,
    #[cfg(feature = "qemu")]
    QmpQuitFail(Box<dyn Any + Send>),
    #[cfg(feature = "qemu")]
    QmpQuitWrite0,
    InvalidDiskType,
    InvalidPartitionName,
    FuseError(Box<dyn Any + Send>),
    UnrecognisedFilesystem(String),
    FailedToCreateFilesystem(String),
}

impl std::error::Error for BuildError {}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}
