# AGENTS.md

Tauri 2 + Nuxt 4 desktop app that launches AI coding agents (OpenCode, Codex) pre-configured for the AI Hive gateway. Frontend in `app/`, Rust backend in `src-tauri/`.

## Commands

- Use **Bun** exclusively (`packageManager` is bun; CI runs `bun install --frozen-lockfile`).
- `bun install` first — `postinstall` runs `nuxt prepare`, which generates `.nuxt/`; `lint` and `typecheck` fail without it (ESLint imports its base config from `.nuxt/eslint.config.mjs`).
- `bun run tauri dev` — full app (Nuxt on :3001 + Tauri window).
- `bun run dev` — web-only dev; Tauri features auto-degrade.
- `bun run lint`, `bun run typecheck` — what CI runs (`.github/workflows/ci.yml`). There is no JS test suite.
- Rust tests: `cargo test` from `src-tauri/` (unit tests live in `src-tauri/src/commands.rs`).

## Architecture

- Single page (`app/pages/index.vue`) + `app/composables/useHive.ts` wrapping all `invoke()` calls. All backend logic is in `src-tauri/src/commands.rs`; commands are registered in `lib.rs`.
- Nuxt is SSG (`ssr: false`); the app source dir is `app/` (Nuxt 4 layout), not a root-level `pages/`.
- Frontend detects Tauri at runtime via `__TAURI_INTERNALS__` (`useHive.ts`) and must keep working in plain web mode — follow this guard when adding `invoke` calls.
- Tauri dev server is pinned to port 3001 (`strictPort` in `nuxt.config.ts`, `devUrl` in `tauri.conf.json`). Don't change one without the other.
- `nuxt.config.ts` ignores `**/src-tauri/**` (EMFILE watch workaround) — don't remove.
- Hive key is stored as plaintext `hive.json` in the Tauri app config dir (`.bak` written before overwrite); model list comes from `GET https://hive.requrv.ai/api/v1/models`.
- Launch flows: OpenCode gets a `requrv-hive` provider merged into `~/.config/opencode/opencode.jsonc` (existing fields preserved, JSONC comments stripped, backup made); Codex gets `~/.codex/hive.config.toml` + `--profile hive` and is only launched after probing the gateway's `/responses` endpoint (Codex ≥ 0.136 requires it).

## Conventions

- User-facing strings (Rust error messages, UI copy, toasts) are in **Italian** — keep new user-facing text in Italian.
- Rust code is indented with **2 spaces** (matches `.editorconfig`); do NOT run `cargo fmt` — it would reformat everything to 4 spaces.
- Frontend style is enforced by ESLint (`@nuxt/eslint` stylistic): no semicolons, single quotes, 2 spaces, no trailing commas. There is a `.prettierrc` but no format script; `bun run lint` is the check.
- Version is duplicated in `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` — bump both together. Releases are automatic: pushing a `v*` tag triggers `.github/workflows/build.yml` (tauri-action builds macos/linux/windows and publishes a GitHub Release; the tag comes from the version in `tauri.conf.json`).
