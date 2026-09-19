## Summary

<!-- What does this PR change? One or two sentences. -->

## Type

- [ ] New connector (agent support)
- [ ] Bug fix
- [ ] Feature / improvement
- [ ] Docs
- [ ] Maintenance / dependencies

## Checklist

- [ ] `bun run lint` passes
- [ ] `bun run typecheck` passes
- [ ] `cargo test` passes (from `src-tauri/`)
- [ ] User-facing strings (UI copy, toasts, Rust error messages) are in Italian
- [ ] New `invoke()` calls are guarded by the `isTauri` check so web mode still works
- [ ] No version bump (`tauri.conf.json` / `Cargo.toml` are maintainer-managed)
- [ ] For new connectors: README.md and AGENTS.md updated in the same PR

## Linked issues

<!-- Closes #123 -->
