use std::io;
use std::io::Write;
use owo_colors::OwoColorize;
use anyhow::Result;

pub async fn run_pipeline(source_pzone: &crate::zones::PipelineZone) -> Result<()> {
    let pzone = crate::zones::create_zone_from_base(source_pzone).await?;
    run_pipeline_pulls(&pzone).await?;
    run_pipeline_steps(&pzone).await?;

    pzone.cleanup();
    pzone.delete();

    Ok(())
}

pub async fn run_pipeline_pulls(pzone: &crate::zones::PipelineZone) -> Result<()> {
    Ok(())
}

pub async fn run_pipeline_steps(pzone: &crate::zones::PipelineZone) -> Result<()> {
    Ok(())
}
