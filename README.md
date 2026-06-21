# Crown & Reach

CNR means Crown & Reach: a durable coordination runtime for multiplayer software work

- Crown: sparse, high-capability planning and adjudication.
- Reach: cheap, parallel execution workers.
- Connor: chat-facing operator interface.
- Axial: append-only durable event spine.

Axial remains separate. CNR depends on Axial for durable events, claims, artifacts, and replay.

## v0 CLI

```sh
cnr init
cnr goal "Fix flaky onboarding tests"
cnr status <goal-id>
cnr log
cnr claims <subject>
cnr loop <goal-id> --cycles 3
cnr ultrawork <goal-id>
```

The current implementation is intentionally small: events are appended to Axial, projections rebuild from Axial, claims are parsed from worker output, and the manager loop emits structured planning and dispatch events.

See [docs/LOCAL_SETUP.md](docs/LOCAL_SETUP.md) for machine setup, local smoke
tests, Discord connection inputs, and Modal status.
