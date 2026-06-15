use std::collections::HashMap;
use std::path::PathBuf;

pub use axial_core::{
    ActorId, ArtifactId, ArtifactRef, Claim, ClaimId, ClaimRetraction, ClaimStatus, EventId,
    GoalCreated, GoalId, Message, MessageFormat, NewEvent, Payload, Predicate, RunFinished, RunId,
    RunStarted, RunStatus, StreamId, StreamType, Subject, ThreadCreated, ThreadId, fresh_id,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CnrActor {
    Human {
        id: String,
        name: Option<String>,
    },
    Connor {
        id: String,
    },
    Crown {
        id: String,
        model: String,
    },
    ReachWorker {
        id: String,
        model: String,
        harness: String,
        run: Option<RunId>,
    },
    System {
        id: String,
    },
}

impl From<CnrActor> for axial_core::Actor {
    fn from(actor: CnrActor) -> Self {
        match actor {
            CnrActor::Human { id, name } => axial_core::Actor::Human { id, name },
            CnrActor::Connor { id } => axial_core::Actor::Agent {
                id,
                model: None,
                harness: Some("connor".to_string()),
                run: None,
            },
            CnrActor::Crown { id, model } => axial_core::Actor::Agent {
                id,
                model: Some(model),
                harness: Some("crown".to_string()),
                run: None,
            },
            CnrActor::ReachWorker {
                id,
                model,
                harness,
                run,
            } => axial_core::Actor::Agent {
                id,
                model: Some(model),
                harness: Some(harness),
                run,
            },
            CnrActor::System { id } => axial_core::Actor::System { id },
        }
    }
}

pub fn human_actor() -> axial_core::Actor {
    CnrActor::Human {
        id: "human:local".to_string(),
        name: None,
    }
    .into()
}

pub fn crown_actor(model: impl Into<String>) -> axial_core::Actor {
    CnrActor::Crown {
        id: "crown:local".to_string(),
        model: model.into(),
    }
    .into()
}

pub fn system_actor() -> axial_core::Actor {
    CnrActor::System {
        id: "system:cnr".to_string(),
    }
    .into()
}

pub fn stream_type_for(stream: &str) -> StreamType {
    if stream.starts_with("goal:") {
        StreamType::Goal
    } else if stream.starts_with("thread:") {
        StreamType::Thread
    } else if stream.starts_with("run:") {
        StreamType::Run
    } else if stream.starts_with("claim:") {
        StreamType::Claim
    } else if stream.starts_with("artifact:") || stream.starts_with("artifact_") {
        StreamType::Artifact
    } else if stream.starts_with("system:") {
        StreamType::System
    } else if let Some((prefix, _)) = stream.split_once(':') {
        StreamType::Custom(prefix.to_string())
    } else {
        StreamType::Custom(stream.to_string())
    }
}

pub fn normalize_goal(goal: &str) -> String {
    if goal.starts_with("goal:") {
        goal.to_string()
    } else {
        format!("goal:{goal}")
    }
}

pub fn claim(
    subject: impl Into<String>,
    predicate: impl Into<String>,
    value: serde_json::Value,
    evidence: Vec<EventId>,
) -> Claim {
    Claim {
        id: ClaimId(fresh_id("claim")),
        subject: Subject(subject.into()),
        predicate: Predicate(predicate.into()),
        value,
        confidence: None,
        evidence,
        supersedes: vec![],
        status: ClaimStatus::Asserted,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunSpec {
    pub goal_id: GoalId,
    pub task_id: String,
    pub repo_path: PathBuf,
    pub base_ref: String,
    pub worker_model: String,
    pub harness: String,
    pub instructions: String,
    pub env: HashMap<String, String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunHandle {
    pub run_id: RunId,
    pub worktree_path: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CnrPredicate {
    TaskUnderstood,
    ApproachSelected,
    ImplementationChanged,
    TestsRun,
    TestsPass,
    Blocked,
    NeedsHuman,
    ReadyForReview,
    ApprovedForMerge,
    ApproachDead,
}

impl CnrPredicate {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::TaskUnderstood => "task_understood",
            Self::ApproachSelected => "approach_selected",
            Self::ImplementationChanged => "implementation_changed",
            Self::TestsRun => "tests_run",
            Self::TestsPass => "tests_pass",
            Self::Blocked => "blocked",
            Self::NeedsHuman => "needs_human",
            Self::ReadyForReview => "ready_for_review",
            Self::ApprovedForMerge => "approved_for_merge",
            Self::ApproachDead => "approach_dead",
        }
    }
}
