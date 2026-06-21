use axial_core::{
    ClaimStatus, GoalId, Message, MessageFormat, NewEvent, Payload, StreamId, StreamType,
};
use axial_store::{EventStore, ExpectedVersion, SqliteEventStore};
use cnr_core::{claim, crown_actor};
use cnr_planner::{PlannerAction, plan_next};

pub fn manager_cycle(
    store: &SqliteEventStore,
    goal_id: GoalId,
    executor: &str,
) -> anyhow::Result<Vec<PlannerAction>> {
    cnr_projections::replay(store)?;
    let output = plan_next(store, &goal_id, executor)?;
    for action in &output.actions {
        match action {
            PlannerAction::CreateTask {
                title,
                instructions,
            } => {
                let task_claim = claim(
                    format!("task:{}", goal_id.0),
                    "task_understood",
                    serde_json::json!({ "title": title, "instructions": instructions }),
                    vec![],
                );
                store.append(
                    StreamId(goal_id.0.clone()),
                    ExpectedVersion::Any,
                    vec![NewEvent::new(
                        StreamType::Goal,
                        crown_actor("frontier-planner"),
                        Payload::ClaimAsserted(task_claim),
                    )],
                )?;
            }
            PlannerAction::DispatchRun { task_id, executor } => {
                let dispatch_claim = axial_core::Claim {
                    status: ClaimStatus::Asserted,
                    ..claim(
                        task_id.clone(),
                        "approach_selected",
                        serde_json::json!({ "executor": executor }),
                        vec![],
                    )
                };
                store.append(
                    StreamId(goal_id.0.clone()),
                    ExpectedVersion::Any,
                    vec![NewEvent::new(
                        StreamType::Goal,
                        crown_actor("frontier-planner"),
                        Payload::ClaimAsserted(dispatch_claim),
                    )],
                )?;
            }
            PlannerAction::PostDigest { body } => {
                store.append(
                    StreamId(goal_id.0.clone()),
                    ExpectedVersion::Any,
                    vec![NewEvent::new(
                        StreamType::Goal,
                        crown_actor("frontier-planner"),
                        Payload::Message(Message {
                            body: body.clone(),
                            format: MessageFormat::Markdown,
                        }),
                    )],
                )?;
            }
            PlannerAction::AskHuman { question } => {
                let needs_human = claim(
                    goal_id.0.clone(),
                    "needs_human",
                    serde_json::json!({ "question": question }),
                    vec![],
                );
                store.append(
                    StreamId(goal_id.0.clone()),
                    ExpectedVersion::Any,
                    vec![NewEvent::new(
                        StreamType::Goal,
                        crown_actor("frontier-planner"),
                        Payload::ClaimAsserted(needs_human),
                    )],
                )?;
            }
        }
    }
    cnr_projections::replay(store)?;
    Ok(output.actions)
}
