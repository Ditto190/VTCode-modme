reference and explore research /Users/vinhnguyenxuan/Developer/learn-by-doing/claude-code-main and apply learning to improve vtcode codebase.

==

add https://docs.x.ai/overview

===

https://github.com/astral-sh/hawk

===

reference and support accessibility research and best practices to apply to vtcode codebase.

https://code.claude.com/docs/en/accessibility

===

https://console.cloud.google.com/agent-platform/publishers/google/model-garden/gemini-3.6-flash

https://ai.google.dev/gemini-api/docs/models/gemini-3.6-flash

===

Implementation Plan - Remove request_user_input from Plan Mode & Streamline Planning Workflow
User Review Required
IMPORTANT

This change removes mandatory/forced request_user_input tool calls from Planning Mode. Instead of forcing model-generated or synthetic interactive modal questions via request_user_input, Planning Mode will operate in a clean, robust read-only research -> text plan synthesis flow. Open questions and design choices will be surfaced directly in plain text within <proposed_plan> or the response text, avoiding permanent tool policy denials and modal failures across interactive, non-interactive, and ACP environments.

Proposed Changes
crates/codegen/vtcode-core (Prompts & Contracts)
[MODIFY]
system.rs
Remove PLANNING_WORKFLOW_INTERVIEW_POLICY_LINE (which instructed the model to call request_user_input during planning).
Update planning workflow policy constants to direct the model to do read-only research, finish unblocked planning, and surface any open questions or choices in plain text.
[MODIFY]
runtime_contract.rs
Simplify append_planning_workflow_notice so planning workflow prompt guidance uniformly instructs plain-text question resolution without referencing or requiring request_user_input.
src/agent/runloop/unified (Unified Runloop & Planning Workflow)
[MODIFY]
planning_workflow.rs
[MODIFY]
interview_forcing.rs
[MODIFY]
gating.rs
[MODIFY]
tests.rs
Remove forced request_user_input tool call injection (inject_planning_workflow_interview).
Filter out any request_user_input tool calls attempted during planning mode so they do not trigger runtime failures.
Retain plan presentation, plan quality reminders (PLANNING_WORKFLOW_REMINDER), and transition hints (implement to execute).
Update test suite to verify plan mode operates smoothly without request_user_input.
Verification Plan
Automated Tests
Fast check: ./scripts/check-dev.sh
Unit & integration tests: ./scripts/check-dev.sh --test
Specific planning workflow test suites:
cargo nextest run -p vtcode -E 'test(planning)'
cargo nextest run -p vtcode-core -E 'test(prompts)'
Manual Verification
Verify that entering plan mode (/plan or planning workflow) conducts read-only research, outputs <proposed_plan> with open decisions in plain text, and does not invoke or fail on request_user_input.
