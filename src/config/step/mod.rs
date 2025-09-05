///! # Pipeline Steps Module
///!
///! This module implements a pipeline execution system that manages steps with
///! dependencies, ensuring proper execution order and handling step lifecycle states.
///!
///! ## Architecture
///!
///! The pipeline system uses three main types representing different stages of a step's lifecycle:
///!
///!    RAW CONFIG                      VALIDATED                           RUNTIME EXECUTION
///!                                                  
///!  ┌──────────┐                  ┌─────────────────┐                     ┌────────────────┐
///!  │   Step   │── .validate() ──▶│  ValidatedStep  │── .as_runnable() ──▶│  RunnableStep  │
///!  └──────────┘                  └─────────────────┘                     └────────────────┘
///!
///! CONTAINER TYPES:
///!
///!                                ┌─────────────────┐                     ┌─────────────────┐
///!                                │ ValidatedSteps  │── .as_runnable() ──▶│  RunnableSteps  │
///!                                └─────────────────┘                     └─────────────────┘
///!
///!
///! STATUS TRANSITIONS:
///!
///!  Pending ──▶ Running ──▶ Finished
///!                 │
///!                 ▼
///!               Failed
///!
use crate::config::{Filter, Frame, Value};
use crate::filterable::Filterable;
use anyhow::{Result, anyhow};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::iter::Iterator;
use std::{cell::RefCell, rc::Rc, sync::Arc};
use tokio::sync::RwLock;

mod runnable;

pub use runnable::*;

#[derive(Debug)]
pub struct Steps {
    vec: Vec<Rc<RefCell<Step>>>,
}

/// Container of steps to run in a pipeline
#[derive(Debug, Serialize, Deserialize)]
pub struct ValidatedSteps {
    #[serde(rename = "step")]
    vec: Vec<ValidatedStep>,
}

impl Steps {
    pub fn new() -> Self {
        Self { vec: Vec::new() }
    }

    pub fn add_empty(&mut self) -> Frame {
        let s = Rc::new(RefCell::new(Step::default()));
        self.vec.push(s.clone());
        Frame::Step(s.clone())
    }

    pub fn select(&self, filter: &Option<Filter>) -> Result<Frame> {
        let matching: Vec<_> = self
            .vec
            .iter()
            .filter(|f| f.borrow().filter(filter))
            .collect();

        match matching[..] {
            [x] => Ok(Frame::Step(x.clone())),
            [] => Err(anyhow!("No element matched the filter")),
            _ => Err(anyhow!("More than one element matched the filter")),
        }
    }

    pub fn validate(&self) -> Result<ValidatedSteps> {
        let step_names: HashSet<String> = self
            .vec
            .iter()
            .map(|s| s.borrow().name.clone().ensure())
            .collect::<Result<HashSet<String>>>()?;
        let vsteps = self
            .vec
            .iter()
            .map(|r| r.borrow().validate(&step_names))
            .collect::<Result<Vec<ValidatedStep>>>()?;
        Ok(ValidatedSteps { vec: vsteps })
    }
}

impl ValidatedSteps {
    pub fn as_runnable(&self) -> RunnableSteps {
        RunnableSteps {
            steps: self.vec.iter().map(|s| s.as_runnable()).collect(),
        }
    }

    pub fn as_steps(&self) -> Steps {
        let steps = self
            .vec
            .iter()
            .map(|p| Rc::new(RefCell::new(p.as_step())))
            .collect();
        Steps { vec: steps }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Dependency {
    pub name: Value<String>,
}

impl Dependency {
    pub fn validate(&self, step_names: &HashSet<String>) -> Result<ValidatedDependency> {
        let name = self.name.ensure()?;

        if step_names.contains(&name) {
            Ok(ValidatedDependency { name: name.clone() })
        } else {
            Err(anyhow!("Step depends on a non-existing step: {}", name))
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidatedDependency {
    pub name: String,
}

impl ValidatedDependency {
    pub fn as_dependency(&self) -> Dependency {
        Dependency {
            name: Value::Set(self.name.clone()),
        }
    }
}

#[derive(Debug, Default)]
pub struct Step {
    pub name: Value<String>,
    pub script: Value<String>,
    pub depends: Vec<Dependency>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ValidatedStep {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@script")]
    pub script: String,
    #[serde(default)]
    #[serde(rename = "depend")]
    pub depends: Vec<ValidatedDependency>,
}

impl Step {
    pub fn validate(&self, step_names: &HashSet<String>) -> Result<ValidatedStep> {
        let name = self.name.ensure()?;
        let script = self.script.ensure()?;

        Ok(ValidatedStep {
            name: name.clone(),
            script: script.clone(),
            depends: self
                .depends
                .iter()
                .map(|d| d.validate(&step_names))
                .collect::<Result<Vec<ValidatedDependency>>>()?,
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
            "name" => {
                self.name = Value::Set(value);
                Ok(())
            }
            "script" => {
                self.script = Value::Set(value);
                Ok(())
            }
            // TODO
            "depends" => {
                self.depends = Vec::new();
                Ok(())
            }
            _ => Err(anyhow!("Unknown attribute for package: {}", key)),
        }
    }
}

impl ValidatedStep {
    pub fn run(&self) -> Result<()> {
        Ok(())
    }

    pub fn as_runnable(&self) -> RunnableStep {
        Arc::new(RwLock::new(InnerRunnableStep {
            step: self.clone(),
            result: StepResult::default(),
        }))
    }

    pub fn is_available(&self, finished_steps: &HashSet<String>) -> bool {
        self.depends
            .iter()
            .all(|d| finished_steps.contains(&d.name))
    }

    pub fn as_step(&self) -> Step {
        Step {
            name: Value::Set(self.name.clone()),
            script: Value::Set(self.script.clone()),
            depends: self.depends.iter().map(|s| s.as_dependency()).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_without_dependencies() {
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
            "lint".to_string(),
            ValidatedStep {
                name: "lint".to_string(),
                script: "lint.sh".to_string(),
                depends: Vec::new(),
            },
        );

        let steps = ValidatedSteps { steps_by_name };
        let mut rsteps = steps.as_runnable();
        let mut iter = rsteps.iter_mut();

        if let Some(Some(steps)) = iter.next() {
            assert_eq!(steps.len(), 2);
            for step in steps.iter() {
                step.borrow_mut().status = Status::Running;
            }

            assert_eq!(iter.next(), Some(None));

            for step in steps.iter() {
                step.borrow_mut().status = Status::Finished;
            }

            assert_eq!(iter.next(), None);
        } else {
            panic!("Should have returned some steps");
        }
    }

    #[test]
    fn steps_with_dependencies() {
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
                depends: vec![ValidatedDependency {
                    name: "build".to_string(),
                }],
            },
        );

        let steps = ValidatedSteps { steps_by_name };
        let mut rsteps = steps.as_runnable();
        let mut iter = rsteps.iter_mut();

        if let Some(Some(steps)) = iter.next() {
            assert_eq!(steps.len(), 1);
            for step in steps.iter() {
                step.borrow_mut().status = Status::Running;
            }

            assert_eq!(iter.next(), Some(None));

            for step in steps.iter() {
                step.borrow_mut().status = Status::Finished;
            }

            if let Some(Some(steps)) = iter.next() {
                assert_eq!(steps.len(), 1);

                for step in steps.iter() {
                    step.borrow_mut().status = Status::Finished;
                }

                assert_eq!(iter.next(), None);
            }
        } else {
            panic!("Should have returned some steps");
        }
    }
}
