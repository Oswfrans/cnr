# Local Setup

Install the local Axial and CNR CLIs through Cargo from adjacent checkouts:

```sh
cargo install --path ../axial/crates/axial-cli --force
cargo install --path crates/cnr-cli --force
cargo install --path crates/cnr-discord --force
```

The installed binaries live in Cargo's bin directory, usually `~/.cargo/bin`.
Make sure that directory is on the shell `PATH`.

## Local Smoke Test

Axial:

```sh
tmp=$(mktemp -d /tmp/axial-smoke.XXXXXX)
cd "$tmp"
axial init
goal_output=$(axial goal "Smoke test installed axial CLI")
goal=$(printf '%s\n' "$goal_output" | sed -n '1p')
axial status "$goal"
axial claims "$goal"
axial replay
```

CNR:

```sh
tmp=$(mktemp -d /tmp/cnr-smoke.XXXXXX)
cd "$tmp"
cnr init
goal=$(cnr goal "Smoke test installed cnr CLI")
cnr status "$goal"
cnr manager --goal "$goal" --executor local --workers 1
cnr manager --goal "$goal" --executor modal --workers 1
cnr replay
```

`--executor modal` currently records Modal dispatch intent in Axial. It does
not yet launch remote Modal workers.

## Discord Connection

`cnr-discord` is a small gateway bot. Discord is transport only: commands read
Axial projections or append new Axial events, and Discord stores no durable
workflow truth.

Human steps:

1. Open the Discord Developer Portal.
2. Create or select an application.
3. Add a bot user and copy the bot token.
4. Enable the Message Content Intent for the bot.
5. Invite the bot to the target server with permission to read and send messages.
6. Copy the channel id for the channel CNR should listen in.

Local configuration:

```sh
cp .env.example .env
$EDITOR .env
```

Required values:

```sh
DISCORD_BOT_TOKEN=
DISCORD_CHANNEL_ID=
MODAL_PROFILE=
```

Run the bot from a CNR runtime directory, or pass one with `--root`:

```sh
cnr init
cnr-discord --root .
```

Supported Discord commands:

```text
!cnr help
!cnr goal <title>
!cnr status [goal]
!cnr claims <subject>
!cnr log [limit]
!cnr manager <goal> [local|modal]
!cnr replay
```

## Modal

CNR v0 can record Modal dispatch intent:

```sh
cnr manager --goal <goal-id> --executor modal --workers 1
```

The remaining work is implementing a Modal executor that launches Reach workers
and appends their run lifecycle and `cnr-claims` output back into Axial.
