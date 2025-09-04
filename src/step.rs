use crate::Value;
use anyhow::{Result, anyhow};
use owo_colors::OwoColorize;
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::iter::Iterator;
use std::rc::Rc;

/// Container of steps to run in a pipeline
pub struct ValidatedSteps {
    pub steps_by_name: HashMap<String, ValidatedStep>,
}

impl ValidatedSteps {
    pub fn as_runnable(&self) -> RunnableSteps {
        RunnableSteps {
            steps_by_name: self
                .steps_by_name
                .iter()
                .map(|(k, s)| (k.clone(), Rc::new(RefCell::new(s.as_runnable()))))
                .collect(),
        }
    }
}

pub struct RunnableSteps {
    steps_by_name: HashMap<String, Rc<RefCell<RunnableStep>>>,
}

impl RunnableSteps {
    pub fn run(&mut self, pzone: &crate::zones::PipelineZone) -> Result<()> {
        for steps in self.iter_mut() {
            match steps {
                Some(steps) => {
                    for step in steps {
                        step.borrow_mut().run(pzone)?;
                    }
                },
                None => ()
            }
        }

        Ok(())
    }

    pub fn iter_mut(&mut self) -> StepsIter {
        StepsIter {
            steps: self.steps_by_name.iter().map(|(_k, s)| s.clone()).collect(),
        }
    }
}

struct StepsIter {
    // NOTE(Marce): I thought about using a toposort here, but I believe there
    // may be cases where we do not start a step earlier due to the linearized
    // nature of toposort even though it's available, so on each `next` I'll just
    // check for available steps.
    // We are still validating that there are no cycles at ValidatedSteps creation
    // so I *believe* we are bound to eventually make work even if individual
    // calls to next may not advance.
    steps: Vec<Rc<RefCell<RunnableStep>>>,
}

impl Iterator for StepsIter {
    // We nest two options, the top level option means "We are done with the steps",
    // the Inner option means "we cannot run any step right now".
    type Item = Option<Vec<Rc<RefCell<RunnableStep>>>>;

    fn next(&mut self) -> Option<Self::Item> {
        let remaining: Vec<_> = self
                .steps
                .iter()
                .filter(|s| s.borrow().status == Status::Pending || s.borrow().status == Status::Running)
                .map(|s| s.clone())
                .collect();

        if remaining.is_empty() {
            None
        } else {
            let completed = self
                .steps
                .iter()
                .filter(|s| s.borrow().status == Status::Finished)
                .map(|s| s.borrow().step.name.clone())
                .collect();

            let available: Vec<_> = remaining
                .iter()
                .filter(|s| s.borrow().is_available(&completed))
                .map(|s| s.clone())
                .collect();

            if available.is_empty() {
                Some(None)
            } else {
                Some(Some(available))
            }
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

#[derive(Debug, PartialEq)]
pub enum Status {
    Pending,
    Running,
    Failed,
    Finished,
}

#[derive(Debug, PartialEq)]
pub struct RunnableStep {
    step: ValidatedStep,
    status: Status,
}

impl RunnableStep {
    pub fn is_available(&self, finished_steps: &HashSet<String>) -> bool {
        self.step.is_available(finished_steps) && self.status == Status::Pending
    }

    pub fn run(&mut self, pzone: &crate::zones::PipelineZone) -> Result<()> {
        self.status = Status::Pending;
        pzone.exec(format!("echo 'HOLA!'"))?;
        self.status = Status:: Finished;
        Ok(())
    }
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
        RunnableStep {
            step: self.clone(),
            status: Status::Pending,
        }
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



