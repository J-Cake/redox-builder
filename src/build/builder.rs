use crate::build::config::BuildMode;
use crate::build::config::Partition;
use crate::build::BuildStatus;
use crate::build::DependencyTree;
use crate::error::*;
use async_recursion::async_recursion;
use log::{debug, info};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::task::JoinError;

pub async fn build_partition<'a>(
    partition: Arc<Partition>,
    resolved_dependencies: HashMap<String, Arc<RwLock<DependencyTree>>>,
    // cx: &mut Context<'a>,
) -> Result<()> {
    info!("Building Partition {}", &partition.label);

    let components = futures::future::join_all(
        resolved_dependencies
            .values()
            .map(|component| tokio::spawn(build_component(Arc::clone(component)))),
    )
    .await;

    debug!("Built dependencies for partition '{}'", &partition.label);

    Ok(())
}

#[async_recursion]
pub async fn build_component(component: Arc<RwLock<DependencyTree>>) -> Result<ArtifactList> {
    loop {
        break match component
            .try_read()
            .map(|comp| comp.status.clone())
            .unwrap_or(BuildStatus::InProgress)
        {
            BuildStatus::NotStarted => {
                let mut component = component.write().await;
                component.status = BuildStatus::InProgress;

                let artifacts = match component.dependencies.is_empty() {
                    true => Default::default(),
                    false => futures::future::join_all(
                        component
                            .dependencies
                            .iter()
                            .map(|component| tokio::spawn(build_component(Arc::clone(component)))),
                    )
                    .await
                    .into_iter()
                    .collect::<std::result::Result<Result<Box<[ArtifactList]>>, JoinError>>()??,
                };

                let build = (match &component.component.build_mode {
                    BuildMode::Cargo(args) => Command::new("cargo")
                        .arg("build")
                        .args(args)
                        // TODO: Pass environment and set CWD
                        .spawn()?,
                    BuildMode::Shell(shell) => Command::new("nu")
                        .arg("-c")
                        .arg(shell)
                        // TODO: Pass environment and set CWD
                        .spawn()?,
                })
                .wait()
                .await?;

                component.status = BuildStatus::Success(ArtifactList {
                    component: component.component.name.clone(),
                    artifacts: Arc::new(Box::new([])),
                });

                continue;
            }
            BuildStatus::InProgress => continue,
            BuildStatus::Success(artifact_list) => Ok(artifact_list.clone()),
            BuildStatus::Failure => {
                Err(BuildError::FailedDependency(Arc::clone(&component)).into())
            }
        };
    }
}

#[derive(Debug, Clone)]
pub struct ArtifactList {
    pub component: String,
    pub artifacts: Arc<Box<[PathBuf]>>,
}
