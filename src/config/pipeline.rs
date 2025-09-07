use crate::zones::PipelineZone;
use crate::config::{
    Frame, Filter, Packages, Repos, Steps,
    ValidatedPackages, ValidatedRepos, ValidatedSteps, Value,
};
use anyhow::{Result, anyhow};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{self, Write},
    path::PathBuf,
};
use tokio::time::Duration;
use rand::{thread_rng, Rng};
use rand::distributions::Alphanumeric;

#[derive(Debug)]
pub struct Pipeline {
    pub name: Value<String>,
    pub repos: Repos,
    pub packages: Packages,
    pub steps: Steps,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatedPipeline {
    #[serde(rename = "@name")]
    pub name: String,

    pub repos: ValidatedRepos,
    pub packages: ValidatedPackages,
    pub steps: ValidatedSteps,
}

impl Pipeline {
    pub fn new(name: &String) -> Pipeline {
        Pipeline {
            name: Value::Set(name.clone()),
            repos: Repos::new(),
            packages: Packages::new(),
            steps: Steps::new(),
        }
    }

    pub fn name(&self) -> String {
        "pipeline".to_string()
    }

    pub fn load_or_create(name: &String) -> Result<Self> {
        match ValidatedPipeline::load(name) {
            Ok(Some(vpipeline)) => Ok(vpipeline.as_pipeline()),
            Ok(None) => Ok(Pipeline::new(name)),
            Err(err) => panic!("{:?}", err),
        }
    }

    pub fn validate(&self) -> Result<ValidatedPipeline> {
        let name = match &self.name {
            Value::Unset => Err(anyhow!("name is unset")),
            Value::Set(name) => Ok(name),
        }?
        .clone();
        let repos = self.repos.validate()?;
        let packages = self.packages.validate()?;
        let steps = self.steps.validate()?;

        Ok(ValidatedPipeline {
            name,
            repos,
            packages,
            steps,
        })
    }

    pub fn set(&mut self, key: String, value: String) -> Result<()> {
        match key.as_str() {
            "name" => {
                self.name = Value::Set(value);
                Ok(())
            }
            _ => Err(anyhow!("Unknown key: {}", key)),
        }
    }

    pub fn select(&self, ty: String, filter: Option<Filter>) -> Result<Frame> {
        match ty.as_str() {
            "package" => self.packages.select(&filter),
            "repo" => self.repos.select(&filter),
            "step" => self.steps.select(&filter),
            _ => unreachable!(),
        }
    }
}

impl ValidatedPipeline {
    pub async fn apply(&self) -> Result<()> {
        println!("Applying pipeline {}", self.name.cyan());
        let base_pzone = self.base_pzone();

        self.ensure_dataset_exists().await?;
        self.ensure_zone_exists(&base_pzone).await?;
        self.install_packages(&base_pzone).await?;
        self.clone_repos(&base_pzone).await?;
        self.execute_steps(&base_pzone).await?;
        self.halt_zone(&base_pzone).await?;

        println!("Pipeline {} created", self.name.cyan());
        Ok(())
    }

    pub async fn run(&self) -> Result<()> {
        let run_id = self.generate_run_id();
        println!("Starting run {}", run_id.cyan());
        let base_pzone = self.base_pzone();
        let run_pzone = base_pzone.get_run_pzone(&run_id);

        crate::zones::create_zone_from_base(&run_pzone, &base_pzone).await?;
        self.execute_steps(&run_pzone).await?;

        run_pzone.cleanup()?;
        run_pzone.delete()
    }

    pub async fn execute_steps(&self, pzone: &PipelineZone) -> Result<()> {
        self.steps.as_runnable().run(pzone).await
    }

    pub fn load(name: &String) -> Result<Option<Self>> {
        let pipeline_path = Self::file_path(name);

        match File::options().read(true).write(true).open(pipeline_path) {
            Ok(file) => Ok(Some(serde_xml_rs::from_reader(file)?)),
            Err(err) => {
                // TODO(Marce): Handle error kinds
                Ok(None)
            }
        }
    }

    pub fn generate_run_id(&self) -> String {
        thread_rng()
            .sample_iter(&Alphanumeric)
            .take(4)
            .map(char::from)
            .collect::<String>()
            .to_lowercase()
    }

    pub fn base_pzone(&self) -> PipelineZone {
        PipelineZone {
            pipeline: self.name.clone(),
            zone_type: crate::zones::ZoneType::Base,
        }
    }

    pub fn file_path(name: &String) -> PathBuf {
        let filename = format!("{}.xml", name);
        ["/etc", "pipelines", filename.as_str()].iter().collect()
    }

    pub fn save(&self) -> Result<()> {
        let pipeline_path = Self::file_path(&self.name);

        match File::options().write(true).create(true).open(pipeline_path) {
            Ok(file) => Ok(serde_xml_rs::to_writer(file, self)?),
            Err(err) => {
                // TODO(Marce): Handle it more gracefully
                panic!("COULDNT WRITE");
            }
        }
    }

    pub fn as_pipeline(&self) -> Pipeline {
        Pipeline {
            name: Value::Set(self.name.clone()),
            packages: self.packages.as_packages(),
            repos: self.repos.as_repos(),
            steps: self.steps.as_steps(),
        }
    }

    pub fn vnic_name(&self) -> String {
        format!("{}_internal0", self.zone_name())
    }

    pub async fn ensure_dataset_exists(&self) -> Result<()> {
        print!("Creating ZFS dataset at {}...", self.dataset().cyan());
        io::stdout().lock().flush().unwrap();

        if crate::zfs::base_dataset_exists(&self.dataset()).await? {
            println!("{}", "ALREADYEXISTS".yellow());
        } else {
            crate::zfs::create_dataset(&self.dataset()).await?;
            println!("{}", "DONE".green());
        }

        Ok(())
    }

    pub async fn halt_zone(&self, pzone: &PipelineZone) -> Result<()> {
        pzone.halt()
    }

    pub async fn ensure_zone_exists(&self, pzone: &PipelineZone) -> Result<()> {
        pzone.cleanup()?;

        print!("Creating VNIC {}...", self.vnic_name().cyan());
        io::stdout().lock().flush().unwrap();
        crate::dladm::ensure_nic_exists(&self.vnic_name()).await?;
        println!("{}", "DONE".green());

        print!("Configuring zone...");
        io::stdout().lock().flush().unwrap();
        crate::zones::configure_zone_with_default_config(&pzone).await?;
        println!("{}", "DONE".green());

        print!("Installing zone...");
        io::stdout().lock().flush().unwrap();
        zone::Adm::new(self.zone_name()).install_blocking(&[])?;
        println!("{}", "DONE".green());

        print!("Booting zone...");
        io::stdout().lock().flush().unwrap();
        zone::Adm::new(self.zone_name()).boot_blocking()?;
        println!("{}", "DONE".green());

        tokio::time::sleep(Duration::new(30, 0)).await;

        // Setup network access
        crate::zones::configure_zone_networking(&pzone).await?;

        Ok(())
    }

    pub async fn clone_repos(&self, pzone: &PipelineZone) -> Result<()> {
        self.repos.clone(pzone).await
    }

    pub async fn install_packages(&self, pzone: &PipelineZone) -> Result<()> {
        self.packages.install(&pzone).await
    }

    pub fn path(&self) -> String {
        format!("/zones/ci/{}/base", self.name)
    }

    pub fn dataset(&self) -> String {
        format!("rpool{}", self.path())
    }

    pub fn zone_name(&self) -> String {
        format!("ci_{}_base", self.name)
    }
}
