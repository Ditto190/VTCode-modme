# Battery Packs: Curated Tool Collections for VT Code

> Adapted from Graydon Hoare's ["Battery packs: Let's talk about crates, baby"](https://smallcultfollowing.com/babysteps/blog/2026/07/15/battery-packs/) (15 July 2026).

## What Are Battery Packs?

A **battery pack** is a curated set of crates arranged around a common theme. Graydon Hoare proposed this concept to address one of the most common pain points in the Rust ecosystem: the abundance of high-quality crates on crates.io is a strength, but choosing between alternatives is a real cost for newcomers and experienced Rustaceans alike.

Battery packs solve this by providing **opinionated defaults**. They are:

- **Thin abstractions.** You don't depend on the battery pack itself — you depend on the individual crates inside it. Swapping one crate for another never breaks your code.
- **Community-curated.** Anyone can publish a battery pack. The best ones emerge from groups who share your context: the embedded working group publishes an embedded battery pack, the network services working group publishes one for async runtimes, and so on.
- **Evolving by design.** Because they are just lists of recommendations, a battery pack can pivot. If `clap` replaces `docopt` in the CLI pack, your code doesn't change — you just update your dependencies.

## Battery Packs and Coding Agents

VT Code is a Rust coding agent built for long-running autonomous workflows. It pulls together dozens of crates across LLM providers, shell execution, code indexing, TUI rendering, and protocol support. The same problem Hoare identifies for Rust newcomers applies to anyone configuring or extending a coding agent: **which crates should you trust, and how do they fit together?**

A battery pack approach maps naturally onto VT Code's architecture:

| Layer | Current state | Battery pack analogy |
|---|---|---|
| LLM providers | 21+ provider implementations | An `llm-providers` battery pack with recommended providers per use-case (local, cloud, gateway) |
| TUI / UI | Ratatui + custom design system | A `terminal-ui` battery pack with recommended widget and theme crates |
| Code intelligence | ripgrep + ast-grep + tree-sitter | A `code-search` battery pack with curated indexing and search backends |
| Agent protocols | A2A, ACP, ATIF, Open Responses | A `protocols` battery pack with recommended protocol stacks by deployment target |
| Sandboxing | Shell sandbox + command safety | A `safety` battery pack with recommended execution policies and guardrails |

## Applying Battery Packs to VT Code Skills

VT Code already has a **Skills** system (`vtcode-skills`). Skills are the closest thing the project has to battery packs: they are curated bundles of configuration, prompts, and tool wiring that solve a specific task. Hoare's framework suggests we can make the skills system more powerful by borrowing three battery pack primitives:

### 1. Dependencies as Recommendations

Today, a VT Code skill bundles prompts and tool config. A battery-pack-inspired skill could also declare *recommended crate dependencies* for a domain. For example, a `full-stack-rust` skill could recommend:

```toml
# Not a real dependency — just a recommendation manifest
[batteries.cli]
crates = ["clap", "dialoguer", "indicatif"]

[batteries.backend]
crates = ["axum", "sqlx", "tower"]

[batteries.frontend]
crates = ["leptos", "tailwind-rs"]
```

The skill doesn't force these on you. It just says "if you're building this kind of project, here's what the community uses."

### 2. Features as Common Sets

Hoare highlights **features** as a way to designate common sets of crates frequently used together. In VT Code, a skill could expose feature flags that compose tool configurations:

```toml
[features]
default = ["search", "exec"]
cloud-only = ["providers/openai", "providers/anthropic"]
local-only = ["providers/ollama", "providers/llama-cpp"]
embedded-safe = ["no-network", "sandbox-strict"]
```

This maps directly onto the existing `vtcode.toml` feature system and makes skill composition explicit.

### 3. Templates as Recipes

Hoare's CI battery pack uses **templates** to configure GitHub Actions. VT Code skills already support templates (`.vtcode/` scaffolding, AGENTS.md files). The battery pack framing encourages treating these as first-class artifacts — not just config snippets, but reproducible project setups that can be versioned, reviewed, and improved collaboratively.

## Why Battery Packs Matter for VT Code

### Lowering the barrier to entry

VT Code supports 21+ LLM providers, multiple editor integrations (Zed, VS Code, Claude Code), and a growing set of built-in skills. A newcomer should not need to read every provider README to pick one. A battery pack gives them a working default in one command:

```
vtcode skill add recommended-setup
```

### Fostering interoperability

Hoare notes that battery packs create a neutral home for interop work. In VT Code's case, a `protocols` battery pack could define the canonical trait set for A2A message passing, so every implementation speaks the same interface without a central authority mandating it.

### Supporting maintainers

If a working group (or the VT Code community) sponsors a battery pack, there is a clear focal point for funding the crates inside it. The maintainers of `vtcode-llm` or `vtcode-bash-runner` benefit directly from the adoption their battery pack drives.

## Risks and Mitigations

Hoare is honest about the downsides:

| Risk | Mitigation in VT Code |
|---|---|
| Too many battery packs, no clear winner | Curated "official" packs maintained by the VT Code core team; community packs live in a separate namespace |
| Stagnation — the pack locks in a dominant crate | Thin abstraction: skills depend on crates, not on the pack. Users can always swap. |
| Who decides what goes in the pack? | Anyone can publish a pack. The best ones rise through adoption, not decree. |

## A Prototype: `vtcode-skill-pack`

To explore this, we could build a minimal prototype:

```
cargo install vtcode-skill-pack  # hypothetical tool
vtcode sp list                   # show available packs
vtcode sp add cli                # add CLI-oriented skill pack
vtcode sp add local-llm          # add local inference skill pack
```

Each pack is a crate named `vtcode-pack-<name>`. Its `Cargo.toml` dependencies are the recommendations. Its `examples/` are the templates. Its `features` are the common configurations. This is exactly Hoare's design, repurposed for coding agent skills instead of application dependencies.

## Conclusion

Battery packs are not a new dependency manager or a replacement for crates.io. They are a **social layer** on top of the ecosystem: a way for communities to share what they have learned about which crates to use together. VT Code is exactly the kind of project that benefits from this. It sits at the intersection of many crate ecosystems (TUI, LLM, async, sandboxing) and its users face real choice paralysis every time they configure or extend it.

The skills system is the natural home for battery packs in VT Code. By treating skills as curated, composable, and community-publishable — just like Hoare's battery packs — we can give users the defaults they need without sacrificing the flexibility that makes Rust great.

---

*This post is a study note, not a specification. The goal is to start a conversation about how VT Code's skill ecosystem can borrow the best ideas from the battery pack proposal.*
