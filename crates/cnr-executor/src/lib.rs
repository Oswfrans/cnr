use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};
use axial_core::{
    ArtifactRef, Claim, ClaimId, ClaimStatus, EventId, Payload, Predicate, RunFinished, RunId,
    RunStarted, RunStatus, StreamId, StreamType, Subject, fresh_id,
};
use axial_store::{ArtifactStore, EventStore, ExpectedVersion, SqliteEventStore};
use cnr_core::{CnrActor, RunHandle, RunSpec};
use serde::{Deserialize, Serialize};

pub trait Executor {
    fn spawn_run(&self, spec: RunSpec) -> anyhow::Result<RunHandle>;
    fn kill(&self, run_id: RunId) -> anyhow::Result<()>;
}

#[derive(Clone, Debug)]
pub struct LocalExecutor {
    pub root: PathBuf,
}

impl Executor for LocalExecutor {
    fn spawn_run(&self, spec: RunSpec) -> anyhow::Result<RunHandle> {
        let run_id = RunId(fresh_id("run"));
        let worktrees = self.root.join(".cnr").join("runs");
        fs::create_dir_all(&worktrees)?;
        let worktree_path = worktrees.join(&run_id.0);
        let status = Command::new("git")
            .arg("-C")
            .arg(&spec.repo_path)
            .arg("worktree")
            .arg("add")
            .arg("--detach")
            .arg(&worktree_path)
            .arg(&spec.base_ref)
            .status()
            .with_context(|| "failed to start git worktree")?;
        if !status.success() {
            bail!("git worktree add failed for {}", worktree_path.display());
        }
        fs::write(worktree_path.join("CNR_TASK.md"), spec.instructions)?;
        Ok(RunHandle {
            run_id,
            worktree_path: Some(worktree_path),
        })
    }

    fn kill(&self, _run_id: RunId) -> anyhow::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerOutput {
    #[serde(default)]
    pub claims: Vec<WorkerClaim>,
    #[serde(default)]
    pub artifacts: Vec<WorkerArtifact>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerClaim {
    pub subject: String,
    pub predicate: String,
    pub value: serde_json::Value,
    pub confidence: Option<f32>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkerArtifact {
    pub kind: String,
    pub path: String,
}

pub fn extract_claim_block(transcript: &str) -> anyhow::Result<WorkerOutput> {
    let fence = "```cnr-claims";
    let Some(start) = transcript.find(fence) else {
        bail!("missing cnr-claims fence");
    };
    let json_start = start + fence.len();
    let rest = &transcript[json_start..];
    let Some(end) = rest.find("```") else {
        bail!("unterminated cnr-claims fence");
    };
    let json = rest[..end].trim();
    serde_json::from_str(json).with_context(|| "malformed cnr-claims json")
}

pub fn worker_claims_to_axial(output: WorkerOutput) -> Vec<Claim> {
    output
        .claims
        .into_iter()
        .map(|claim| Claim {
            id: ClaimId(fresh_id("claim")),
            subject: Subject(claim.subject),
            predicate: Predicate(claim.predicate),
            value: claim.value,
            confidence: claim.confidence,
            evidence: claim.evidence.into_iter().map(EventId).collect(),
            supersedes: vec![],
            status: ClaimStatus::Asserted,
        })
        .collect()
}

pub fn append_incomplete_run_claim(
    store: &SqliteEventStore,
    run_id: &RunId,
    task_id: &str,
    reason: &str,
) -> anyhow::Result<()> {
    let claim = Claim {
        id: ClaimId(fresh_id("claim")),
        subject: Subject(format!("task:{task_id}")),
        predicate: Predicate("needs_human".to_string()),
        value: serde_json::json!({ "run": run_id.0, "reason": reason }),
        confidence: Some(1.0),
        evidence: vec![],
        supersedes: vec![],
        status: ClaimStatus::Asserted,
    };
    store.append(
        StreamId(run_id.0.clone()),
        ExpectedVersion::Any,
        vec![axial_core::NewEvent::new(
            StreamType::Run,
            CnrActor::System {
                id: "system:cnr".to_string(),
            }
            .into(),
            Payload::ClaimAsserted(claim),
        )],
    )?;
    Ok(())
}

pub fn append_run_lifecycle(
    store: &SqliteEventStore,
    run_id: &RunId,
    goal: Option<axial_core::GoalId>,
    label: Option<String>,
    status: RunStatus,
    summary: Option<String>,
) -> anyhow::Result<()> {
    store.append(
        StreamId(run_id.0.clone()),
        ExpectedVersion::Any,
        vec![
            axial_core::NewEvent::new(
                StreamType::Run,
                CnrActor::System {
                    id: "system:cnr".to_string(),
                }
                .into(),
                Payload::RunStarted(RunStarted {
                    id: run_id.clone(),
                    goal,
                    thread: None,
                    label,
                }),
            ),
            axial_core::NewEvent::new(
                StreamType::Run,
                CnrActor::System {
                    id: "system:cnr".to_string(),
                }
                .into(),
                Payload::RunFinished(RunFinished {
                    id: run_id.clone(),
                    status,
                    summary,
                }),
            ),
        ],
    )?;
    Ok(())
}

pub fn store_artifact(
    path: impl AsRef<Path>,
    artifact_root: impl AsRef<Path>,
) -> anyhow::Result<ArtifactRef> {
    Ok(ArtifactStore::new(artifact_root)?.put_file(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_fenced_claims() {
        let transcript = r#"hello
```cnr-claims
{"claims":[{"subject":"task:1","predicate":"ready_for_review","value":true,"confidence":0.82}],"artifacts":[]}
```
"#;
        let output = extract_claim_block(transcript).unwrap();
        assert_eq!(output.claims[0].predicate, "ready_for_review");
    }

    #[test]
    fn missing_claims_is_error() {
        assert!(extract_claim_block("plain transcript").is_err());
    }
}
