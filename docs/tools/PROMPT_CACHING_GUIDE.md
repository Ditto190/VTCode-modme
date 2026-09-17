# Prompt Caching Guide

The harness treats a cache prefix as an immutable segment. Tool definitions use
deterministic core-first ordering followed by tool name and type, and the
serialized catalog is reused for the segment. Catalog expansion through tool
discovery takes effect at the next segment boundary.

Prompt caching lets VT Code reuse validated conversation prefixes across providers to reduce latency and token consumption. This guide explains how to configure the feature globally and fine-tune the per-provider behaviour exposed in `vtcode.toml`.

## Global Settings

All prompt caching controls live under the `[prompt_cache]` section in `vtcode.toml`.

| Key                     | Type    | Description                                                                                               |
| ----------------------- | ------- | --------------------------------------------------------------------------------------------------------- |
| `enabled`               | bool    | Master switch for the caching subsystem. When disabled, per-provider overrides are ignored.               |
| `cache_dir`             | string  | Path (supports `~`) where cache entries are persisted. Relative paths resolve against the workspace root. |
| `max_entries`           | integer | Maximum entries persisted on disk before rotation.                                                        |
| `max_age_days`          | integer | Maximum age of an entry before automatic eviction.                                                        |
| `enable_auto_cleanup`   | bool    | If `true`, stale entries are purged during startup and shutdown.                                          |
| `min_quality_threshold` | float   | Minimum quality score a completion must meet before it is cached.                                         |
| `cache_friendly_prompt_shaping` | bool | Default-on prompt shaping that keeps volatile runtime context at the end of system prompts for better cache-prefix reuse. |

## Provider Overrides

Each provider exposes an override block under `[prompt_cache.providers]`. Overrides are only honoured when both the global `enabled` flag and the provider-level `enabled` flag are `true`.

### OpenAI

```toml
[prompt_cache.providers.openai]
enabled = true
min_prefix_tokens = 256
idle_expiration_seconds = 3600
surface_metrics = true
```

-   `min_prefix_tokens` — minimum number of prompt tokens before the API is asked to cache the prefix.
-   `idle_expiration_seconds` — how long (in seconds) a cached prefix can remain idle before expiry.
-   `surface_metrics` — when enabled, OpenAI usage responses expose cache-hit statistics surfaced through VT Code’s usage telemetry.
-   `prompt_cache_retention` — optional OpenAI Responses API retention policy for cached prefixes. Supported values are `"in_memory"` and `"24h"`.
-   Default: `None` (opt-in) - VT Code does not set prompt_cache_retention by default; so OpenAI keeps its default `in_memory` behavior unless you opt in explicitly.
-   GPT-5.6-family Responses requests additionally send `prompt_cache_options: {"ttl": "30m"}` by default (currently the only TTL OpenAI accepts for these models), declaring cache intent explicitly instead of relying on the implicit default alone. An explicit catalog TTL or a configured `prompt_cache_retention` takes precedence where applicable.
-   Example CLI override to enable 24h retention for Responses model:

    ```bash
    vtcode --model gpt-5 --config prompt_cache.providers.openai.prompt_cache_retention=24h ask "Explain this function"
    ```

-   To list all Response-API-enabled OpenAI models:

    ```bash
    vtcode models list --provider openai
    ```

-   Applies only to OpenAI models that use the Responses API; for other models this value is ignored.

## Prefix Stability Rules

Prompt caching on Responses-style providers only hits when the new request keeps an exact prefix match. In VT Code, the most common cache breakers are:

-   Changing `model`, `tools`, or sandbox/environment instruction blocks mid-session.
-   Reordering tools between requests.
-   Injecting new dynamic context above existing prompt items.
-   Putting a per-second timestamp in the static system prompt.
-   Sending compaction/summarization as a fresh single-message prompt instead of forking the parent prefix.

To reduce avoidable misses, VT Code keeps tool ordering deterministic and defers MCP `tools/list_changed` refreshes to turn boundaries so an active turn sees a stable tool catalog.
VT Code enables `prompt_cache.cache_friendly_prompt_shaping = true` by default. When it is enabled, VT Code applies provider-aware shaping:

- OpenAI, Gemini, DeepSeek, OpenRouter, Moonshot, Z.AI: move volatile counters to a trailing `[Runtime Context]` block.
- Anthropic and MiniMax: same trailing runtime block, plus Anthropic-format system prompt splitting so runtime context is sent as an uncached block.

### Static first, dynamic last

The harness lays out every prompt so stable pieces stay cached and only the conversation grows turn by turn:

1.  Static system prompt and tool definitions (globally cached).
2.  Project instruction layers (`AGENTS.md` / `CLAUDE.md`, cached within a project).
3.  Session-stable routing (`## Skills` before the more volatile `## Active Tools`).
4.  Dynamic suffix: `## Environment` (date-only, never clock time), planning/full-auto notices, `[Harness Limits]`, `[Runtime Tool Catalog]`, conversation messages.

