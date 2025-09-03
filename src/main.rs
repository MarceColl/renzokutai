use anyhow::{anyhow, Context, Result};
use std::io;
use std::rc::Rc;
use std::io::Write;
use std::cell::RefCell;
use std::time::Duration;
use serde::{Serialize,Deserialize};
use axum::{
    routing::{get, post},
    http::StatusCode,
    Json, Router,
};
use nom::{
    Parser,
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{char, multispace0, multispace1},
    combinator::{map, opt, rest},
    sequence::{preceded, separated_pair},
    IResult,
};
use owo_colors::OwoColorize;
use itertools::Itertools;

mod zfs;
mod dladm;
mod zones;
mod pipeline;

pub trait Filterable {
    fn filter(&self, filter: &Option<Filter>) -> bool {
        match filter {
            None => true,
            Some(filter) => self.inner_filter(filter),
        }
    }

    fn inner_filter(&self, filter: &Filter) -> bool;
}

#[derive(Debug,Default,PartialEq,Eq)]
enum Value<T> {
    #[default]
    Unset,
    Set(T),
}

#[derive(Debug)]
#[repr(u8)]
enum Provider {
    Pkg,
    PkgSrc,
}

#[derive(Debug,Default)]
struct Repo {
    url: Value<String>,
}

#[derive(Debug,Serialize)]
struct ValidatedRepo {
    #[serde(rename="@url")]
    url: String,
}

impl Filterable for Repo {
    fn inner_filter(&self, filter: &Filter) -> bool {
        match filter.key.as_str() {
            "url" => self.url == Value::Set(filter.value.clone()),
            _ => false
        }
    }
}

impl Repo {
    pub fn validate(&self) -> Result<ValidatedRepo> {
        let url= match &self.url {
            Value::Unset => Err(anyhow!("url is unset")),
            Value::Set(url) => Ok(url),
        }?;

        Ok(ValidatedRepo  {
            url: url.clone(),
        })
    }

    pub fn name(&self) -> String {
        match &self.url {
            Value::Unset => "repo".to_string(),
            Value::Set(v) => format!("repo({})", v.cyan()),
        }
    }

    pub fn set(&mut self, key: String, value: String) -> Result<()> {
        match key.as_str() {
            "url" => { self.url = Value::Set(value); Ok(()) },
            _ => Err(anyhow!("Unknown attribute for repo: {}", key)),
        }
    }
}

#[derive(Debug, Default)]
struct Package {
    provider: Value<String>,
    name: Value<String>,
}

#[derive(Debug, Serialize)]
struct ValidatedPackage {
    #[serde(rename="@provider")]
    provider: String,
    #[serde(rename="@name")]
    name: String,
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
            "name" => { self.name = Value::Set(value); Ok(()) },
            "provider" => { self.provider = Value::Set(value); Ok(()) },
            _ => Err(anyhow!("Unknown attribute for package: {}", key)),
        }
    }
}

impl Filterable for Package {
    fn inner_filter(&self, filter: &Filter) -> bool {
        match filter.key.as_str() {
            "name" => self.name == Value::Set(filter.value.clone()),
            "provider" => self.provider == Value::Set(filter.value.clone()),
            _ => false
        }
    }
}


#[derive(Debug, Default)]
struct Step {
    name: Value<String>,
    script: Value<String>,
    depends: Option<String>,
}

#[derive(Debug, Serialize)]
struct ValidatedStep {
    #[serde(rename="@name")]
    name: String,
    #[serde(rename="@script")]
    script: String,
    #[serde(rename="depend")]
    depends: Option<String>,
}

impl Step {
    pub fn validate(&self) -> Result<ValidatedStep> {
        let name = match &self.name {
            Value::Unset => Err(anyhow!("name is unset")),
            Value::Set(name) => Ok(name),
        }?;
        let script = match &self.script {
            Value::Unset => Err(anyhow!("script is unset")),
            Value::Set(script) => Ok(script),
        }?;
        Ok(ValidatedStep {
            name: name.clone(),
            script: script.clone(),
            depends: self.depends.clone(),
        })
    }

