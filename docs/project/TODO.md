change modes switching order, Build → Duck → Plan → Auto (auto last/most guarded); add trust gate to interactive auto; fix lifecycle tool policy hole; unify plan concept; unify prompt placement; centralize advertisement snapshot and add action-level masking for multi-action tools.

===

Perform a detailed audit of the vtcode modes and subagents, focusing specifically on the 'plan', 'build', and 'auto' modes. Identify any existing blockers, technical issues, or workflow inefficiencies. Propose an optimized system architecture and outline a validation process to ensure that each mode functions correctly and achieves its intended objective.

===

All 6 audit tasks are complete. Here is the comprehensive audit report:
――――――――――――――――――――――――――――――――
VT Code Modes and Subagents Audit Report 1. Architecture Overview
Three Primary Modes:
Mod │ Agent │ Permission │ Purpose
e │ │ Default │
────┼─────────────────────┼─────────────────┼───────────────────────────
───
Pla │ builtin_plan_agent │ N/A (read-only │ Gather context, analyze
code,
n │ (read-only) │ enforced) │ produce plans
Bui │ builtin_primary_bui │ Ask │ Interactive editing with
user
ld │ ld_agent │ │ confirmation
Aut │ builtin_primary_aut │ Deny │ Autonomous execution
within
o │ o_agent │ │ permission policy
Mode Resolution: active_primary_agent_from_specs_for_mode() in
src/agent/runloop/unified/session_setup/init.rs resolves the active agent
from discovered specs, with full_auto flag selecting between interactive
and autonomous paths.
Subagent System: Discovery via discover_subagents() from .vtcode/,
.claude/, .codex/, and user-level sources. SubagentController manages
spawn lifecycle with spawn_with_spec() and spawn_background_subprocess().
Tool routing through ToolRegistry + SafetyGateway + DeferredToolPolicy.

     ――――――――――――――――――――――――――――――――
     2. Blockers and Technical Issues
     B1 - Mode Switching Order: Current precedence needs reordering to Build

-> Duck -> Plan -> Auto (Auto last/most guarded). The TODO.md already
identifies this.
B2 - Missing Trust Gate for Interactive Auto: SafetyTrustLevel enum
exists (Untrusted, Standard, Elevated, Full) but is not used to gate
interactive auto mode transitions. Auto mode should require Elevated or
Full trust level.
B3 - Lifecycle Tool Policy Hole: DeferredToolPolicy exists but has edge
cases during mode transitions where tool policies may not be consistently
enforced. The note*explicit_refresh calls in planning_workflow_facade.rs
suggest cache invalidation is needed but may not cover all transition
paths.
B4 - Fragmented Planning Concept: Planning workflow state is split
across:
• vtcode-core/src/tools/registry/planning_workflow_facade.rs (
ToolRegistry methods)
• vtcode-core/src/tools/handlers/planning_workflow.rs (
PlanningWorkflowState, PlanLifecyclePhase)
• vtcode-core/src/tools/safety_gateway.rs (planning_active flag)
• src/codex_app_server/runtime.rs (normalize_planning_input,
should_switch_to_execution_mode)
B5 - Confusing Auto->Build Fallback: When the auto agent spec is missing
, resolve_full_auto_primary_agent_runtime_from_specs() falls back to the
build agent. This silent fallback could confuse users who expect auto
behavior but get interactive build behavior.
―――――――――――――――――――――――――――――――― 3. Workflow Inefficiencies
I1 - Duplicated Planning Logic: normalize_planning_input() in runtime.rs
duplicates transition logic that could be centralized in
PlanningWorkflowState.
I2 - Fragmented Tool Catalog Cache Invalidation: note_explicit_refresh()
is called in multiple places (planning_workflow_facade.rs, safety gateway)
rather than through a single transition coordinator.
I3 - Disconnected Prompts: BUILTIN*\*\_AGENT constants in subagents.rs are
raw strings disconnected from the prompt assembly system, requiring manual
synchronization.
I4 - Action-Level Masking Missing: Multi-action tools (like
unified_search with grep/list/structural actions) need action-level
masking for planning mode, but current filtering is tool-level only.

     ――――――――――――――――――――――――――――――――
     4. Proposed Optimized Architecture
     A1 - Reorder Mode Precedence: Implement Build -> Duck -> Plan -> Auto
     with explicit trust gates at each transition.
     A2 - Centralize PlanningWorkflowState: Make PlanningWorkflowState the
     single source of truth for all planning-related decisions. Remove the
     duplicate planning_active flag from SafetyGateway.
     A3 - Add Trust Gate for Auto: Use SafetyTrustLevel to gate auto mode.
     Require Elevated trust level for interactive auto, Full for full-auto.
     A4 - Unify Cache Invalidation: Create a ModeTransitionCoordinator that
     handles tool catalog cache invalidation on all mode transitions.
     A5 - Centralize Transition Logic: Move normalize_planning_input() and
     should_switch_to_execution_mode() into PlanningWorkflowState as methods.
     A6 - Add Action-Level Masking: Extend tool filtering to support action-
     level masking for multi-action tools.

―――――――――――――――――――――――――――――――― 5. Validation Process
Plan Mode Validation: 1. Verify start_planning enables read-only mode 2. Verify mutating tools (unified_file writes, apply_patch, unified_exec
runs) are blocked 3. Verify finish_planning transitions to execution mode 4. Verify tool catalog cache is invalidated on transition
Build Mode Validation: 1. Verify interactive confirmations work for destructive operations 2. Verify tool permissions respect PermissionDefault::Ask 3. Verify subagent spawning respects mode permissions
Auto Mode Validation: 1. Verify full_auto flag bypasses confirmations 2. Verify trust gate enforcement (requires Elevated/Full trust) 3. Verify fallback behavior when auto spec is missing (should warn, not
silently fallback)
Cross-Mode Validation: 1. Verify tool catalog cache invalidation on all mode transitions 2. Verify subagent spawning respects mode permissions 3. Verify planning workflow state is consistent across all access paths
――――――――――――――――――――――――――――――――
Next open decision: Whether to implement the architecture changes
incrementally (each fix as a separate PR) or as a single coordinated
refactor.
