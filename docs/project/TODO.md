Perform a detailed audit of the vtcode modes and subagents, focusing specifically on the 'plan', 'build', and 'auto' modes. Identify any existing blockers, technical issues, or workflow inefficiencies. Propose an optimized system architecture and outline a validation process to ensure that each mode functions correctly and achieves its intended objective.

==

Analyze and improve the agent's budget management logic to address the issue where the system forces a "final tool-free synthesis pass" even when the agent attempts to summarize and terminate. This current behavior leads to suboptimal results because the agent is compelled to continue without sufficient resources. Propose an adaptive mechanism that grants the agent the autonomy to decide its next action when the budget is nearly exhausted, such as concluding the task, requesting additional resources, adjusting its plan, or using a /compact command to optimize its remaining capacity.

```

     Would you like me to proceed with the implementation? The tool budget
     was exhausted during exploration, but I have all the context needed to
     implement this cleanly.
  ---------------------------------- Info -----------------------------------
    Turn budget nearly exhausted; forcing a final tool-free synthesis pass.
  ---------------------------------------------------------------------------
The system is telling me to execute the next concrete action. Let me
implement the changes now. I have enough context from my exploration. Let me
start implementing:
```

logs:

/Users/vinhnguyenxuan/Developer/learn-by-doing/vtcode/.vtcode/checkpoints/turn_480.json

/Users/vinhnguyenxuan/.vtcode/sessions/atif-trajectory-session-vtcode-20260620T055949Z_873902-59976-20260620T064540Z.json

---

Here's the synthesis and plan for task 4.
Summary of Findings
Task 4: "Centralize advertisement snapshot and add action-level masking
for multi-action tools"
Current Architecture
• SessionToolCatalogSnapshot (harness_kernel.rs) is the tool definition
snapshot sent to the LLM
• filter_tool_definitions_for_mode filters at whole-tool level - tools
are either fully exposed or fully hidden
• Multi-action tools (unified_file, unified_exec) use ToolMutationModel:
:ByArgs(classifier) - their mutation status depends on runtime args
• In planning mode, ByArgs tools pass the filter (they're not Mutating),
so the entire tool schema including write/edit/delete actions is
advertised to the LLM

     The Problem
     The LLM sees all actions (read, write, edit, patch, delete, move, copy)
     for unified_file in planning mode, even though only read is allowed.
     This creates a policy hole: the tool schema lies about what's available.
     Implementation Plan
     1. Add action-level masking function in tool_intent.rs:
       1  /// Returns read-only actions for a multi-action tool in planning

mode.
2 pub fn planning*allowed_actions(tool_name: &str) -> Option<&'static
[&'static str]> {
3 match tool_name {
4 tools::UNIFIED_FILE => Some(&["read"]),
5 tools::UNIFIED_EXEC => Some(&["run", "poll", "list", "
inspect", "continue"]),
6 * => None,
7 }
8 }
9

     2. Add schema masking function in harness_kernel.rs:
       1  fn mask_tool_actions_for_mode(
       2      tool: &ToolDefinition,
       3      planning_active: bool,
       4  ) -> ToolDefinition {
       5      if !planning_active { return tool.clone(); }
       6      let Some(name) = tool.function.as_ref().map(|f| f.name.as_str()

) else { return tool.clone(); };
7 let Some(allowed) = tool_intent::planning_allowed_actions(name)
else { return tool.clone(); };
8 // Filter the "action" enum in the JSON schema parameters
9 // to only include allowed actions
10 // ... modify tool.parameters.properties.action.enum
11 }
12 3. Integrate into filter_tool_definitions_for_mode:
Apply mask_tool_actions_for_mode to each tool after the whole-tool
filter, so the returned definitions have masked action enums. 4. Update SessionToolCatalogSnapshot to track action masking applied.

Files to Modify 1. vtcode-core/src/tools/tool_intent.rs - add planning_allowed_actions() 2. vtcode-core/src/core/agent/harness_kernel.rs - add
mask_tool_actions_for_mode(), integrate into
filter_tool_definitions_for_mode 3. Tests in both files

===

Investigate and resolve a regression where the program fails to exit upon receiving repeated Ctrl+C (SIGINT) signals. Implement a robust signal handler that intercepts these interrupts to facilitate a graceful shutdown. The solution should include setting a termination flag to halt ongoing processes and executing all necessary cleanup routines before the program exits, ensuring the agent remains responsive to user interrupt requests and does not become stuck in an indefinite running state.

===

Develop a technical architecture and implementation plan to transform the current provider-specific `/compact` command into a unified, provider-agnostic feature. Currently, compaction is only available for the native OpenAI provider on api.openai.com. The goal is to abstract the compaction logic so that the command functions consistently across all LLM models and backends, including all providers. Your response should include an architectural design for the abstraction layer, strategies for integrating various provider APIs, and a method for ensuring manual user triggers work universally across all environments.
