# AGENTS.md

Tauri 2 + Nuxt 4 desktop app that launches AI coding agents (OpenCode, Codex, Claude Code, Hermes) pre-configured for the AI Hive gateway. Frontend in `app/`, Rust backend in `src-tauri/`.

Open source (GPL-3.0-or-later). To add a new agent connector, follow the step-by-step guide in [CONTRIBUTING.md](./CONTRIBUTING.md).

## Commands

- Use **Bun** exclusively (`packageManager` is bun; CI runs `bun install --frozen-lockfile`).
- `bun install` first — `postinstall` runs `nuxt prepare`, which generates `.nuxt/`; `lint` and `typecheck` fail without it (ESLint imports its base config from `.nuxt/eslint.config.mjs`).
- `bun run tauri dev` — full app (Nuxt on :3001 + Tauri window).
- `bun run build:mac` — signed + notarized macOS release build. Sources the gitignored `.env` (`APPLE_ID`, `APPLE_PASSWORD` app-specific, `APPLE_TEAM_ID`) into the real process env — required because Bun's automatic `.env` loading is invisible to the N-API Rust CLI (`getenv`). The signing identity is pinned in `src-tauri/tauri.conf.json` (`bundle.macOS.signingIdentity`); keep it in sync with the Developer ID certificate.
- `bun run dev` — web-only dev; Tauri features auto-degrade.
- `bun run lint`, `bun run typecheck` — what CI runs for the frontend (`.github/workflows/ci.yml`). There is no JS test suite.
- Rust tests: `cargo test` from `src-tauri/` (unit tests live in `src-tauri/src/commands.rs`). CI also runs `cargo clippy -- -D warnings`.

## Architecture

- Single page (`app/pages/index.vue`) + `app/composables/useHive.ts` wrapping all `invoke()` calls. All backend logic is in `src-tauri/src/commands.rs`; commands are registered in `lib.rs`.
- Nuxt is SSG (`ssr: false`); the app source dir is `app/` (Nuxt 4 layout), not a root-level `pages/`.
- Frontend detects Tauri at runtime via `__TAURI_INTERNALS__` (`useHive.ts`) and must keep working in plain web mode — follow this guard when adding `invoke` calls.
- Tauri dev server is pinned to port 3001 (`strictPort` in `nuxt.config.ts`, `devUrl` in `tauri.conf.json`). Don't change one without the other.
- `nuxt.config.ts` ignores `**/src-tauri/**` (EMFILE watch workaround) — don't remove.
- Hive key is stored as plaintext `hive.json` in the Tauri app config dir (`.bak` written before overwrite); model list comes from `GET https://hive.requrv.ai/api/v1/models`.
- Launch flows: OpenCode gets a `requrv-hive` provider merged into `~/.config/opencode/opencode.jsonc` (existing fields preserved, JSONC comments stripped, backup made); Codex CLI gets `~/.codex/hive.config.toml` + `--profile hive` and is only launched after probing the gateway's `/responses` endpoint (Codex ≥ 0.136 requires it).
- Claude Code is terminal-only: no persistent config, it's launched with env vars (`ANTHROPIC_BASE_URL` = `HIVE_ANTHROPIC_BASE_URL` — note it's the gateway **without** the trailing `/v1`, because the client appends `/v1/messages` — plus `ANTHROPIC_API_KEY`, `ANTHROPIC_MODEL`, `CLAUDE_CODE_MAX_CONTEXT_TOKENS` pinned per model (200k for requrv-small-3.8, 128k default)) and is only launched after probing the gateway's `/messages` endpoint (Anthropic Messages API; the gateway must also accept `role:"system"` in `messages[]`, which Claude Code v2.x sends).
- ChatGPT.app (macOS) is configured by a frontend-driven flow (`configure_chatgpt_app` → restart confirmation → `restart_chatgpt_app`/`open_chatgpt_app`): it rewrites the root of `~/.codex/config.toml` (`model`, `model_provider = "requrv-hive"`, `model_catalog_json` → `~/.codex/hive-models.json`, other keys preserved via the `toml` crate) plus a `[model_providers.requrv-hive]` table (`base_url` = Hive, `wire_api = "responses"`, `supports_websockets = false`, key in `experimental_bearer_token`), sets `~/.codex/auth.json` to apikey mode with the Hive key, keeps one-shot `.hive.bak` backups of both and can fully restore them (`restore_chatgpt_app`, never touches a ChatGPT OAuth auth). The custom provider is mandatory: ChatGPT.app 26.x (Codex core ≥ 0.154) defaults the Responses transport to WebSocket, which the Hive gateway does not implement — built-in providers cannot be overridden, and custom providers ignore `auth.json` (hence the bearer token). Older installs used a root `openai_base_url` instead; detection/restore still recognize and clean that legacy shape. The app reads its model catalog at startup, so a running instance needs the restart confirmation; `launch_service` does NOT handle `("codex","app")` on purpose.
- Hermes (hermes-ide.com) is a desktop app without a CLI: its Agent mode runs the Claude Agent SDK in a Node bridge that inherits the app process environment, so it's launched (like the ChatGPT.app flow, frontend-driven `launch_hermes_app`/`restart_hermes_app`; `launch_service` does NOT handle `("hermes","app")`) with env vars only — `ANTHROPIC_BASE_URL` = `HIVE_ANTHROPIC_BASE_URL`, `ANTHROPIC_API_KEY`, `ANTHROPIC_MODEL`, `CLAUDE_CODE_MAX_CONTEXT_TOKENS` pinned per model (200k for requrv-small-3.8, 128k default) — after probing `/messages`, and no config file is written (nothing to restore). An already-running instance cannot receive env vars, so the frontend asks for a restart confirmation; the initial agent-session spawn does not pass `--model`, so `ANTHROPIC_MODEL` applies until the user switches model from the Hermes UI. Detection: any `*.app` containing "hermes" in `/Applications`/`~/Applications` (macOS), `HERMES-IDE`/`hermes-ide`/`hermes` on PATH plus `%LOCALAPPDATA%\Programs` elsewhere; on macOS the inner Mach-O (`CFBundleExecutable` via `plutil`, fallback: first file in `Contents/MacOS`) is spawned detached with the env.
- Claude Desktop (Code tab) cannot be pointed at AI Hive and is intentionally NOT supported: the consumer app's Code tab runs cloud sessions authenticated with the claude.ai account (`CLAUDE_CODE_OAUTH_TOKEN`, subscription-scoped model catalog), launches its SDK with empty setting sources (ignores `~/.claude/settings.json`), and custom gateways are an enterprise-only ("3p") feature disabled in this build. Claude Code detection therefore counts the CLI only.

## Conventions

- User-facing strings (Rust error messages, UI copy, toasts) are in **Italian** — keep new user-facing text in Italian.
- Rust code is indented with **2 spaces** (matches `.editorconfig`); do NOT run `cargo fmt` — it would reformat everything to 4 spaces.
- Frontend style is enforced by ESLint (`@nuxt/eslint` stylistic): no semicolons, single quotes, 2 spaces, no trailing commas. There is a `.prettierrc` but no format script; `bun run lint` is the check.
- Version is duplicated in `src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml` — bump both together. Releases are automatic: pushing a `v*` tag triggers `.github/workflows/build.yml` (tauri-action builds macos/linux/windows and publishes a GitHub Release; the tag comes from the version in `tauri.conf.json`).