    pub fn name(&self) -> String {
        match &self.name {
            Value::Unset => "step".to_string(),
            Value::Set(v) => format!("step({})", v.cyan()),
        }
    }

    pub fn set(&mut self, key: String, value: String) -> Result<()> {
        match key.as_str() {
            "name" => { self.name = Value::Set(value); Ok(()) },
            "script" => { self.script = Value::Set(value); Ok(()) },
            "depends" => { self.depends = Some(value); Ok(()) },
            _ => Err(anyhow!("Unknown attribute for package: {}", key)),
        }
    }
}

impl Filterable for Step {
    fn inner_filter(&self, filter: &Filter) -> bool {
        match filter.key.as_str() {
            "name" => self.name == Value::Set(filter.value.clone()),
            "script" => self.script == Value::Set(filter.value.clone()),
            "depends" => self.depends == Some(filter.value.clone()),
            _ => false
        }
    }
}

#[derive(Debug)]
struct Pipeline {
    name: Value<String>,
    repos: Vec<Rc<RefCell<Repo>>>,
    packages: Vec<Rc<RefCell<Package>>>,
    steps: Vec<Rc<RefCell<Step>>>,
}

#[derive(Debug, Serialize)]
struct ValidatedPipeline {
    #[serde(rename="@name")]
    name: String,

    #[serde(rename="repo")]
    repos: Vec<ValidatedRepo>,

    #[serde(rename="package")]
    packages: Vec<ValidatedPackage>,

    #[serde(rename="steps")]
    steps: Vec<ValidatedStep>,
}

impl Pipeline {
    pub fn new() -> Pipeline {
        Pipeline {
            name: Value::Unset,
            repos: Vec::new(),
            packages: Vec::new(),
            steps: Vec::new(),
        }
    }

    pub fn name(&self) -> String {
        "pipeline".to_string()
    }

    pub fn validate(&self) -> Result<ValidatedPipeline> {
        let name = match &self.name {
            Value::Unset => Err(anyhow!("name is unset")),
            Value::Set(name) => Ok(name),
        }?.clone();
        let repos = self.repos.iter().map(|r| r.borrow().validate()).collect::<Result<Vec<ValidatedRepo>>>()?;
        let packages = self.packages.iter().map(|p| p.borrow().validate()).collect::<Result<Vec<ValidatedPackage>>>()?;
        let steps = self.steps.iter().map(|s| s.borrow().validate()).collect::<Result<Vec<ValidatedStep>>>()?;

        Ok(ValidatedPipeline { name, repos, packages, steps })
    }

    pub fn set(&mut self, key: String, value: String) -> Result<()> {
        match key.as_str() {
            "name" => {
                self.name = Value::Set(value);
                Ok(())
            },
            _ => Err(anyhow!("Unknown key: {}", key))
        }
    }

    pub fn select(&self, ty: String, filter: Option<Filter>) -> Result<Frame> {
        match ty.as_str() {
            "package" => {
                let matching: Vec<_> = self.packages
                    .iter()
                    .filter(|f| f.borrow().filter(&filter))
                    .collect();

                match matching[..] {
                    [x] => Ok(Frame::Package(x.clone())),
                    [] => Err(anyhow!("No element matched the filter")),
                    _ => Err(anyhow!("More than one element matched the filter")),
                }
            },
            "repo" => {
                let matching: Vec<_> = self.repos
                    .iter()
                    .filter(|f| f.borrow().filter(&filter))
                    .collect();

                match matching[..] {
                    [x] => Ok(Frame::Repo(x.clone())),
                    [] => Err(anyhow!("No element matched the filter")),
                    _ => Err(anyhow!("More than one element matched the filter")),
                }
            },
            "step" => {
                let matching: Vec<_> = self.steps
                    .iter()
                    .filter(|f| f.borrow().filter(&filter))
                    .collect();

                match matching[..] {
                    [x] => Ok(Frame::Step(x.clone())),
                    [] => Err(anyhow!("No element matched the filter")),
                    _ => Err(anyhow!("More than one element matched the filter")),
                }
            },
            _ => unreachable!(),
        }
    }
}

