---
feature: s3-followups
status: in-progress
updated: 2026-09-28
branch: feat/s3-followups
commits: # leave empty while in progress; fill at delivery
---

# S3 Follow-ups (session-efficiency out-of-scope)

## Report

## [S1] Problem

`session-efficiency` closed empty-session spam, tool-result clearing, search drift, and exit cost. Its [S3] left five items open. Grilled scope (2026-09-28): implement all five, **but keep lean budgets** (no schema/first-request raise).

1. **Compaction** — `DEFAULT_COMPACTION_TRIGGER_RATIO = 0.90` waits until 90% of the window before compacting; research runs at ~1M tokens/turn sit in the expensive zone too long.
2. **Registry** — prior audit found no dead *registered* tools; leftover dead `*_ID` constants were already removed. Need a fresh check and any remaining dead registration.
3. **Budgets** — explicit non-change: keep 2k/6k/8k (HarnessTax lean). Document only.
4. **Auth-env flake** — `cli_harness_failures::print_mode_requires_prompt_or_stdin` fails because `--print` with **no prompt** still requires provider auth (`allow_missing_provider_auth = args.print.is_none()`), so the test dies on “Authentication not found” instead of “No prompt provided”.
5. **Model-picker WIP** — already landed (`8778f177a`); no further code unless a leftover is found.

## [S2] Design

### S2-compaction: trigger earlier

- Lower `DEFAULT_COMPACTION_TRIGGER_RATIO` **0.90 → 0.75** so long research turns compact before they burn the whole window.
- Keep `DEFAULT_COMPACTION_TARGET_THRESHOLD` at 0.50 and `keep_last_messages` at 10.
- Do not change `auto_compaction_threshold_tokens` (stays `None` = ratio-driven) or compaction prompt text.

### S2-registry: dead-tool sweep

- Re-audit `BUILTIN_TOOLS` registrations and workspace-wide tool-ID constants for zero-reference dead code; remove only demonstrably unused items.
- If the audit finds none, record that in the spec Report and ship no registry deletion.

### S2-budgets: keep lean (no code)

- Schema cap stays **2,000**; first-request stays **6,000 / 8,000**. No raise. EXECUTION_POLICY numbers unchanged.

### S2-auth: `--print` without prompt fails before auth

- When `--print` is present with empty/whitespace prompt text **and stdin is a TTY** (no piped prompt possible), `StartupPolicy::allow_missing_provider_auth` is **true**: the command cannot call the model, so auth is irrelevant.
- Piped stdin may still carry a prompt, so empty `--print` with non-TTY stdin still requires auth.
- `build_print_prompt` still returns `No prompt provided…` when there is no piped stdin and no inline text.
- `--print <text>` still requires provider auth (unchanged).
- CLI tests set both `OPENAI_API_KEY` and `MERGE_GATEWAY_API_KEY` (the default OpenAI route's auth env) so the auth preflight accepts the synthetic key.

### S2-model-picker: no-op unless leftover

- Confirmed committed. No further change in this delivery.

## [S3] Out of Scope

- Raising schema/first-request budgets (explicitly rejected).
- Compaction prompt prose rewrites.
- Changing auth priority (keyring vs ChatGPT vs env).
- Windows PTY SIGKILL tests (no Windows CI).

## Tasks

- [ ] T1: compaction trigger ratio 0.75 — acceptance: `DEFAULT_COMPACTION_TRIGGER_RATIO == 0.75`; existing compaction tests pass (covers: S2-compaction)
- [ ] T2: dead-tool sweep — acceptance: any zero-reference tool IDs/registrations removed, or Report records none found (covers: S2-registry)
- [ ] T3: `--print` empty prompt skips auth — acceptance: `print_mode_requires_prompt_or_stdin` passes without credentials; `--print hello` still requires auth; unit test on `StartupPolicy` (covers: S2-auth)
- [ ] T4: docs note budgets stay lean — acceptance: EXECUTION_POLICY or spec records 2k/6k/8k unchanged (covers: S2-budgets)