Planning and full-auto transitions are runtime mode changes: they are conveyed in the uncached dynamic suffix (and gated at execution), never by rewriting the stable prefix or swapping the tool set mid-segment. A planning toggle ends the current cache-stable segment once by design (logged as `planning_workflow_enabled/disabled`); the next request re-establishes the prefix and subsequent turns hit again.

Temporal context in the system prompt is date-only (`- Date:`). Precise clock time belongs in a `<system-reminder>` history message on turns that actually need it, so the cached prefix stays stable all day.

### Cache-safe compaction forking

When the context window fills, compaction forks the cached call: the request reuses the parent segment's exact system prompt, ordered tools, and conversation prefix, with only the compaction instruction appended as the new final turn. The summary is then installed and a new immutable segment begins (`thread.compact_boundary` records the before/after prefix and catalog hashes). Standalone single-message summarization (no parent prefix) is only a fallback and pays full input cost.

All local summarization paths fork this way: flat single-pass summaries, hierarchical abstract/detail band requests (which reuse the parent system/tools prefix with `tool_choice: none`), and prefire two-pass pass-2 requests. Native inline compaction (`compact_20260112`) likewise attaches the parent system/tools prefix with `tool_choice: none`, and its local fallbacks forward the parent so a rejected inline attempt does not lose the fork. Provider-native standalone compaction (OpenAI `/responses/compact`) is server-side and needs no fork.

### Cache health monitoring

Beyond per-event advisories (reasoning-effort changes, idle-gap expiry, planning transitions), both runloops feed every turn's normalized usage into a shared session health monitor (`core::agent::cache_health::PromptCacheHealthMonitor`). Turns without provider cache metrics or below 1,024 input tokens are ignored as noise. Two session-scoped alerts fire at most once each, via `tracing::warn` plus the runloop's user-warning channel:

-   **Sustained misses** — 3 consecutive measured turns each reusing under 50% of cache. Indicates the session is re-paying full input cost turn after turn.
-   **Low hit rate** — after 8 measured turns, the cumulative hit rate is under 25%.

Either alert names the likely causes to check: prompt/tool-catalog churn (model switches, MCP refreshes, planning toggles) or idle gaps expiring the provider cache.

### Mid-session model switches

Prompt caches are unique per model: switching rebuilds the cache at full input cost even when the rest of the prefix is unchanged, so it is the most expensive single cache event in a session. Both runloops emit a one-line advisory on the first request carrying a new model (headless: tracing warning plus session warning; interactive: tracing warning plus renderer warning line), mirroring the existing reasoning-effort-change advisory. Prefer resolving the model up front; when a switch is unavoidable (e.g. escalation or `/model`), expect one full-price request before hits resume. For cheap exploratory work, prefer delegating to a subagent on the cheaper model with a handoff summary over switching the main session's model.

### Why mode content stays in the prompt (triage note)

Two further article prescriptions were researched and deliberately deferred:

-   **Planning/full-auto via `<system-reminder>` messages instead of system-prompt sections.** Snapshots are deterministic per mode, so the wire prefix is already stable turn-to-turn within a mode on every provider; the only residual cost is the single toggle-transition miss (zero on Anthropic, where the wire split keeps the cached stable prefix across the toggle). Moving the full planning contract — read-only enforcement, plan-quality spec, research floor — out of the system prompt risks planning behavior with no eval to verify, and would require reworking the prompt/catalog alignment guard that pins the interview-policy line. Revisit only with eval coverage for planning adherence.
-   **Always exposing the full tool catalog (no planning filtering).** Same steady-state analysis: deterministic per-mode filtering means no per-turn churn today; always-expose would save only the toggle-transition miss while showing mutating tools on every planning turn, trading rare one-time savings for per-turn model-confusion risk and denied-call waste across all supported models (including small ones). The fail-closed execution gate stays as the safety net, and tool hiding stays as defense-in-depth. Revisit only with eval evidence that target models obey read-only instructions reliably when mutating tools are visible.

OpenAI additionally keeps `prompt_cache_key` stable per session (unless `prompt_cache_key_mode = "off"`). The wire key is exactly `vtcode:openai:{session_id}` (namespaced per gateway); per-turn capability/catalog hashes are tracked separately in `tool_catalog_hash` / `system_prompt_prefix_hash` and never mixed into the routing key, since OpenAI requires the key to stay consistent across requests sharing a prefix.

VT Code surfaces prompt-cache churn using the same terminology in `vtcode trajectory` and `/share-log` exports:

- `stable_prefix` means the cache-friendly stable system prefix changed while the model stayed the same.
- `tool_catalog` means the tool-definition hash changed.
- `stable_prefix+tool_catalog` means both changed in the same observation.
- `model` means the model changed, so the provider-side cache lineage changed with it.

The share-log HTML overview also reports prompt-cache observations, churn breakdown, and the last visible change reason.

### Request segments and telemetry

Each request prefix is frozen in an immutable `SessionRequestEnvelope` for a
request segment. Compaction starts the next segment before the compacted
history is installed; the old envelope, compacted-history artifact, and
`ThreadCompactBoundary` event remain recoverable. Automatic, manual, recovery,
and model-switch compaction use the same boundary path. A change to the model,
provider, mode, instruction snapshot, or tool-catalog epoch starts a new
segment; unchanged turns reuse the existing envelope byte-for-byte.

