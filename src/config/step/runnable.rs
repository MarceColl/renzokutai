use crate::config::ValidatedStep;
use anyhow::Result;
use futures::stream::{self, StreamExt};
use owo_colors::OwoColorize;
use std::collections::HashSet;
use std::io::{Stderr, Stdout};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;

#[derive(Debug, Default)]
pub struct StepResult {
    status: Status,
    stdout: Option<BufReader<Stdout>>,
    stderr: Option<BufReader<Stderr>>,
}

impl PartialEq for StepResult {
    fn eq(&self, other: &Self) -> bool {
        self.status == other.status
    }
}

#[derive(Debug, PartialEq, Default)]
pub enum Status {
    #[default]
    Pending,
    Running,
    Failed,
    Finished,
}

#[derive(Debug, PartialEq)]
pub struct InnerRunnableStep {
    pub step: ValidatedStep,
    pub result: StepResult,
}

pub type RunnableStep = Arc<RwLock<InnerRunnableStep>>;

impl InnerRunnableStep {
    pub fn is_available(&self, finished_steps: &HashSet<String>) -> bool {
        self.step.is_available(finished_steps) && self.result.status == Status::Pending
    }

    pub async fn run(&mut self, pzone: &crate::zones::PipelineZone) -> Result<()> {
        self.result.status = Status::Pending;
        let mut child = pzone.exec(format!(
            "/usr/bin/sh -x ./renzokutai/{}",
            self.step.script
        ))?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        // TODO(Marce): Save into the DB
        tokio::select! {
            _result = async {
                while let Some(line) = stdout_reader.next_line().await? {
                    println!("stdout({}): {}", self.step.name.cyan(), line);
                }
                Ok::<(), Box<dyn std::error::Error>>(())
            } => { },

            _result = async {
                while let Some(line) = stderr_reader.next_line().await? {
                    println!("stderr({}): {}", self.step.name.cyan(), line.yellow());
                }
                Ok::<(), Box<dyn std::error::Error>>(())
            } => { },

            _status = child.wait() => {
                println!("Step {} {}", self.step.name, "DONE".green());
            }
        }

        self.result.status = Status::Finished;
        Ok(())
    }
}

pub struct RunnableSteps {
    pub steps: Vec<RunnableStep>,
}

impl RunnableSteps {
    /// Run available steps until completion of the Step Set
    pub async fn run(&mut self, pzone: &crate::zones::PipelineZone) -> Result<()> {
        let mut set = tokio::task::JoinSet::new();

        loop {
            match self.unblocked_steps().await {
                Some(mut steps) => {
                    for step in steps.drain(..) {
                        let cloned_pzone = pzone.clone();
                        set.spawn(async move { step.write().await.run(&cloned_pzone).await });
                    }
                }
                None => (),
            }

            match set.join_next().await {
                Some(res) => println!("Step finished: {:?}", res),
                None => break,
            }
        }

        Ok(())
    }

    async fn unblocked_steps(&mut self) -> Option<Vec<RunnableStep>> {
        let remaining: Vec<_> = stream::iter(&self.steps)
            .filter_map(async |s| {
                let inner = s.read().await;
                match inner.result.status {
                    Status::Pending | Status::Running => Some(s.clone()),
                    _ => None,
                }
            })
            .collect()
            .await;

        if remaining.is_empty() {
            None
        } else {
            let completed = stream::iter(&self.steps)
                .filter_map(async |s| {
                    let s = s.read().await;
                    match s.result.status {
                        Status::Finished => Some(s.step.name.clone()),
                        _ => None,
                    }
                })
                .collect()
                .await;

            let available: Vec<_> = stream::iter(remaining)
                .filter_map(async |s| match s.read().await.is_available(&completed) {
                    true => Some(s.clone()),
                    false => None,
                })
                .collect()
                .await;

            if available.is_empty() {
                None
            } else {
                Some(available)
            }
        }
    }
}
