# vtcode-diff

[Root AGENTS.md](../../../AGENTS.md) | Reusable bounded diff computation and semantic preview layout.

## Conventions

- Keep the core independent of VT Code crates and renderer themes.
- Preserve source line endings and byte-safe intraline ranges.
- All expensive diff and intraline paths require explicit bounds or deadlines.
- Layout emits semantic rows; applications supply syntax and color policy.

## Dependencies

- `similar` owns diff algorithms and deadline-aware refinement.
- `unicode-width` owns terminal display-cell measurements.
