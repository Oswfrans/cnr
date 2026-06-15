use axial_core::{GoalId, Payload};
use axial_store::{EventStore, SqliteEventStore};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlannerAction {
    CreateTask { title: String, instructions: String },
    DispatchRun { task_id: String, executor: String },
    PostDigest { body: String },
    AskHuman { question: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PlannerOutput {
    pub actions: Vec<PlannerAction>,
}

pub fn plan_next(store: &SqliteEventStore, goal_id: &GoalId) -> anyhow::Result<PlannerOutput> {
    let events = store.read_all(None)?;
    let mut has_run = false;
    let mut has_ready_claim = false;
    for event in events {
        match event.payload {
            Payload::RunStarted(run) if run.goal.as_ref() == Some(goal_id) => has_run = true,
            Payload::ClaimAsserted(claim)
                if claim.subject.0.contains(&goal_id.0)
                    || claim.predicate.0 == "ready_for_review" =>
            {
                has_ready_claim = true;
            }
            _ => {}
        }
    }

    if !has_run {
        return Ok(PlannerOutput {
            actions: vec![
                PlannerAction::CreateTask {
                    title: "Understand goal and propose first implementation slice".to_string(),
                    instructions: format!(
                        "Inspect the repository, identify the smallest useful first slice for {}, and emit cnr-claims.",
                        goal_id.0
                    ),
                },
                PlannerAction::DispatchRun {
                    task_id: format!("task:{}", goal_id.0),
                    executor: "local".to_string(),
                },
                PlannerAction::PostDigest {
                    body: "Crown created the first task and requested Reach execution.".to_string(),
                },
            ],
        });
    }

    if has_ready_claim {
        Ok(PlannerOutput {
            actions: vec![PlannerAction::PostDigest {
                body: "Reach reported work ready for review. Human approval is the next gate."
                    .to_string(),
            }],
        })
    } else {
        Ok(PlannerOutput {
            actions: vec![PlannerAction::AskHuman {
                question: "Reach has not produced a ready_for_review claim yet. Continue, redirect, or inspect transcript?".to_string(),
            }],
        })
    }
}
