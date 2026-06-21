//! Discord command surface for CNR.
//!
//! Discord is transport only. Commands read projections from Axial or append
//! events to Axial; no durable workflow truth lives in Discord-specific state.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use axial_core::{GoalId, StreamId};
use axial_store::{EventStore, ExpectedVersion, SqliteEventStore};
use cnr_core::{goal_created_events, normalize_goal};

pub const ADAPTER: &str = "discord";
pub const DEFAULT_PREFIX: &str = "!cnr";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscordConfig {
    pub token: String,
    pub channel_id: Option<u64>,
    pub root: PathBuf,
    pub prefix: String,
}

impl DiscordConfig {
    pub fn from_env(root: PathBuf, prefix: Option<String>) -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();
        let token = env::var("DISCORD_BOT_TOKEN")
            .with_context(|| "DISCORD_BOT_TOKEN is required to run cnr-discord")?;
        let channel_id = optional_u64_env("DISCORD_CHANNEL_ID")?;
        Ok(Self {
            token,
            channel_id,
            root,
            prefix: prefix.unwrap_or_else(|| DEFAULT_PREFIX.to_string()),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscordCommand {
    Help,
    Goal { title: String },
    Status { goal: Option<String> },
    Claims { subject: String },
    Log { limit: usize },
    Manager { goal: String, executor: String },
    Replay,
}

pub fn parse_command(prefix: &str, content: &str) -> anyhow::Result<Option<DiscordCommand>> {
    let Some(rest) = content.strip_prefix(prefix) else {
        return Ok(None);
    };
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return Ok(None);
    }
    let rest = rest.trim();
    if rest.is_empty() || rest == "help" {
        return Ok(Some(DiscordCommand::Help));
    }

    let mut parts = rest.split_whitespace();
    let Some(command) = parts.next() else {
        return Ok(Some(DiscordCommand::Help));
    };

    let parsed = match command {
        "goal" => {
            let title = rest
                .strip_prefix("goal")
                .unwrap_or_default()
                .trim()
                .to_string();
            if title.is_empty() {
                bail!("usage: {prefix} goal <title>");
            }
            DiscordCommand::Goal { title }
        }
        "status" => DiscordCommand::Status {
            goal: parts.next().map(str::to_string),
        },
        "claims" => {
            let Some(subject) = parts.next() else {
                bail!("usage: {prefix} claims <subject>");
            };
            DiscordCommand::Claims {
                subject: subject.to_string(),
            }
        }
        "log" => {
            let limit = parts
                .next()
                .map(str::parse)
                .transpose()
                .with_context(|| "log limit must be a number")?
                .unwrap_or(10);
            DiscordCommand::Log { limit }
        }
        "manager" => {
            let Some(goal) = parts.next() else {
                bail!("usage: {prefix} manager <goal> [local|modal]");
            };
            let executor = parts.next().unwrap_or("local").to_string();
            DiscordCommand::Manager {
                goal: goal.to_string(),
                executor,
            }
        }
        "replay" => DiscordCommand::Replay,
        other => bail!("unknown command: {other}. Try `{prefix} help`."),
    };

    Ok(Some(parsed))
}

pub fn execute_command(root: &Path, command: DiscordCommand) -> anyhow::Result<String> {
    let store = open_store(root)?;
    match command {
        DiscordCommand::Help => Ok(help_text()),
        DiscordCommand::Goal { title } => create_goal(&store, title),
        DiscordCommand::Status { goal } => {
            cnr_projections::replay(&store)?;
            if let Some(goal) = goal {
                let goal = normalize_goal(&goal);
                match cnr_projections::status_line(&store, &goal)? {
                    Some(line) => Ok(line),
                    None => bail!("goal not found: {goal}"),
                }
            } else {
                let summary = cnr_projections::summarize(&store)?;
                Ok(format!(
                    "events={} goals={} runs={} claims={} artifacts={}",
                    summary.events, summary.goals, summary.runs, summary.claims, summary.artifacts
                ))
            }
        }
        DiscordCommand::Claims { subject } => {
            cnr_projections::replay(&store)?;
            let claims = axial_projections::claims_for_subject(store.connection(), &subject)?;
            if claims.is_empty() {
                return Ok(format!("no claims for {subject}"));
            }
            let mut lines = Vec::new();
            for view in claims {
                lines.push(format!(
                    "{}\t{}\t{}",
                    view.claim.id.0,
                    view.claim.predicate.0,
                    serde_json::to_string(&view.claim.value)?
                ));
            }
            Ok(lines.join("\n"))
        }
        DiscordCommand::Log { limit } => {
            let mut events = store.read_all(None)?;
            let keep_from = events.len().saturating_sub(limit);
            events.drain(..keep_from);
            let lines = events
                .into_iter()
                .map(|event| {
                    format!(
                        "{}\t{}\t{}\t{}",
                        event.seq, event.stream_id.0, event.payload_type, event.id.0
                    )
                })
                .collect::<Vec<_>>();
            Ok(if lines.is_empty() {
                "no events yet".to_string()
            } else {
                lines.join("\n")
            })
        }
        DiscordCommand::Manager { goal, executor } => {
            if executor != "local" && executor != "modal" {
                bail!("executor must be local or modal");
            }
            let actions =
                cnr_manager::manager_cycle(&store, GoalId(normalize_goal(&goal)), &executor)?;
            let mut lines = vec![format!("actions={}", actions.len())];
            for action in actions {
                lines.push(serde_json::to_string(&action)?);
            }
            Ok(lines.join("\n"))
        }
        DiscordCommand::Replay => {
            let summary = cnr_projections::replay(&store)?;
            Ok(format!(
                "replayed {} events (goals={} claims={})",
                summary.events, summary.goals, summary.claims
            ))
        }
    }
}

pub fn help_text() -> String {
    [
        "CNR Discord commands:",
        "`!cnr goal <title>`",
        "`!cnr status [goal]`",
        "`!cnr claims <subject>`",
        "`!cnr log [limit]`",
        "`!cnr manager <goal> [local|modal]`",
        "`!cnr replay`",
    ]
    .join("\n")
}

pub fn truncate_discord_message(body: &str) -> String {
    const LIMIT: usize = 1_900;
    if body.len() <= LIMIT {
        return body.to_string();
    }
    let mut truncated = body
        .chars()
        .take(LIMIT.saturating_sub("...\n[truncated]".len()))
        .collect::<String>();
    truncated.push_str("...\n[truncated]");
    truncated
}

fn create_goal(store: &SqliteEventStore, title: String) -> anyhow::Result<String> {
    let (goal_id, events) = goal_created_events(title, ADAPTER);
    store.append(
        StreamId(goal_id.0.clone()),
        ExpectedVersion::NoStream,
        events,
    )?;
    cnr_projections::replay(store)?;
    Ok(goal_id.0)
}

fn open_store(root: &Path) -> anyhow::Result<SqliteEventStore> {
    let paths = SqliteEventStore::axial_paths(root);
    fs::create_dir_all(&paths.artifacts)?;
    SqliteEventStore::open(&paths.db).map_err(Into::into)
}

fn optional_u64_env(name: &str) -> anyhow::Result<Option<u64>> {
    match env::var(name) {
        Ok(value) if value.trim().is_empty() => Ok(None),
        Ok(value) => value
            .parse()
            .with_context(|| format!("{name} must be an integer"))
            .map(Some),
        Err(env::VarError::NotPresent) => Ok(None),
        Err(err) => Err(err).with_context(|| format!("failed to read {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_goal_with_spaces() {
        assert_eq!(
            parse_command("!cnr", "!cnr goal Fix the Discord adapter")
                .unwrap()
                .unwrap(),
            DiscordCommand::Goal {
                title: "Fix the Discord adapter".to_string()
            }
        );
    }

    #[test]
    fn ignores_non_prefixed_messages() {
        assert_eq!(parse_command("!cnr", "hello").unwrap(), None);
    }

    #[test]
    fn ignores_prefix_as_part_of_larger_word() {
        assert_eq!(parse_command("!cnr", "!cnrfoo").unwrap(), None);
    }

    #[test]
    fn truncates_long_messages() {
        let body = "x".repeat(2_500);
        let truncated = truncate_discord_message(&body);
        assert!(truncated.len() <= 1_900);
        assert!(truncated.ends_with("[truncated]"));
    }

    #[test]
    fn executes_goal_and_status_locally() {
        let dir = tempfile::tempdir().unwrap();
        let goal = execute_command(
            dir.path(),
            DiscordCommand::Goal {
                title: "Test Discord command surface".to_string(),
            },
        )
        .unwrap();

        let status =
            execute_command(dir.path(), DiscordCommand::Status { goal: Some(goal) }).unwrap();
        assert!(status.contains("Test Discord command surface"));
    }
}
