pub mod package;
pub mod pipeline;
pub mod repo;
pub mod step;

pub use package::*;
pub use pipeline::*;
pub use repo::*;
pub use step::*;

use anyhow::{Result, anyhow};
use itertools::Itertools;
use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_while1},
    character::complete::{char, multispace0, multispace1},
    combinator::{map, opt, rest},
    sequence::{preceded, separated_pair},
};
use owo_colors::OwoColorize;
use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;

#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub enum Value<T: Clone> {
    #[default]
    Unset,
    Set(T),
}

impl<T: Clone> Value<T> {
    pub fn ensure(&self) -> Result<T> {
        match self {
            Value::Unset => Err(anyhow!("Value not set")),
            Value::Set(v) => Ok(v.clone()),
        }
    }
}

/// Configure interactive loop
pub async fn builder(pipeline_name: &String) -> Result<()> {
    let mut state = CfgState::new(pipeline_name)?;

    loop {
        let prompt = state.prompt();
        let response = prompt_read(prompt.as_str());

        match parse_command(response.as_str()) {
            Ok((_, CfgCommand::Select { ty, filter })) => state.select(ty, filter),
            Ok((_, CfgCommand::Set { key, value })) => state.set(key, value),
            Ok((_, CfgCommand::Add { ty })) => Ok(state.add(ty)),
            Ok((_, CfgCommand::Print)) => {
                println!("{:?}", state.stack_top().unwrap());
                Ok(())
            }
            Ok((_, CfgCommand::End)) => {
                state.end()?;
                Ok(())
            }
            Ok((_, CfgCommand::Commit)) => {
                match state.inner.borrow().validate() {
                    Ok(vp) => {
                        vp.save()?;
                        vp.apply().await?;
                    }
                    Err(err) => println!("{:?}", err),
                };
                Ok(())
            }
            Err(_) => {
                println!("Unrecognized command");
                Ok(())
            }
        }?;
    }
}

#[derive(Debug)]
pub struct CfgState {
    stack: Vec<Frame>,
    inner: Rc<RefCell<Pipeline>>,
}

impl CfgState {
    pub fn new(pipeline_name: &String) -> Result<CfgState> {
        let p = Pipeline::load_or_create(pipeline_name)?;
        let p = Rc::new(RefCell::new(p));

        Ok(Self {
            inner: p.clone(),
            stack: vec![Frame::Pipeline(p)],
        })
    }

    pub fn prompt(&self) -> String {
        [
            ["cicfg".to_string()]
                .into_iter()
                .chain(self.stack.iter().map(|x| format!("{}", x.name().yellow())))
                .join(":"),
            "> ".to_string(),
        ]
        .join("")
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
            }
            _ => Err(anyhow!("Can't select anything from here")),
        }
    }

    pub fn set(&mut self, key: String, value: String) -> Result<()> {
        match self.stack_top() {
            Some(Frame::Pipeline(pipeline)) => pipeline.borrow_mut().set(key, value),
            Some(Frame::Package(package)) => package.borrow_mut().set(key, value),
            Some(Frame::Repo(repo)) => repo.borrow_mut().set(key, value),
            Some(Frame::Step(step)) => step.borrow_mut().set(key, value),
            None => unreachable!(),
        }?;
        Ok(())
    }

    pub fn add(&mut self, ty: String) {
        match self.stack_top() {
            Some(Frame::Pipeline(pipeline)) => match ty.as_str() {
                "package" => {
                    let frame = pipeline.borrow_mut().packages.add_empty();
                    self.stack.push(frame);
                }
                "repo" => {
                    let frame = pipeline.borrow_mut().repos.add_empty();
                    self.stack.push(frame);
                }
                "step" => {
                    let frame = pipeline.borrow_mut().steps.add_empty();
                    self.stack.push(frame);
                }
                _ => todo!(),
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

pub struct Filter {
    pub key: String,
    pub value: String,
}

pub enum CfgCommand {
    Select { ty: String, filter: Option<Filter> },
    Set { key: String, value: String },
    Add { ty: String },
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
            opt((multispace1, key_value_pair)),
        ),
        |(_, _, ty, kv)| CfgCommand::Select {
            ty: ty.to_string(),
            filter: match kv {
                Some((_, (name, value))) => Some(Filter {
                    key: name.to_string(),
                    value: value.to_string(),
                }),
                None => None,
            },
        },
    )
    .parse(input)
}

// Parse "set name=test" command
fn parse_set(input: &str) -> IResult<&str, CfgCommand> {
    map(
        (tag("set"), multispace1, key_value_pair),
        |(_, _, (name, value))| CfgCommand::Set {
            key: name.to_string(),
            value: value.to_string(),
        },
    )
    .parse(input)
}

// Parse "add attr" command
fn parse_add(input: &str) -> IResult<&str, CfgCommand> {
    map((tag("add"), multispace1, identifier), |(_, _, ty)| {
        CfgCommand::Add { ty: ty.to_string() }
    })
    .parse(input)
}

// Main parser that tries all command types
pub fn parse_command(input: &str) -> IResult<&str, CfgCommand> {
    preceded(
        multispace0,
        alt((
            parse_end,
            parse_print,
            parse_select,
            parse_set,
            parse_add,
            parse_commit,
        )),
    )
    .parse(input)
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
