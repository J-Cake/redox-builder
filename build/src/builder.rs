use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, RwLock};

use log::{debug, info};
use rayon::prelude::IntoParallelRefIterator;

use hub::config::BuildMode;
use hub::config::Partition;
use hub::error::*;

use crate::{BuildStatus, DependencyTree};

pub fn build_partition<'a>(
    partition: Arc<Partition>,
    resolved_dependencies: HashMap<String, Arc<RwLock<DependencyTree>>>,
    // cx: &mut Context<'a>,
) -> Result<()> {
    info!("Building Partition {}", &partition.label);

    // let components = futures::future::join_all(
    //     resolved_dependencies
    //         .values()
    //         .map(|component| tokio::spawn(build_component(Arc::clone(component)))),
    // )
    //     .await;

    // let components = resolved_dependencies
    //     .par_iter()
    //     .map(|(component, _)| build_component(Arc::clone(component)))
    //     .collect::<Result<Vec<_>>>();

    debug!("Built dependencies for partition '{}'", &partition.label);

    Ok(())
}

// pub fn build_component(component: Arc<RwLock<DependencyTree>>) -> Result<ArtifactList> {
//     loop {
//         break match component
//             .try_read()
//             .map(|comp| comp.status.clone())
//             .unwrap_or(BuildStatus::InProgress)
//         {
//             BuildStatus::NotStarted => {
//                 let mut component = component.write()?;
//                 component.status = BuildStatus::InProgress;
//
//                 let artifacts = match component.dependencies.is_empty() {
//                     true => Default::default(),
//                     false => futures::future::join_all(
//                         component
//                             .dependencies
//                             .iter()
//                             .map(|component| tokio::spawn(build_component(Arc::clone(component)))),
//                     )
//
//                         .into_iter()
//                         .collect::<std::result::Result<Result<Box<[ArtifactList]>>, JoinError>>()??,
//                 };
//
//                 let build = (match &component.component.build_mode {
//                     BuildMode::Cargo(args) => Command::new("cargo")
//                         .arg("build")
//                         .args(args)
//                         // TODO: Pass environment and set CWD
//                         .spawn()?,
//                     BuildMode::Shell(shell) => Command::new("nu")
//                         .arg("-c")
//                         .arg(shell)
//                         // TODO: Pass environment and set CWD
//                         .spawn()?,
//                 })
//                     .wait()
//                     ?;
//
//                 component.status = BuildStatus::Success(ArtifactList {
//                     component: component.component.name.clone(),
//                     artifacts: Arc::new(Box::new([])),
//                 });
//
//                 continue;
//             }
//             BuildStatus::InProgress => continue,
//             BuildStatus::Success(artifact_list) => Ok(artifact_list.clone()),
//             BuildStatus::Failure => {
//                 Err(BuildError::FailedDependency(component.read().component.name.clone()).into())
//             }
//         };
//     }
// }

#[derive(Debug, Clone)]
pub struct ArtifactList {
    pub component: String,
    pub artifacts: Arc<Box<[PathBuf]>>,
}
