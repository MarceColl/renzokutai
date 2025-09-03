use anyhow::{anyhow, Result};
use std::process::{Stdio};

pub async fn base_dataset_exists(name: &String) -> Result<bool> {
    let status = tokio::process::Command::new("zfs")
        .arg("list")
        .arg("-H")
        .arg("-o")
        .arg("name")
        .arg("-r")
        .arg(name.as_str())
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .await?;

    Ok(status.success())
}

pub async fn create_dataset(name: &String) -> Result<()> {
    let status = tokio::process::Command::new("zfs")
        .arg("create")
        .arg("-p")
        .arg(name.as_str())
        .stderr(Stdio::null())
        .status()
        .await?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("Couldn't create dataset"))
    }
}
