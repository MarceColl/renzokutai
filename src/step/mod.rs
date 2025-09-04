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
use crate::Value;
use anyhow::{Result, anyhow};
use owo_colors::OwoColorize;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::iter::Iterator;
use std::sync::Arc;
use tokio::sync::RwLock;

mod runnable;

pub use runnable::*;

/// Container of steps to run in a pipeline
pub struct ValidatedSteps {
    pub steps_by_name: HashMap<String, ValidatedStep>,
}

impl ValidatedSteps {
    pub fn as_runnable(&self) -> RunnableSteps {
        RunnableSteps {
            steps: self
                .steps_by_name
                .iter()
                .map(|(_k, s)| s.as_runnable())
                .collect(),
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct Dependency {
    pub name: String,
}

#[derive(Debug, Default, Clone, Serialize, PartialEq)]
pub struct ValidatedDependency {
    pub name: String,
}

#[derive(Debug, Default)]
pub struct Step {
    pub name: Value<String>,
    pub script: Value<String>,
    pub depends: Vec<Dependency>,
}

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct ValidatedStep {
    #[serde(rename = "@name")]
    pub name: String,
    #[serde(rename = "@script")]
    pub script: String,
    #[serde(rename = "depend")]
    pub depends: Vec<ValidatedDependency>,
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
            // TODO(Marce)
            depends: Vec::new(),
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

impl crate::Filterable for Step {
    fn inner_filter(&self, filter: &crate::Filter) -> bool {
        match filter.key.as_str() {
            "name" => self.name == Value::Set(filter.value.clone()),
            "script" => self.script == Value::Set(filter.value.clone()),
            // "depends" => self.depends == filter.value,
            _ => false,
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
            result: StepResult::default()
        }))
    }

    pub fn is_available(&self, finished_steps: &HashSet<String>) -> bool {
        self.depends
            .iter()
            .all(|d| finished_steps.contains(&d.name))
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
                depends: vec![ValidatedDependency { name: "build".to_string() }],
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