impl ValidatedPipeline {
    pub async fn apply(&self) -> Result<()> {
        println!("Applying pipeline {}", self.name.cyan());
        self.ensure_dataset_exists().await?;
        self.ensure_zone_exists().await?;
        self.install_packages()?;
        self.clone_repos().await?;
        self.execute_steps().await?;
        self.halt_zone().await?;

        println!("Pipeline {} created", self.name.cyan());
        Ok(())
    }

    pub fn vnic_name(&self) -> String {
        format!("{}_internal0", self.zone_name())
    }

    pub async fn ensure_dataset_exists(&self) -> Result<()> {
        print!("Creating ZFS dataset at {}...", self.dataset().cyan());
        io::stdout().lock().flush().unwrap();

        if zfs::base_dataset_exists(&self.dataset()).await? {
            println!("{}", "ALREADYEXISTS".yellow());
        } else {
            zfs::create_dataset(&self.dataset()).await?;
            println!("{}", "DONE".green());
        }

        Ok(())
    }

    pub async fn execute_steps(&self) -> Result<()> {
        for step in self.steps.iter() {
            println!("Running {} step", step.name.yellow());
            // TODO(Marce): Run step
        }

        Ok(())
    }

    pub async fn halt_zone(&self) -> Result<()> {
        zone::Adm::new(self.zone_name())
            .halt_blocking()?;

        Ok(())
    }

    pub async fn ensure_zone_exists(&self) -> Result<()> {
        let zones = zone::Adm::list_blocking()?;
        let pzone = crate::zones::PipelineZone {
            pipeline: self.name.clone(),
            zone_type: crate::zones::ZoneType::Base,
        };

        pzone.cleanup();

        print!("Creating VNIC {}...", self.vnic_name().cyan());
        io::stdout().lock().flush().unwrap();
        dladm::ensure_nic_exists(&self.vnic_name());
        println!("{}", "DONE".green());

        print!("Configuring zone...");
        io::stdout().lock().flush().unwrap();
        crate::zones::configure_zone_with_default_config(&pzone).await?;
        println!("{}", "DONE".green());

        print!("Installing zone...");
        io::stdout().lock().flush().unwrap();
        zone::Adm::new(self.zone_name())
            .install_blocking(&[])?;
        println!("{}", "DONE".green());

        print!("Booting zone...");
        io::stdout().lock().flush().unwrap();
        zone::Adm::new(self.zone_name())
            .boot_blocking()?;
        println!("{}", "DONE".green());

        tokio::time::sleep(Duration::new(30,0)).await;

        // Setup network access
        crate::zones::configure_zone_networking(&pzone).await?;

        Ok(())
    }

    pub async fn clone_repos(&self) -> Result<()> {
        for repo in self.repos.iter() {
            print!("Cloning repo {}...", repo.url.yellow());
            io::stdout().lock().flush().unwrap();
            zone::Zlogin::new(self.zone_name())
                .exec_blocking(format!("git clone {}", repo.url))?;
            println!("{}", "DONE".green());
        }

        Ok(())
    }

