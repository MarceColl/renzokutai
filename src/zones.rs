use anyhow::Result;
use owo_colors::OwoColorize;
use std::ffi::OsStr;
use std::io;
use std::io::Write;

#[derive(Debug, Clone)]
pub enum ZoneType {
    Base,
    Run(String),
}

impl ZoneType {
    pub fn id(&self) -> String {
        match self {
            ZoneType::Base => "base".to_string(),
            ZoneType::Run(id) => id.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PipelineZone {
    pub pipeline: String,
    pub zone_type: ZoneType,
}

impl PipelineZone {
    pub fn root_path(&self) -> String {
        format!("/zones/ci/{}", self.pipeline)
    }

    pub fn path(&self) -> String {
        format!("{}/{}", self.root_path(), self.zone_type.id())
    }

    pub fn name(&self) -> String {
        format!("ci_{}_{}", self.pipeline, self.zone_type.id())
    }

    pub fn vnic_name(&self) -> String {
        format!("{}_internal0", self.name())
    }

    pub fn get_run_pzone(&self) -> Self {
        PipelineZone {
            pipeline: self.pipeline.clone(),
            zone_type: ZoneType::Run("a9skl10".to_string()),
        }
    }

    pub fn exec(&self, command: impl AsRef<OsStr>) -> Result<tokio::process::Child> {
        let command = zone::Zlogin::new(self.name()).as_command(command);

        Ok(tokio::process::Command::from(command)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?)
    }

    pub fn cleanup(&self) -> Result<()> {
        if let Some(mut state) = get_zone_state(&self)? {
            print!(
                "Zone {} already exists in state {:?}...",
                self.name().cyan(),
                state
            );
            io::stdout().lock().flush().unwrap();

            if state == zone::State::Running {
                zone::Adm::new(self.name()).halt_blocking()?;
                state = zone::State::Installed;
                print!("{}", " HALTED".yellow());
                io::stdout().lock().flush().unwrap();
            }

            if state == zone::State::Installed {
                zone::Adm::new(self.name()).uninstall_blocking(true)?;
                print!("{}", " UNINSTALLED".yellow());
                io::stdout().lock().flush().unwrap();
            }

            println!(" {}", "DONE".green());
        }

        Ok(())
    }

    pub fn delete(self) -> Result<()> {
        print!("Deleting {:?}...", self.name().cyan());
        io::stdout().lock().flush().unwrap();

        zone::Config::new(self.name()).delete(true).run_blocking()?;
        println!(" {}", "DONE".green());

        Ok(())
    }
}

fn get_zone_state(pzone: &PipelineZone) -> Result<Option<zone::State>> {
    Ok(match get_zone(pzone)? {
        Some(z) => Some(z.state),
        None => None,
    })
}

fn get_zone(pzone: &PipelineZone) -> Result<Option<zone::Zone>> {
    Ok(list()?.into_iter().find(|z| z.name == pzone.name()))
}

fn list() -> Result<Vec<zone::Zone>> {
    Ok(zone::Adm::list_blocking()?)
}

pub async fn create_zone_from_base(base_pzone: &PipelineZone) -> Result<PipelineZone> {
    let run_pzone = base_pzone.get_run_pzone();

    print!("Creating VNIC {}...", run_pzone.vnic_name().cyan());
    io::stdout().lock().flush().unwrap();
    crate::dladm::ensure_nic_exists(&run_pzone.vnic_name()).await?;
    println!("{}", "DONE".green());

    print!("Configuring zone {}...", run_pzone.name().cyan());
    io::stdout().lock().flush().unwrap();
    crate::zones::configure_zone_with_default_config(&run_pzone).await?;
    println!("{}", "DONE".green());

    print!("Cloning source zone {}...", base_pzone.name().cyan());
    io::stdout().lock().flush().unwrap();
    zone::Adm::new(run_pzone.name()).clone_blocking(base_pzone.name())?;
    println!("{}", "DONE".green());

    print!("Booting zone {}...", run_pzone.name().cyan());
    io::stdout().lock().flush().unwrap();
    zone::Adm::new(run_pzone.name()).boot_blocking()?;
    println!("{}", "DONE".green());

    Ok(run_pzone)
}

pub async fn configure_zone_with_default_config(pzone: &PipelineZone) -> Result<()> {
    let mut cfg = zone::Config::create(pzone.name(), true, zone::CreationOptions::Default);

    cfg.get_global()
        .set_path(pzone.path())
        .set_brand("pkgsrc")
        .set_autoboot(false);

    cfg.add_net(&zone::Net {
        physical: pzone.vnic_name(),
        ..Default::default()
    });

    cfg.add_attr(&zone::Attr {
        name: "resolvers".to_string(),
        value: zone::AttributeValue::String("8.8.8.8,8.8.4.4".to_string()),
    });

    cfg.run_blocking()?;

    Ok(())
}

pub async fn configure_zone_networking(pzone: &PipelineZone) -> Result<()> {
    let ip = "10.0.0.100";
    let command = format!("ipadm create-ip {}", pzone.vnic_name());
    let _status = tokio::process::Command::new("zlogin")
        .arg(pzone.name())
        .arg(command)
        .status()
        .await?;
    let command = format!(
        "ipadm create-addr -T static -a {} {}/v4",
        ip,
        pzone.vnic_name()
    );
    let _status = tokio::process::Command::new("zlogin")
        .arg(pzone.name())
        .arg(command)
        .status()
        .await?;
    let command = "route -p add default 10.0.0.1";
    let _status = tokio::process::Command::new("zlogin")
        .arg(pzone.name())
        .arg(command)
        .status()
        .await?;

    Ok(())
}
