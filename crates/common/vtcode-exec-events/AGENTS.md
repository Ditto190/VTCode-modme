# vtcode-exec-events
[Root AGENTS.md](../AGENTS.md) | Authoritative `ThreadEvent` contract. All runtime events flow through this crate.

## Key Types

`ThreadEvent` enum — the single event type (serde-tagged) | `VersionedThreadEvent` wrapper with schema version | `EventEmitter` trait | `Usage` token accounting | `ThreadItem` + `ThreadItemDetails` item taxonomy | `EVENT_SCHEMA_VERSION` semver string

## ThreadEvent Variants

`thread.started` | `thread.completed` | `thread.compact_boundary` | `context.reset` | `turn.started` | `turn.completed` | `turn.failed` | `turn.blocked` | `item.started` | `item.updated` | `item.completed` | `plan.delta` | `plan.approval.requested` | `plan.approval.resolved` | `error`

## Rules

- **Do not invent parallel event types.** Extend `ThreadEvent` and `ThreadItemDetails` enums.
- `EVENT_SCHEMA_VERSION` must be bumped when the serialized contract changes.
- `EventEmitter` trait has a blanket `FnMut(&ThreadEvent)` impl.
- Feature-gated emitters: `telemetry-log` (LogEmitter), `telemetry-tracing` (TracingEmitter), `schema-export` (JSON Schema), `serde-json` (JSON helpers).
- `atif/` exports ATIF trajectories; `trace/` implements Agent Trace attribution.
- **Keep `ThreadEvent` compact**: large sparse payloads must be `Box`ed (see `thread_event_stays_compact` size-guard test; ≤80 bytes). `Box<T>` is serde/schema transparent.

## Gotchas

- `vtcode-core::exec::events` re-exports these types — consumers should use that path, not depend on this crate directly.
- Plan approval state is `PlanApprovalRequested/Resolved`; keep `PlanApprovalDecision` stable for headless/Open Responses clients. Bounded failures use `ReasoningItem` stage `"diagnosis"`; no parallel variant. `HarnessEventKind` additions need a schema bump.
- Schema history: `0.12.0` blocked-handoff metadata; `0.13.0` `turn.blocked` + fuse counters; `0.14.0` limit-grant harness kinds; keep legacy readable and ATIF stable.
- Schema `0.15.0` adds optional `turn.completed.in_progress_exec_sessions` (bounded to 4) for cross-turn resume; empty omits, missing/null defaults empty, ATIF mirrors non-empty ids in `extra`.
