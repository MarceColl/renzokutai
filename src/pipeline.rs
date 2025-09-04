use crate::step::{ValidatedDependency, ValidatedStep, ValidatedSteps};
use anyhow::Result;
use std::collections::HashMap;

pub async fn run_pipeline(source_pzone: &crate::zones::PipelineZone) -> Result<()> {
    let pzone = crate::zones::create_zone_from_base(source_pzone).await?;
    let mut steps_by_name = HashMap::new();
    steps_by_name.insert(
        "build".to_string(),
        ValidatedStep {
            name: "build".to_string(),
            script: "build.sh".to_string(),
            depends: Vec::new(),
        },
    );

    steps_by_name.insert(
        "test".to_string(),
        ValidatedStep {
            name: "test".to_string(),
            script: "test.sh".to_string(),
            // depends: vec![ValidatedDependency { name: "build".to_string() }],
            depends: Vec::new(),
        },
    );

    let steps = ValidatedSteps { steps_by_name };
    let mut rsteps = steps.as_runnable();
    match rsteps.run(&pzone).await {
        Ok(()) => (),
        Err(err) => println!("Error: {}", err),
    };

    pzone.cleanup();
    pzone.delete();

    Ok(())
}

pub async fn run_pipeline_pulls(_pzone: &crate::zones::PipelineZone) -> Result<()> {
    Ok(())
}

pub async fn run_pipeline_steps(_pzone: &crate::zones::PipelineZone) -> Result<()> {
    Ok(())
}
