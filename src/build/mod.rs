pub mod builder;
pub mod config;
pub mod cx;
pub mod image;

use crate::build::builder::{build_partition, ArtifactList};
use crate::build::config::ImportableModule;
use crate::build::config::{Component, ConfigFile};
use crate::build::cx::mk_context;
use crate::error::*;
use log::debug;
use log::info;
use serde::de::DeserializeOwned;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use tokio::fs;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct DependencyTree {
    pub dependencies: Box<[Arc<RwLock<DependencyTree>>]>,
    pub sources: Box<[PathBuf]>,
    pub component: Component,
    pub status: BuildStatus,
}

#[derive(Debug, Clone)]
pub enum BuildStatus {
    NotStarted,
    InProgress,
    Success(ArtifactList),
    Failure,
}

async fn read_toml_file<T: DeserializeOwned, FilePath: AsRef<Path>>(path: FilePath) -> Result<T> {
    debug!("Reading file: {:?}", path.as_ref());
    let file = fs::read_to_string(&path).await?;
    match toml::from_str::<T>(&file) {
        Ok(file) => Ok(file),
        Err(err) => {
            if let Some(span) = err.span() {
                eprintln!("An error occurred: '{}'\n", &file[span]);
            }

            return Err(Error::from(err));
        }
    }
}

pub async fn build(config_path: PathBuf, clean: bool, build_dir: Option<PathBuf>) -> Result<()> {
    let mut config: ConfigFile = read_toml_file(&config_path).await?;
    info!("Beginning build '{}'", &config.name);

    debug!("resolving imports");
    for i in &config.requires {
        let r#mod: ImportableModule = read_toml_file(match i.is_absolute() {
            true => i.clone(),
            false => config_path
                .parent()
                .map(|i| i.to_path_buf())
                .unwrap_or(config_path.clone())
                .join(i)
                .with_extension("toml"),
        })
        .await?;

        config.components.extend(r#mod.components);
    }

    debug!("Preparing Environment");
    let mut cx = mk_context(&config, build_dir.as_ref()).await?;

    {
        let mut check_duplicates = HashSet::<String>::new();
        for i in &config.components {
            if check_duplicates.contains(&i.name) {
                return Err(BuildError::DuplicateComponentName(i.name.clone()).into());
            } else {
                check_duplicates.insert(i.name.clone());
            }
        }
    }

    debug!("Building Dependency Graph");
    let dependency_graph = build_dependency_graph(&config)?;

    let items = futures::future::join_all(
        config
            .image
            .partitions
            .iter()
            .map(|i| Arc::new(i.clone()))
            .map(|partition| {
                tokio::spawn(build_partition(
                    Arc::clone(&partition),
                    partition
                        .requires
                        .iter()
                        .filter_map(|i| match dependency_graph.get(i) {
                            Some(component) => Some((i.clone(), Arc::clone(component))),
                            None => None,
                        })
                        .collect(),
                ))
            }),
    )
    .await; // TODO: check for errors

    info!("All partitions built");

    Ok(())
}

fn build_dependency_graph(
    config: &ConfigFile,
) -> Result<HashMap<String, Arc<RwLock<DependencyTree>>>> {
    let mut dependency_graph = HashMap::new();
    let mut all_components = HashMap::<String, Weak<RwLock<DependencyTree>>>::new();

    // Gets called for each direct or indirect dependency of any partition
    // Aim: To build a recursive structure where the top-level items are only the direct dependencies of the partitions.
    //      Each component with a dependency on another should contain it within itself, forming the recursive structure
    fn build_step(
        all_components: &mut HashMap<String, Weak<RwLock<DependencyTree>>>,
        config: &ConfigFile,
        component: &Component,
    ) -> Result<Arc<RwLock<DependencyTree>>> {
        if let Some(dep) = all_components.get(&component.name) {
            Ok(dep.upgrade().ok_or(BuildError::ReferenceDropped)?)
        } else {
            let mut sources = Vec::<String>::new();
            let mut dependencies = Vec::<&Component>::new();

            for (i, component) in component
                .requires
                .iter()
                .map(|i| (i, config.components.iter().find(|j| j.name.eq(i))))
            {
                match component {
                    Some(component) => dependencies.push(component),
                    None => sources.push(i.clone()),
                }
            }

            let dep = Arc::new(RwLock::new(DependencyTree {
                status: BuildStatus::NotStarted,
                component: component.clone(),
                sources: sources.into_iter().map(PathBuf::from).collect(),
                dependencies: dependencies
                    .into_iter()
                    .map(|i| build_step(all_components, config, i))
                    .collect::<Result<_>>()?,
            }));

            all_components.insert(component.name.clone(), Arc::downgrade(&dep));

            Ok(dep)
        }
    }

    for i in config.image.partitions.iter().flat_map(|i| {
        i.requires
            .iter()
            .map(|i| config.components.iter().find(|j| j.name.eq(i)))
    }) {
        if let Some(comp) = i {
            dependency_graph.insert(
                comp.name.clone(),
                build_step(&mut all_components, config, comp)?,
            );
        }
    }

    return Ok(dependency_graph);
}
