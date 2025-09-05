use crate::zones::PipelineZone;
use crate::config::{Filter, Frame, Value};
use crate::filterable::Filterable;
use anyhow::{Result, anyhow};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc};
use std::io::{self, Write};

#[derive(Debug)]
pub struct Packages {
    vec: Vec<Rc<RefCell<Package>>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatedPackages {
    #[serde(rename = "package")]
    vec: Vec<ValidatedPackage>,
}

impl Packages {
    pub fn new() -> Self {
        Self { vec: Vec::new() }
    }

    pub fn add_empty(&mut self) -> Frame {
        let p = Rc::new(RefCell::new(Package::default()));
        self.vec.push(p.clone());
        Frame::Package(p.clone())
    }

    pub fn select(&self, filter: &Option<Filter>) -> Result<Frame> {
        let matching: Vec<_> = self
            .vec
            .iter()
            .filter(|f| f.borrow().filter(filter))
            .collect();

        match matching[..] {
            [x] => Ok(Frame::Package(x.clone())),
            [] => Err(anyhow!("No element matched the filter")),
            _ => Err(anyhow!("More than one element matched the filter")),
        }
    }

    pub fn validate(&self) -> Result<ValidatedPackages> {
        let vpacks = self
            .vec
            .iter()
            .map(|r| r.borrow().validate())
            .collect::<Result<Vec<ValidatedPackage>>>()?;

        Ok(ValidatedPackages { vec: vpacks })
    }
}

impl ValidatedPackages {
    pub fn as_packages(&self) -> Packages {
        let packs = self
            .vec
            .iter()
            .map(|p| Rc::new(RefCell::new(p.as_package())))
            .collect();

        Packages { vec: packs }
    }

    pub async fn install(&self, pzone: &PipelineZone) -> Result<()> {
        print!(
            "Installing packages ({}) This may take a while...",
            "rust".yellow()
        );
        io::stdout().lock().flush().unwrap();
        pzone.exec("pkg install git")?.wait().await?;
        pzone.exec("pkgin -y install rust")?.wait().await?;
        println!("{}", "DONE".green());
        Ok(())
    }
}

#[derive(Debug)]
#[repr(u8)]
enum Provider {
    Pkg,
    PkgSrc,
}

#[derive(Debug, Default)]
pub struct Package {
    pub provider: Value<String>,
    pub name: Value<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatedPackage {
    #[serde(rename = "@provider")]
    pub provider: String,
    #[serde(rename = "@name")]
    pub name: String,
}

impl Package {
    pub fn validate(&self) -> Result<ValidatedPackage> {
        let name = match &self.name {
            Value::Unset => Err(anyhow!("name is unset")),
            Value::Set(name) => Ok(name),
        }?;
        let provider = match &self.provider {
            Value::Unset => Err(anyhow!("provider is unset")),
            Value::Set(provider) => Ok(provider),
        }?;

        Ok(ValidatedPackage {
            name: name.clone(),
            provider: provider.clone(),
        })
    }

    pub fn name(&self) -> String {
        match &self.name {
            Value::Unset => "package".to_string(),
            Value::Set(v) => format!("package({})", v.cyan()),
        }
    }

    pub fn set(&mut self, key: String, value: String) -> Result<()> {
        match key.as_str() {
            "name" => {
                self.name = Value::Set(value);
                Ok(())
            }
            "provider" => {
                self.provider = Value::Set(value);
                Ok(())
            }
            _ => Err(anyhow!("Unknown attribute for package: {}", key)),
        }
    }
}

impl ValidatedPackage {
    pub fn as_package(&self) -> Package {
        Package {
            provider: Value::Set(self.provider.clone()),
            name: Value::Set(self.name.clone()),
        }
    }
}
