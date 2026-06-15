use axial_core::Payload;
use axial_store::{EventStore, SqliteEventStore};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CnrStateSummary {
    pub events: usize,
    pub goals: usize,
    pub runs: usize,
    pub claims: usize,
    pub artifacts: usize,
}

pub fn replay(store: &SqliteEventStore) -> anyhow::Result<CnrStateSummary> {
    axial_projections::replay(store)?;
    summarize(store)
}

pub fn summarize(store: &SqliteEventStore) -> anyhow::Result<CnrStateSummary> {
    let events = store.read_all(None)?;
    let mut summary = CnrStateSummary {
        events: events.len(),
        goals: 0,
        runs: 0,
        claims: 0,
        artifacts: 0,
    };
    for event in events {
        match event.payload {
            Payload::GoalCreated(_) => summary.goals += 1,
            Payload::RunStarted(_) => summary.runs += 1,
            Payload::ClaimAsserted(_) => summary.claims += 1,
            Payload::ArtifactRef(_) => summary.artifacts += 1,
            _ => {}
        }
    }
    Ok(summary)
}

pub fn status_line(store: &SqliteEventStore, goal: &str) -> anyhow::Result<Option<String>> {
    axial_projections::replay(store)?;
    Ok(axial_projections::goal_status(store.connection(), goal)?
        .map(|view| format!("{} {} {}", view.id.0, view.status, view.title)))
}
