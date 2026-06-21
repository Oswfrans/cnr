use std::fs;
use std::path::PathBuf;

use anyhow::{Context, bail};
use axial_core::{GoalId, NewEvent, Payload, StreamId, StreamType, fresh_id};
use axial_store::{EventStore, ExpectedVersion, SqliteEventStore};
use clap::{Parser, Subcommand};
use cnr_core::{goal_created_events, normalize_goal, stream_type_for, system_actor};

#[derive(Parser)]
#[command(
    name = "cnr",
    version,
    about = "Crown & Reach durable coordination runtime"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
    Goal {
        title: String,
    },
    Status {
        goal: Option<String>,
    },
    Log {
        #[arg(long)]
        stream: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    Claims {
        subject: String,
    },
    Run {
        transcript: PathBuf,
        #[arg(long)]
        goal: Option<String>,
        #[arg(long)]
        task: Option<String>,
    },
    Manager {
        #[arg(long)]
        goal: String,
        #[arg(long, default_value = "local")]
        executor: String,
        #[arg(long, default_value_t = 1)]
        workers: usize,
    },
    Loop {
        goal: String,
        #[arg(long, default_value_t = 1)]
        cycles: usize,
    },
    Ultrawork {
        goal: String,
    },
    Replay,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init => {
            let paths = paths();
            fs::create_dir_all(".cnr/runs")?;
            fs::create_dir_all(".cnr/cache")?;
            fs::create_dir_all(&paths.artifacts)?;
            SqliteEventStore::open(&paths.db)?;
            fs::write(
                ".cnr/config.toml",
                "runtime = \"cnr\"\nspine = \"axial\"\noperator_email = \"oswfrans@gmail.com\"\n",
            )?;
            println!("initialized .cnr and {}", paths.axial_dir.display());
        }
        Command::Goal { title } => {
            let store = open_store()?;
            let (goal_id, events) = goal_created_events(title, "connor-cli");
            store.append(
                StreamId(goal_id.0.clone()),
                ExpectedVersion::NoStream,
                events,
            )?;
            cnr_projections::replay(&store)?;
            println!("{}", goal_id.0);
        }
        Command::Status { goal } => {
            let store = open_store()?;
            cnr_projections::replay(&store)?;
            if let Some(goal) = goal {
                let goal = normalize_goal(&goal);
                match cnr_projections::status_line(&store, &goal)? {
                    Some(line) => println!("{line}"),
                    None => bail!("goal not found: {goal}"),
                }
            } else {
                let summary = cnr_projections::summarize(&store)?;
                println!(
                    "events={} goals={} runs={} claims={} artifacts={}",
                    summary.events, summary.goals, summary.runs, summary.claims, summary.artifacts
                );
            }
        }
        Command::Log { stream, limit } => {
            let store = open_store()?;
            let events = if let Some(stream) = stream {
                store.read_stream(StreamId(stream), None)?
            } else {
                let mut events = store.read_all(None)?;
                let keep_from = events.len().saturating_sub(limit);
                events.drain(..keep_from);
                events
            };
            for event in events {
                println!(
                    "{}\t{}\t{}\t{}",
                    event.seq, event.stream_id.0, event.payload_type, event.id.0
                );
            }
        }
        Command::Claims { subject } => {
            let store = open_store()?;
            cnr_projections::replay(&store)?;
            for view in axial_projections::claims_for_subject(store.connection(), &subject)? {
                println!(
                    "{}\t{}\t{}",
                    view.claim.id.0,
                    view.claim.predicate.0,
                    serde_json::to_string(&view.claim.value)?
                );
            }
        }
        Command::Run {
            transcript,
            goal,
            task,
        } => {
            let store = open_store()?;
            let body = fs::read_to_string(&transcript)
                .with_context(|| format!("failed to read {}", transcript.display()))?;
            let run_id = axial_core::RunId(fresh_id("run"));
            let task_id = task.unwrap_or_else(|| "manual".to_string());
            match cnr_executor::extract_claim_block(&body) {
                Ok(output) => {
                    let claims = cnr_executor::worker_claims_to_axial(output);
                    let mut events = vec![NewEvent::new(
                        StreamType::Run,
                        system_actor(),
                        Payload::RunStarted(axial_core::RunStarted {
                            id: run_id.clone(),
                            goal: goal.as_deref().map(normalize_goal).map(GoalId),
                            thread: None,
                            label: Some(format!("manual transcript {}", transcript.display())),
                        }),
                    )];
                    for claim in claims {
                        events.push(NewEvent::new(
                            StreamType::Run,
                            system_actor(),
                            Payload::ClaimAsserted(claim),
                        ));
                    }
                    events.push(NewEvent::new(
                        StreamType::Run,
                        system_actor(),
                        Payload::RunFinished(axial_core::RunFinished {
                            id: run_id.clone(),
                            status: axial_core::RunStatus::Succeeded,
                            summary: Some("parsed cnr-claims".to_string()),
                        }),
                    ));
                    store.append(
                        StreamId(run_id.0.clone()),
                        ExpectedVersion::NoStream,
                        events,
                    )?;
                }
                Err(err) => {
                    cnr_executor::append_run_lifecycle(
                        &store,
                        &run_id,
                        goal.as_deref().map(normalize_goal).map(GoalId),
                        Some(format!("manual transcript {}", transcript.display())),
                        axial_core::RunStatus::Failed,
                        Some(err.to_string()),
                    )?;
                    cnr_executor::append_incomplete_run_claim(
                        &store,
                        &run_id,
                        &task_id,
                        &err.to_string(),
                    )?;
                }
            }
            cnr_projections::replay(&store)?;
            println!("{}", run_id.0);
        }
        Command::Manager {
            goal,
            executor,
            workers,
        } => {
            if executor == "modal" {
                println!(
                    "modal executor requested; v0 records dispatch intent and keeps Modal out of durable truth"
                );
            }
            let store = open_store()?;
            let actions =
                cnr_manager::manager_cycle(&store, GoalId(normalize_goal(&goal)), &executor)?;
            println!("actions={} workers={workers}", actions.len());
            for action in actions {
                println!("{}", serde_json::to_string(&action)?);
            }
        }
        Command::Loop { goal, cycles } => {
            let store = open_store()?;
            for cycle in 1..=cycles {
                let actions =
                    cnr_manager::manager_cycle(&store, GoalId(normalize_goal(&goal)), "local")?;
                println!("cycle={cycle} actions={}", actions.len());
            }
        }
        Command::Ultrawork { goal } => {
            let store = open_store()?;
            let actions =
                cnr_manager::manager_cycle(&store, GoalId(normalize_goal(&goal)), "local")?;
            println!("proposed actions:");
            for (idx, action) in actions.iter().enumerate() {
                println!("{} {}", idx + 1, serde_json::to_string(action)?);
            }
            println!("approval gate: rerun cnr loop after approving or redirecting via claims");
        }
        Command::Replay => {
            let store = open_store()?;
            let summary = cnr_projections::replay(&store)?;
            println!(
                "replayed {} events (goals={} claims={})",
                summary.events, summary.goals, summary.claims
            );
        }
    }
    Ok(())
}

fn open_store() -> anyhow::Result<SqliteEventStore> {
    let paths = paths();
    fs::create_dir_all(&paths.artifacts)?;
    SqliteEventStore::open(&paths.db).map_err(Into::into)
}

fn paths() -> axial_store::AxialPaths {
    SqliteEventStore::axial_paths(std::env::current_dir().expect("current dir"))
}

#[allow(dead_code)]
fn append_payload(store: &SqliteEventStore, stream: &str, payload: Payload) -> anyhow::Result<()> {
    store.append(
        StreamId(stream.to_string()),
        ExpectedVersion::Any,
        vec![NewEvent::new(
            stream_type_for(stream),
            system_actor(),
            payload,
        )],
    )?;
    Ok(())
}
