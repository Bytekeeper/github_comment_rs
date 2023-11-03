use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

mod server;

/// This is the request from a client
#[derive(Deserialize, Debug)]
pub struct Post {
    path: String,
    message: String,
    name: String,
    url: String,
}

/// This will be serialized into a comment file on GitHub
#[derive(Serialize, Debug)]
struct Comment<'a> {
    id: &'a str,
    message: &'a str,
    name: &'a str,
    url: &'a str,
    date: u64,
}

#[derive(Deserialize)]
pub struct GitHubConfig {
    pub token: String,
    pub owner: String,
    pub repo: String,
}

pub static CONFIG: OnceLock<GitHubConfig> = OnceLock::new();

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let Ok(_) = CONFIG.set(
        serde_yaml::from_slice(&std::fs::read("config.yaml").context("Loading config file")?)
            .context("Parsing config file")?,
    ) else {
        panic!("Could not set config")
    };
    server::main()?;
    Ok(())
}
