reference and explore research /Users/vinhnguyenxuan/Developer/learn-by-doing/claude-code-main and apply learning to improve vtcode codebase.

==

CRITICAL check why installer and self update doesnt correctly install latest github release version, it install N-1 version. CRITICAL

17:11:48 ~/developer/learn-by-doing/vtcode main
❯ curl -fsSL https://raw.githubusercontent.com/vinhnx/vtcode/main/scripts/install.sh | bash
INFO: VT Code Native Installer

INFO: Detected platform: aarch64-apple-darwin
INFO: Fetched release metadata from GitHub API
⠴ Checking for compatible binaries for aarch64-apple-darwin...
✓ Found compatible version: 0.136.5 (aarch64-apple-darwin)
INFO: Downloading binary...
######################################################################## 100.0%
✓ Downloaded successfully
INFO: Verifying binary integrity...
✓ Checksum verified: b35d0ee0a07d7163e9544e5d168f399f48a08df4f5c5fd23a708b93b2c20f929
INFO: Extracting binary...
INFO: Installing to /Users/vinhnguyenxuan/.local/bin/vtcode...
✓ Binary installed to /Users/vinhnguyenxuan/.local/bin/vtcode

✓ Installation complete!
INFO: VT Code is ready to use

✓ Version check passed: vtcode-core 0.136.4

Authors: Vinh Nguyen <vinhnguyen2308@gmail.com>
Config directory: /Users/vinhnguyenxuan/Library/Application Support/com.vinhnx.vtcode
Data directory: /Users/vinhnguyenxuan/Library/Application Support/com.vinhnx.vtcode

Environment variables:
VTCODE_CONFIG - Override config directory
VTCODE_DATA - Override data directory

INFO: Installing search tools (ripgrep + ast-grep)...
→ Installing the optional search tools bundle
✓ ripgrep already available: ripgrep 15.1.0

features:+pcre2
simd(compile):+NEON
simd(runtime):+NEON

PCRE2 10.45 is available (JIT is available)
✓ ast-grep already available: ast-grep 0.44.0
→ Binary: /Users/vinhnguyenxuan/.vtcode/bin/ast-grep
→ Source: VT Code-managed
→ For a local repository, run `vtcode init` to materialize `sgconfig.yml`, `rules/`, and `rule-tests/`.
→ Then run `vtcode check ast-grep`.
✓ search tools installed

✓ Next steps:
INFO: vtcode init # scaffold project config and AGENTS.md
INFO: vtcode # launch interactive TUI
INFO: vtcode --help # see all commands

INFO: Docs: https://vtcode.dev/docs

17:11:56 ~/developer/learn-by-doing/vtcode main 8s
❯

17:11:56 ~/developer/learn-by-doing/vtcode main
❯ v --version
vtcode-core 0.136.4

Authors: Vinh Nguyen <vinhnguyen2308@gmail.com>
Config directory: /Users/vinhnguyenxuan/Library/Application Support/com.vinhnx.vtcode
Data directory: /Users/vinhnguyenxuan/Library/Application Support/com.vinhnx.vtcode

Environment variables:
VTCODE_CONFIG - Override config directory
VTCODE_DATA - Override data directory
