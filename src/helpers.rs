use std::{collections::HashMap, fs, io::Read, path::Path};

use anyhow::{anyhow, Context};
use monitor_client::{futures_util::future::join_all, MonitorClient};
use tracing::info;

use crate::{Action, CredsFile, MonitorFile, Stage};

pub fn parse_monitor_file(path: impl AsRef<Path>) -> anyhow::Result<MonitorFile> {
  let contents = fs::read_to_string(path).context("failed to read file contents")?;
  toml::from_str(&contents).context("failed to parse toml contents into execution stages")
}

pub fn parse_creds_file(path: impl AsRef<Path>) -> anyhow::Result<CredsFile> {
  let contents = fs::read_to_string(path).context("failed to read file contents")?;
  toml::from_str(&contents)
    .context("failed to parse toml contents into monitor url and credentials")
}

pub fn wait_for_enter() -> anyhow::Result<()> {
  println!("\nPress ENTER to RUN");
  let buffer = &mut [0u8];
  std::io::stdin()
    .read_exact(buffer)
    .context("failed to read ENTER")?;
  Ok(())
}

pub async fn run_stages(client: &MonitorClient, stages: Vec<Stage>) -> anyhow::Result<()> {
  // info!("running monitor file: {name}");
  let build_map = build_name_to_id_map(client).await?;
  let deployment_map = deployment_name_to_id_map(client).await?;
  for Stage {
    name,
    action,
    targets,
  } in stages
  {
    info!("running {action} stage: {name}... ⏳");
    let targets = match action {
      Action::Build => names_to_ids(&targets, &build_map)?,
      _ => names_to_ids(&targets, &deployment_map)?,
    };
    match action {
      Action::Build => {
        trigger_builds_in_parallel(client, &targets).await?;
      }
      Action::Deploy => {
        redeploy_deployments_in_parallel(client, &targets).await?;
      }
      Action::StartContainer => start_containers_in_parallel(client, &targets).await?,
      Action::StopContainer => stop_containers_in_parallel(client, &targets).await?,
      Action::DestroyContainer => {
        destroy_containers_in_parallel(client, &targets).await?;
      }
    }
    info!("finished {action} stage: {name} ✅");
  }
  Ok(())
}

pub async fn redeploy_deployments_in_parallel(
  client: &MonitorClient,
  deployment_ids: &[String],
) -> anyhow::Result<()> {
  let futes = deployment_ids.iter().map(|id| async move {
    client
      .deploy_container(id)
      .await
      .with_context(|| format!("failed to deploy {id}"))
      .and_then(|update| {
        if update.success {
          Ok(())
        } else {
          Err(anyhow!(
            "failed to deploy {id}. operation unsuccessful, see monitor update"
          ))
        }
      })
  });
  join_all(futes).await.into_iter().collect()
}

pub async fn start_containers_in_parallel(
  client: &MonitorClient,
  deployment_ids: &[String],
) -> anyhow::Result<()> {
  let futes = deployment_ids.iter().map(|id| async move {
    client
      .start_container(id)
      .await
      .with_context(|| format!("failed to start container {id}"))
      .and_then(|update| {
        if update.success {
          Ok(())
        } else {
          Err(anyhow!(
            "failed to start container {id}. operation unsuccessful, see monitor update"
          ))
        }
      })
  });
  join_all(futes).await.into_iter().collect()
}

pub async fn stop_containers_in_parallel(
  client: &MonitorClient,
  deployment_ids: &[String],
) -> anyhow::Result<()> {
  let futes = deployment_ids.iter().map(|id| async move {
    client
      .stop_container(id)
      .await
      .with_context(|| format!("failed to stop container {id}"))
      .and_then(|update| {
        if update.success {
          Ok(())
        } else {
          Err(anyhow!(
            "failed to stop container {id}. operation unsuccessful, see monitor update"
          ))
        }
      })
  });
  join_all(futes).await.into_iter().collect()
}

pub async fn destroy_containers_in_parallel(
  client: &MonitorClient,
  deployment_ids: &[String],
) -> anyhow::Result<()> {
  let futes = deployment_ids.iter().map(|id| async move {
    client
      .remove_container(id)
      .await
      .with_context(|| format!("failed to destroy container {id}"))
      .and_then(|update| {
        if update.success {
          Ok(())
        } else {
          Err(anyhow!(
            "failed to destroy container {id}. operation unsuccessful, see monitor update"
          ))
        }
      })
  });
  join_all(futes).await.into_iter().collect()
}

pub async fn trigger_builds_in_parallel(
  client: &MonitorClient,
  build_ids: &[String],
) -> anyhow::Result<()> {
  let futes = build_ids.iter().map(|id| async move {
    client
      .build(id)
      .await
      .with_context(|| format!("failed to build {id}"))
      .and_then(|update| {
        if update.success {
          Ok(())
        } else {
          Err(anyhow!(
            "failed to build {id}. operation unsuccessful, see monitor update"
          ))
        }
      })
  });
  join_all(futes).await.into_iter().collect()
}

pub async fn deployment_name_to_id_map(
  client: &MonitorClient,
) -> anyhow::Result<HashMap<String, String>> {
  let deployment_name_to_id_map = client
    .list_deployments(None)
    .await?
    .into_iter()
    .map(|d| (d.deployment.name, d.deployment.id))
    .collect::<HashMap<_, _>>();
  Ok(deployment_name_to_id_map)
}

#[allow(unused)]
pub async fn server_name_to_id_map(
  client: &MonitorClient,
) -> anyhow::Result<HashMap<String, String>> {
  let server_name_to_id_map = client
    .list_servers(None)
    .await?
    .into_iter()
    .map(|s| (s.server.name, s.server.id))
    .collect::<HashMap<_, _>>();
  Ok(server_name_to_id_map)
}

pub async fn build_name_to_id_map(
  client: &MonitorClient,
) -> anyhow::Result<HashMap<String, String>> {
  let build_name_to_id_map = client
    .list_builds(None)
    .await?
    .into_iter()
    .map(|build| (build.name, build.id))
    .collect::<HashMap<_, _>>();
  Ok(build_name_to_id_map)
}

pub fn names_to_ids(
  names: &[String],
  name_to_id_map: &HashMap<String, String>,
) -> anyhow::Result<Vec<String>> {
  names
    .iter()
    .map(|name| {
      name_to_id_map
        .get(name)
        .cloned()
        .with_context(|| format!("no id found for name {name}"))
    })
    .collect()
}