    pub fn install_packages(&self) -> Result<()> {
        print!("Installing packages ({}) This may take a while...", "elixir".yellow());
        io::stdout().lock().flush().unwrap();
        zone::Zlogin::new(self.zone_name())
            .exec_blocking("pkg install git")?;
        zone::Zlogin::new(self.zone_name())
            .exec_blocking("pkgin -y install elixir")?;
        println!("{}", "DONE".green());
        Ok(())
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

pub struct Filter {
    key: String,
    value: String,
}

pub enum CfgCommand {
    Select {
        ty: String,
        filter: Option<Filter>, 
    },
    Set {
        key: String,
        value: String,
    },
    Add {
        ty: String,
    },
    Print,
    End,
    Commit,
}

fn identifier(input: &str) -> IResult<&str, &str> {
    take_while1(|c: char| c.is_alphanumeric() || c == '_').parse(input)
}

// Parse a key=value pair
fn key_value_pair(input: &str) -> IResult<&str, (&str, &str)> {
    separated_pair(identifier, char('='), rest).parse(input)
}

// Parse "end" command
fn parse_end(input: &str) -> IResult<&str, CfgCommand> {
    map(tag("end"), |_| CfgCommand::End).parse(input)
}

fn parse_commit(input: &str) -> IResult<&str, CfgCommand> {
    map(tag("commit"), |_| CfgCommand::Commit).parse(input)
}

fn parse_print(input: &str) -> IResult<&str, CfgCommand> {
    map(tag("print"), |_| CfgCommand::Print).parse(input)
}

// Parse "select attr name=test" command
fn parse_select(input: &str) -> IResult<&str, CfgCommand> {
    map(
        (
            tag("select"),
            multispace1,
            identifier,
            opt((
                multispace1,
                key_value_pair
            )),
        ),
        |(_, _, ty, kv)| CfgCommand::Select {
            ty: ty.to_string(),
            filter: match kv {
                Some((_, (name, value))) => Some(Filter {
                    key: name.to_string(),
                    value: value.to_string(),
                }),
                None => None
            },
        },
    ).parse(input)
}

// Parse "set name=test" command
fn parse_set(input: &str) -> IResult<&str, CfgCommand> {
    map(
        (tag("set"), multispace1, key_value_pair),
        |(_, _, (name, value))| CfgCommand::Set {
            key: name.to_string(),
            value: value.to_string(),
        },
    ).parse(input)
}

// Parse "add attr" command
fn parse_add(input: &str) -> IResult<&str, CfgCommand> {
    map(
        (tag("add"), multispace1, identifier),
        |(_, _, ty)| CfgCommand::Add {
            ty: ty.to_string(),
        },
    ).parse(input)
}

// Main parser that tries all command types
pub fn parse_command(input: &str) -> IResult<&str, CfgCommand> {
    preceded(
        multispace0,
        alt((parse_end, parse_print, parse_select, parse_set, parse_add, parse_commit)),
    ).parse(input)
}

pub fn prompt_read(prompt: &str) -> String {
    print!("{}", prompt);
    io::stdout().lock().flush().unwrap();

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read input");

    input.trim().to_string()
}

#[derive(Debug)]
pub enum Frame {
    Pipeline(Rc<RefCell<Pipeline>>),
    Step(Rc<RefCell<Step>>),
    Package(Rc<RefCell<Package>>),
    Repo(Rc<RefCell<Repo>>),
}

impl Frame {
    pub fn name(&self) -> String {
        match self {
            Frame::Pipeline(p) => p.borrow().name(),
            Frame::Step(s) => s.borrow().name(),
            Frame::Package(p) => p.borrow().name(),
            Frame::Repo(r) => r.borrow().name(),
        }
    }
}

#[derive(Debug)]
pub struct CfgState {
    stack: Vec<Frame>,
    inner: Rc<RefCell<Pipeline>>,
    path: Vec<String>,
}

impl CfgState {
    pub fn new() -> CfgState {
        let p = Rc::new(RefCell::new(Pipeline::new()));

        Self {
            inner: p.clone(),
            stack: vec![Frame::Pipeline(p.clone())],
            path: vec!["pipeline".to_string()],
        }
    }

    pub fn prompt(&self) -> String {
        [
            ["cicfg".to_string()].into_iter().chain(self.stack.iter().map(|x| format!("{}", x.name().yellow()))).join(":"),
            "> ".to_string()
        ].join("")
    }

    pub fn stack_top(&self) -> Option<&Frame> {
        self.stack.last()
    }

    pub fn select(&mut self, ty: String, filter: Option<Filter>) -> Result<()> {
        match self.stack_top() {
            Some(Frame::Pipeline(pipeline)) => {
                let frame = pipeline.borrow().select(ty, filter)?;
                self.stack.push(frame);
                Ok(())
            },
            _ => Err(anyhow!("Can't select anything from here")),
        }
    }

    pub fn set(&mut self, key: String, value: String) -> Result<()> {
        match self.stack_top() {
            Some(Frame::Pipeline(pipeline)) => pipeline
                .borrow_mut()
                .set(key, value),
            Some(Frame::Package(package)) => package
                .borrow_mut()
                .set(key, value),
            Some(Frame::Repo(repo)) => repo
                .borrow_mut()
                .set(key, value),
            Some(Frame::Step(step)) => step
                .borrow_mut()
                .set(key, value),
            None => unreachable!(),
            frame => Err(anyhow!("Unsupported frame value: {:?}", frame)),
        }?;
        Ok(())
    }

    pub fn add(&mut self, ty: String) -> Result<()> {
        match self.stack_top() {
            Some(Frame::Pipeline(pipeline)) => {
                match ty.as_str() {
                    "package" => {
                        let p = Rc::new(RefCell::new(Package::default()));
                        pipeline.borrow_mut().packages.push(p.clone());
                        self.stack.push(Frame::Package(p.clone()));
                        Ok(())
                    },
                    "repo" => {
                        let p = Rc::new(RefCell::new(Repo::default()));
                        pipeline.borrow_mut().repos.push(p.clone());
                        self.stack.push(Frame::Repo(p.clone()));
                        Ok(())
                    },
                    "step" => {
                        let p = Rc::new(RefCell::new(Step::default()));
                        pipeline.borrow_mut().steps.push(p.clone());
                        self.stack.push(Frame::Step(p.clone()));
                        Ok(())
                    },
                    _ => todo!()
                }
            },
            _ => todo!(),
        }
    }

    pub fn end(&mut self) -> Result<()> {
        if self.stack.len() == 1 {
            println!("Nothing to end");
        } else {
            self.stack.pop();
        }
        Ok(())
    }
}

pub async fn builder() -> Result<()> {
    let mut state = CfgState::new();

    loop {
        let prompt = state.prompt();
        let response = prompt_read(prompt.as_str());

        match parse_command(response.as_str()) {
            Ok((_, CfgCommand::Select { ty, filter })) => state.select(ty, filter),
            Ok((_, CfgCommand::Set { key, value })) => state.set(key, value),
            Ok((_, CfgCommand::Add { ty })) => state.add(ty),
            Ok((_, CfgCommand::Print)) => {
                println!("{:?}", state.stack_top().unwrap());
                Ok(())
            },
            Ok((_, CfgCommand::End)) => {
                state.end()?;
                Ok(())
            },
            Ok((_, CfgCommand::Commit)) => {
                match state.inner.borrow().validate() {
                    Ok(vp) => {
                        let xml = serde_xml_rs::to_string(&vp).unwrap();
                        println!("{}", xml);
                        vp.apply().await?;
                    },
                    Err(err) => println!("{:?}", err),
                };
                Ok(())
            },
            Err(_) => {
                println!("Unrecognized command");
                Ok(())
            },
        }?;
    }

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    /*
    let pipeline = Pipeline::new();
    pipeline.create();

    let app = Router::new()
        .route("/", get(root));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();

    if let facet::Type::User(facet::UserType::Struct(ty)) = Pipeline::SHAPE.ty {
        println!("{:?}", ty.fields);
        let pipeline = Pipeline::new();
        let p = Peek::new(&pipeline).into_struct()?;
        println!("{:?}", p.field_by_name("name"));
    }
    */

    // builder().await?;

    let base_pzone = crate::zones::PipelineZone {
        pipeline: "katarineko".to_string(),
        zone_type: crate::zones::ZoneType::Base,
    };

    crate::pipeline::run_pipeline(&base_pzone).await?;

    Ok(())
}

async fn root() -> &'static str {
    "Hello, World!"
}


