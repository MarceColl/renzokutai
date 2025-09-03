use anyhow::{anyhow, Result};
use std::process::{Stdio};

pub async fn ensure_nic_exists(name: &String) -> Result<()> {
    if !nic_exists(name).await? {
        tokio::process::Command::new("dladm")
            .arg("create-vnic")
            .arg(name)
            .arg("-l")
            .arg("internal0")
            .stdout(Stdio::null())
            .status()
            .await?;
        Ok(())
    } else {
        Ok(())
    }
}

pub async fn nic_exists(name: &String) -> Result<bool> {
    let status = tokio::process::Command::new("dladm")
        .arg("show-vnic")
        .arg(name)
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .await?;

    Ok(status.success())
}
