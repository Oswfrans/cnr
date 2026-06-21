use std::path::PathBuf;

use clap::Parser;
use cnr_discord::{DiscordConfig, execute_command, parse_command, truncate_discord_message};
use serenity::all::{ChannelId, Context, EventHandler, GatewayIntents, Message, Ready};
use serenity::async_trait;

#[derive(Parser)]
#[command(
    name = "cnr-discord",
    version,
    about = "Discord command surface for CNR"
)]
struct Cli {
    #[arg(long, default_value = ".")]
    root: PathBuf,
    #[arg(long)]
    prefix: Option<String>,
}

struct Handler {
    root: PathBuf,
    channel_id: Option<ChannelId>,
    prefix: String,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: Ready) {
        println!("cnr-discord connected as {}", ready.user.name);
        if let Some(channel_id) = self.channel_id {
            println!("listening in channel {}", channel_id);
        } else {
            println!("listening in every channel the bot can read");
        }
    }

    async fn message(&self, ctx: Context, msg: Message) {
        if msg.author.bot {
            return;
        }
        if self
            .channel_id
            .is_some_and(|channel| channel != msg.channel_id)
        {
            return;
        }

        let command = match parse_command(&self.prefix, &msg.content) {
            Ok(Some(command)) => command,
            Ok(None) => return,
            Err(err) => {
                send_reply(&ctx, &msg, format!("error: {err:#}")).await;
                return;
            }
        };

        let response = match execute_command(&self.root, command) {
            Ok(response) => response,
            Err(err) => format!("error: {err:#}"),
        };
        send_reply(&ctx, &msg, response).await;
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = DiscordConfig::from_env(cli.root, cli.prefix)?;
    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::DIRECT_MESSAGES
        | GatewayIntents::MESSAGE_CONTENT;
    let handler = Handler {
        root: config.root,
        channel_id: config.channel_id.map(ChannelId::new),
        prefix: config.prefix,
    };

    let mut client = serenity::Client::builder(config.token, intents)
        .event_handler(handler)
        .await?;
    client.start().await?;
    Ok(())
}

async fn send_reply(ctx: &Context, msg: &Message, body: String) {
    let body = truncate_discord_message(&body);
    if let Err(err) = msg.channel_id.say(&ctx.http, body).await {
        eprintln!("failed to send Discord reply: {err}");
    }
}
