use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use helpers::{parse_creds_file, parse_monitor_file};
use monitor_client::MonitorClient;
use serde::Deserialize;
use strum::Display;
use tracing::info;

use crate::helpers::{run_stages, wait_for_enter};

mod helpers;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct CliArgs {
  path: PathBuf,
  #[arg(default_value_t = String::from("./creds.toml"))]
  creds: String,
}

#[derive(Debug, Deserialize)]
pub struct CredsFile {
  pub url: String,
  pub username: String,
  pub secret: String,
}

#[derive(Debug, Deserialize)]
pub struct MonitorFile {
  pub name: String,
  pub stage: Vec<Stage>,
}

#[derive(Debug, Deserialize)]
pub struct Stage {
  pub name: String,
  pub action: Action,
  pub targets: Vec<String>,
}

#[derive(Debug, Deserialize, Display)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Action {
  Build,
  Deploy,
  StartContainer,
  StopContainer,
  DestroyContainer,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let CliArgs { path, creds } = CliArgs::parse();

  let CredsFile {
    url,
    username,
    secret,
  } = parse_creds_file(creds).context("failed to parse credentials file")?;

  let client = MonitorClient::new_with_secret(&url, username, secret)
    .await
    .context("failed to initialize client")?;

  let MonitorFile {
    name,
    stage: stages,
  } = parse_monitor_file(&path).context("failed to parse monitor file")?;

  info!("{name}");
  info!("path: {path:?}");
  println!("{stages:#?}");

  wait_for_enter()?;

  run_stages(&client, stages)
    .await
    .context("failed during a stage. terminating run.")?;

  info!("finished successfully ✅");

  Ok(())
}
