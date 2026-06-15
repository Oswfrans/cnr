# CNR Architecture

CNR is the runtime layer above Axial. Meaningful state transitions are Axial events. Runtime files in `.cnr/` are caches, transcripts, worktrees, and other rebuildable execution state.

## Boundary

Axial owns:

- event append and replay
- SQLite event store
- durable artifacts
- claims as durable semantic records

CNR owns:

- goal-oriented CLI
- Crown planner actions
- Reach worker contracts
- executor orchestration
- Connor and Discord command surfaces

Modal is execution only. Discord is projection and command surface only. Connor is an actor in the log, not hidden memory.

