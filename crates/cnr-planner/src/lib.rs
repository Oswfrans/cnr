use anyhow::bail;
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

pub fn plan_next(
    store: &SqliteEventStore,
    goal_id: &GoalId,
    executor: &str,
) -> anyhow::Result<PlannerOutput> {
    validate_executor(executor)?;
    let events = store.read_all(None)?;
    let mut has_run = false;
    let mut has_dispatch = false;
    let mut has_ready_claim = false;
    let task_id = first_task_id(goal_id);
    for event in events {
        match event.payload {
            Payload::RunStarted(run) if run.goal.as_ref() == Some(goal_id) => has_run = true,
            Payload::ClaimAsserted(claim) if claim.subject.0 == task_id => {
                match claim.predicate.0.as_str() {
                    "approach_selected" => has_dispatch = true,
                    "ready_for_review" => has_ready_claim = true,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if has_dispatch && !has_run {
        return Ok(PlannerOutput {
            actions: vec![PlannerAction::AskHuman {
                question: format!(
                    "Dispatch intent is already recorded for {task_id}, but no run has started yet. Start the executor, redirect, or inspect dispatch state?"
                ),
            }],
        });
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
                    task_id,
                    executor: executor.to_string(),
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

fn first_task_id(goal_id: &GoalId) -> String {
    format!("task:{}", goal_id.0)
}

fn validate_executor(executor: &str) -> anyhow::Result<()> {
    match executor {
        "local" | "modal" => Ok(()),
        other => bail!("unsupported executor `{other}`; expected `local` or `modal`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axial_core::{NewEvent, StreamId, StreamType};
    use axial_store::ExpectedVersion;
    use cnr_core::{claim, system_actor};

    fn store() -> SqliteEventStore {
        let dir = tempfile::tempdir().unwrap().keep();
        SqliteEventStore::open(dir.join("axial.db")).unwrap()
    }

    #[test]
    fn dispatch_uses_requested_executor() {
        let store = store();
        let output = plan_next(&store, &GoalId("goal:test".to_string()), "modal").unwrap();

        assert!(output.actions.iter().any(|action| {
            matches!(
                action,
                PlannerAction::DispatchRun { executor, .. } if executor == "modal"
            )
        }));
    }

    #[test]
    fn dispatch_defaults_to_local_when_caller_requests_local() {
        let store = store();
        let output = plan_next(&store, &GoalId("goal:test".to_string()), "local").unwrap();

        assert!(output.actions.iter().any(|action| {
            matches!(
                action,
                PlannerAction::DispatchRun { executor, .. } if executor == "local"
            )
        }));
    }

    #[test]
    fn rejects_unknown_executor() {
        let store = store();
        let err = plan_next(&store, &GoalId("goal:test".to_string()), "remote").unwrap_err();

        assert!(err.to_string().contains("unsupported executor"));
    }

    #[test]
    fn does_not_dispatch_same_task_twice_before_run_starts() {
        let store = store();
        let goal_id = GoalId("goal:test".to_string());
        store
            .append(
                StreamId(goal_id.0.clone()),
                ExpectedVersion::Any,
                vec![NewEvent::new(
                    StreamType::Goal,
                    system_actor(),
                    Payload::ClaimAsserted(claim(
                        first_task_id(&goal_id),
                        "approach_selected",
                        serde_json::json!({ "executor": "local" }),
                        vec![],
                    )),
                )],
            )
            .unwrap();

        let output = plan_next(&store, &goal_id, "local").unwrap();

        assert!(
            !output
                .actions
                .iter()
                .any(|action| matches!(action, PlannerAction::DispatchRun { .. }))
        );
        assert!(
            output
                .actions
                .iter()
                .any(|action| matches!(action, PlannerAction::AskHuman { .. }))
        );
    }
}
