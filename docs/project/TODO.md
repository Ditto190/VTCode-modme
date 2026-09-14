Improve apply/edit patch UI

1. Change the apply/edit patch UI to be more intuitive.
2. style and syntax highlight the file and diff color on the edit patch
3. reason code kept stable; renderers show `diff_preview_user_message`
4. Improve and streamline the overall UI of file ops UI/UX in the TUI
5. Follow design system

---

https://github.com/mitsuhiko/similar

also check https://deepwiki.com/search/how-codex-implement-diff-previ_17050125-2468-4dce-94da-0a0ea6c0af6b?mode=fast and refer to the implementation details for rewrite the diff preview UI. and also consider open source it.

===

improve git diff preview UI '/Users/vinhnguyenxuan/Documents/vtcode-resources/idea/Screenshot 2026-09-14 at 14.26.27.png' it too dimmed.

====

open source vtcode-diff

---

fix:

vtcode man
.ie \n(.g .ds Aq \(aq
.el .ds Aq '
.TH VTCODE 1 2026-09-14 "VT Code" "User Commands"
.SH NAME
vtcode \- Advanced coding agent with Decision Ledger
.SH SYNOPSIS
\fBvtcode\fR [\fBOPTIONS\fR] [\fBCOMMAND\fR] [\fBARGS\fR]
.SH DESCRIPTION
VT Code is an advanced coding agent with single\-agent architecture and Decision Ledger that provides intelligent code generation, analysis, and modification capabilities. It supports multiple LLM providers including Gemini, OpenAI, Anthropic, DeepSeek, Meta AI, Z.AI, Moonshot AI, OpenRouter, Merge Gateway, NVIDIA NIM, Vercel AI Gateway, and Ollama, and includes LLM\-native semantic code understanding. Rust, Python, JavaScript, TypeScript, Go, and Java.
.SH OPTIONS
.TP
\fB\-m\fR, \fB\-\-model\fR \fIMODEL\fR
Specify the LLM model to use (default: gemini\-3\-flash\-preview)
.TP
\fB\-p\fR, \fB\-\-provider\fR \fIPROVIDER\fR
Specify the LLM provider (gemini, openai, anthropic, deepseek, meta, zai, moonshot, openrouter, merge\-gateway, nvidia, vercel, ollama, lmstudio)
.TP
\fB\-\-workspace\fR \fIPATH\fR
Set the workspace root directory for file operations
.TP
\fB\-\-performance\-monitoring\fR
Enable performance monitoring and metrics
.TP
\fB\-\-research\-preview\fR
Enable research\-preview features
.TP
\fB\-\-debug\fR
Enable debug output
.TP
\fB\-\-verbose\fR
Enable verbose logging
.TP
\fB\-h\fR, \fB\-\-help\fR
Display help information
.TP
\fB\-V\fR, \fB\-\-version\fR
Display version information
.SH COMMANDS
.TP
\fBchat\fR
Start interactive AI coding assistant
.TP
\fBask\fR \fIPROMPT\fR
Single prompt mode without tools
.TP
\fBperformance\fR
Display performance metrics and system status
.TP
\fBbenchmark\fR
Run SWE\-bench evaluation framework
.TP
\fBcreate\-project\fR \fINAME\fR \fIFEATURES\fR
Create complete Rust project with features
.TP
\fBinit\fR
Guided AGENTS.md and workspace setup
.TP
\fBman\fR \fICOMMAND\fR
Generate or display man pages for commands
.TP
\fBcheck\fR \fISUBCOMMAND\fR
Run built\-in repository checks
.TP
\fBacp\fR
Start Agent Client Protocol bridge for IDE integrations
.TP
\fBchat\-verbose\fR
Verbose interactive chat with enhanced transparency
.TP
\fBperformance\fR
Display performance metrics and system status
.TP
\fBtrajectory\fR
Pretty\-print trajectory logs and show basic analytics
.TP
\fBbenchmark\fR
Benchmark against SWE\-bench evaluation framework
.TP
\fBcreate\-project\fR \fIname\fR \fIfeatures\fR
Create complete Rust project with advanced features
.TP
.TP
\fBrevert\fR \fIturn\fR
Revert agent to a previous snapshot
.TP
\fBsnapshots\fR
List all available snapshots
.TP
\fBcleanup\-snapshots\fR
Clean up old snapshots
.TP
\fBinit\fR
Initialize project with enhanced dot\-folder structure
.TP
\fBinit\-project\fR
Initialize project with dot\-folder structure
.TP
\fBconfig\fR
Generate configuration file
.TP
\fBtool\-policy\fR
Manage tool execution policies
.TP
\fBmcp\fR
Manage Model Context Protocol providers
.TP
\fBmodels\fR
Manage models and providers
.SH EXAMPLES
Start interactive chat:
\fB vtcode chat\fR
Ask a question:
\fB vtcode ask "Explain Rust ownership"\fR
Create a web project:
\fB vtcode create\-project myapp web,auth,db\fR
Generate man page:
\fB vtcode man chat\fR
Run ast\-grep checks for the current workspace:
\fB vtcode check ast\-grep\fR
.SH ENVIRONMENT
.TP
\fBGEMINI_API_KEY\fR
API key for Google Gemini (default provider)
.TP
\fBOPENAI_API_KEY\fR
API key for OpenAI GPT models
.TP
\fBANTHROPIC_API_KEY\fR
API key for Anthropic Claude models
.TP
\fBDEEPSEEK_API_KEY\fR
API key for DeepSeek models
.TP
\fBMETA_API_KEY\fR
API key for Meta AI Muse models
.TP
\fBMODEL_API_KEY\fR
Meta AI\*(Aqs documented API key variable
.TP
\fBZAI_API_KEY\fR
API key for Z.AI GLM models
.TP
\fBMOONSHOT_API_KEY\fR
API key for Moonshot AI Kimi models
.TP
\fBOPENROUTER_API_KEY\fR
API key for OpenRouter models
.TP
\fBNVIDIA_API_KEY\fR
API key for NVIDIA NIM models
.TP
\fBMERGE_GATEWAY_API_KEY\fR
API key for Merge Gateway routes
.TP
\fBMERGE_GATEWAY_BASE_URL\fR
Optional Merge Gateway endpoint override; /v1/openai selects legacy compatibility
.TP
\fBAI_GATEWAY_API_KEY\fR
API key for Vercel AI Gateway models
.TP
\fBVERCEL_AI_GATEWAY_BASE_URL\fR
Optional Vercel AI Gateway endpoint override (default: https://ai\-gateway.vercel.sh/v1)
.SH FILES
.TP
\fBvtcode.toml\fR
Configuration file (current directory or the canonical user config directory)
.TP
\fB.vtcode/\fR
Project cache and context directory
.SH SAFETY
.TP
apply_patch: reserve for reviewed diffs or small batches. For large refactors or critical files, stage local backups and prefer edit_file/write_file to avoid partial rewrites if a patch fails.
.TP
Timeout governance: tune [timeouts] in vtcode.toml to clamp tool duration. VT Code warns once execution passes the configured warning threshold so you can cancel runaway commands.
.SH "SEE ALSO"
Full documentation: https://github.com/vinhnx/vtcode
Related commands: cargo(1), rustc(1), git(1)

===

14:41:59 ~/developer/learn-by-doing/vtcode main\* ⇡
❯ vtcode analyze
Analyzing workspace...

1. Getting workspace structure...
   Failed to list root directory: tool denied by policy
2. Identifying project type...
3. Reading project configuration...
   Read Read AGENTS.md (null bytes)
   Read Read README.md (null bytes)
   Read Read Cargo.toml (null bytes)
   Read Read package.json (null bytes)
4. Analyzing source code structure...
   Deep analysis: use grep/search tools for detailed code inspection.
   Workspace analysis complete!
   You can now ask me specific questions about the codebase.

===

❯ vtcode trajectory --last
error: unexpected argument '--last' found

tip: to pass '--last' as a value, use '-- --last'

Usage: vtcode trajectory [OPTIONS] [WORKSPACE]

For more information, try '--help'.

===

Implement keyboard shortcut enhancements in chat input text field:

1. command (macos:) or similar key on other platforms: a/ command+backspace should clear all text input in a single line of multi-line chat input. if it is single-line chat input, it should clear the entire input. also applied for compact [pasted content ... chars] block and image block.
2. command+left or command+right should move the cursor to the beginning or end of the current line in multi-line chat input. if it is single-line chat input, it should move the cursor to the beginning or end of the entire input.
3. if the cursor focus is active in chat input, pressing the double-escape key should clear the current line in multi-line chat input or the entire input in single-line chat input.