The `tool_catalog_cache_metrics` trajectory record includes ordered wire tool
names, catalog/wire/deferred counts, and active loaded-skill names on startup or
when that catalog identity changes. Unchanged turns omit those repeated lists.
`vtcode trajectory` compares the ordered snapshots so a catalog reorder across
starts is visible instead of being hidden by an unchanged tool count.

### Anthropic (Claude)

```toml
[prompt_cache.providers.anthropic]
enabled = true
tools_ttl_seconds = 3600
messages_ttl_seconds = 300
extended_ttl_seconds = 3600
max_breakpoints = 4
cache_system_messages = true
cache_user_messages = true
cache_tool_definitions = true
min_message_length_for_cache = 256
```

-   `tools_ttl_seconds` — TTL for tool definitions and system prompt cache hints.
-   `messages_ttl_seconds` — TTL for user message cache hints.
-   `extended_ttl_seconds` — optional longer-lived TTL. When present, VT Code automatically opts into Anthropic’s extended prompt caching beta header.
-   `max_breakpoints` — maximum number of cache insertion points per request (tools, system prompt, user messages).
-   `cache_system_messages` / `cache_user_messages` / `cache_tool_definitions` — toggle cache hints for each content type.
-   `min_message_length_for_cache` — avoids setting cache hints on very short user messages.

### Gemini

```toml
[prompt_cache.providers.gemini]
enabled = true
mode = "implicit"       # implicit | explicit | off
min_prefix_tokens = 128
explicit_ttl_seconds = 900
```

-   `mode` — `implicit` leverages built-in cache detection; `explicit` reserves cache slots for manual lifecycle management; `off` disables all Gemini caching.
-   `min_prefix_tokens` — minimum prompt size before requesting cache evaluation.
-   `explicit_ttl_seconds` — optional TTL when explicit mode is active.

### OpenRouter
```toml
[prompt_cache.providers.openrouter]
enabled = true
propagate_provider_capabilities = true
report_savings = true
```

-   `propagate_provider_capabilities` — pass provider cache instructions straight through to upstream models.
-   `report_savings` — surface cache-hit metrics returned by OpenRouter alongside standard usage data.

### DeepSeek

```toml
[prompt_cache.providers.deepseek]
enabled = true
```

DeepSeek caches automatically on disk with 64-token prefix units; no wire flags are needed. VT Code sends the system prompt as the leading `messages[0]` system message so the stable prefix starts at token 0 — a requirement for DeepSeek prefix matching. Keep volatile content at the end of the history; any byte difference at the front busts the whole prefix.

### Z.AI

```toml
[prompt_cache.providers.zai]
enabled = true
```

Z.AI handles caching server-side. When the override is enabled, VT Code honors upstream behavior and surfaces usage metrics when available.

## Usage Telemetry

When caching is active, `Usage` structs now include:

-   `cached_prompt_tokens` — tokens served from cache (OpenAI, OpenRouter).
-   `cache_creation_tokens` — tokens spent establishing a new cache entry (Anthropic, OpenRouter).
-   `cache_read_tokens` — tokens satisfied from an existing cache entry (Anthropic, OpenRouter).

These metrics flow through `vtcode-core::llm::types::Usage` and appear anywhere VT Code reports token accounting.

## Validation & Testing

-   Unit tests in `crates/codegen/vtcode-core/src/llm/providers/anthropic.rs` validate cache control insertion and beta header composition.
-   `crates/codegen/vtcode-core/src/llm/providers/openrouter.rs` exercises usage parsing to ensure cache metrics are preserved.
-   Local cache behavior tests in `crates/codegen/vtcode-core/src/core/prompt_caching.rs` verify caching, eviction, and persistence.
-   Configuration loading tests ensure settings from `vtcode.toml` are applied correctly.
-   Run `cargo test` to execute all fast tests after updating configuration logic.

## Implementation Architecture

The prompt caching system is implemented as a multi-layered architecture:

1. **Global Configuration Layer**: Managed in `crates/codegen/vtcode-config/src/core/prompt_cache.rs` with global and per-provider settings
2. **Provider Integration Layer**: Each provider has specific cache control implementation in `crates/codegen/vtcode-core/src/llm/providers/`
3. **Local Caching Layer**: File-based caching engine in `crates/codegen/vtcode-core/src/core/prompt_caching.rs` for optimized prompt storage
4. **Runtime Integration**: Cache configuration flows through the provider factory to ensure proper initialization

## Migration Guide

When upgrading to the new prompt caching system:

1. Add the `[prompt_cache]` section to your `vtcode.toml` if you want to customize caching behavior
2. Review provider-specific settings to optimize for your usage patterns
3. Monitor cache metrics to verify the system is performing as expected

## Troubleshooting

-   If caching isn't working as expected, verify that both global and provider-specific `enabled` flags are set to `true`
-   Check that your prompts meet the minimum token requirements for each provider
-   Enable verbose logging to see cache interaction details

By tuning these values you can balance latency, cost, and cache freshness per provider while keeping the behaviour consistent across the VT Code agent ecosystem.
