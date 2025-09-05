use crate::config::{Filter, Frame, Value};
use crate::filterable::Filterable;
use crate::zones::PipelineZone;
use anyhow::{Result, anyhow};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, rc::Rc};
use std::io::{self, Write};

#[derive(Debug)]
pub struct Repos {
    vec: Vec<Rc<RefCell<Repo>>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatedRepos {
    #[serde(rename = "repo")]
    vec: Vec<ValidatedRepo>,
}

impl Repos {
    pub fn new() -> Repos {
        Self { vec: Vec::new() }
    }

    pub fn add_empty(&mut self) -> Frame {
        let r = Rc::new(RefCell::new(Repo::default()));
        self.vec.push(r.clone());
        Frame::Repo(r.clone())
    }

    pub fn select(&self, filter: &Option<Filter>) -> Result<Frame> {
        let matching: Vec<_> = self
            .vec
            .iter()
            .filter(|f| f.borrow().filter(filter))
            .collect();

        match matching[..] {
            [x] => Ok(Frame::Repo(x.clone())),
            [] => Err(anyhow!("No element matched the filter")),
            _ => Err(anyhow!("More than one element matched the filter")),
        }
    }

    pub fn validate(&self) -> Result<ValidatedRepos> {
        let vrepos = self
            .vec
            .iter()
            .map(|r| r.borrow().validate())
            .collect::<Result<Vec<ValidatedRepo>>>()?;
        Ok(ValidatedRepos { vec: vrepos })
    }
}

impl ValidatedRepos {
    pub fn as_repos(&self) -> Repos {
        let repos = self
            .vec
            .iter()
            .map(|p| Rc::new(RefCell::new(p.as_repo())))
            .collect();
        Repos { vec: repos }
    }

    pub async fn clone(&self, pzone: &PipelineZone) -> Result<()> {
        for repo in self.vec.iter() {
            print!("Cloning repo {}...", repo.url.yellow());
            io::stdout().lock().flush().unwrap();
            pzone.exec(format!("git clone {}", repo.url))?.wait().await?;
            println!("{}", "DONE".green());
        }

        Ok(())
    }

    pub fn pull(&self, pzone: &PipelineZone) -> Result<()> {
        // TODO(Marce)
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &ValidatedRepo> {
        self.vec.iter()
    }
}

#[derive(Debug, Default)]
pub struct Repo {
    pub url: Value<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatedRepo {
    #[serde(rename = "@url")]
    pub url: String,
}

impl Repo {
    pub fn validate(&self) -> Result<ValidatedRepo> {
        let url = match &self.url {
            Value::Unset => Err(anyhow!("url is unset")),
            Value::Set(url) => Ok(url),
        }?;

        Ok(ValidatedRepo { url: url.clone() })
    }

    pub fn name(&self) -> String {
        match &self.url {
            Value::Unset => "repo".to_string(),
            Value::Set(v) => format!("repo({})", v.cyan()),
        }
    }

    pub fn set(&mut self, key: String, value: String) -> Result<()> {
        match key.as_str() {
            "url" => {
                self.url = Value::Set(value);
                Ok(())
            }
            _ => Err(anyhow!("Unknown attribute for repo: {}", key)),
        }
    }
}

impl ValidatedRepo {
    pub fn as_repo(&self) -> Repo {
        Repo {
            url: Value::Set(self.url.clone()),
        }
    }
}
