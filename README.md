# ReQurv Launch

[![CI](https://img.shields.io/badge/CI-lint%20%2B%20typecheck-00DC82?logo=githubactions)](https://github.com/ReQurv/requrv-launch/actions)

ReQurv Launch è un'applicazione desktop che avvia i tuoi agenti di coding AI già configurati per [AI Hive](https://hive.requrv.ai), l'AI Gateway di ReQurv.

Attualmente supporta:

- [OpenCode](https://opencode.ai) — IDE di coding (su macOS è disponibile sia l'app desktop sia la CLI, altrove la CLI)
- [Codex](https://chatgpt.com/codex) — CLI di coding di OpenAI (su macOS è disponibile anche l'app ChatGPT)

## Funzionalità

- Salvataggio e verifica della chiave API di AI Hive (validata contro il gateway al salvataggio)
- Elencazione dei modelli disponibili su AI Hive e scelta del modello di default
- Rilevazione automatica dell'installazione di OpenCode e Codex (con supporto a nvm, Volta, Homebrew, installazioni npm globali e PATH "stale" da registry su Windows)
- Avvio con un clic:
  - **Scelta della destinazione**: se sia l'app desktop sia la CLI sono installate, un modale chiede all'utente di aprire l'app o il terminale; altrimenti l'avvio è diretto sulla destinazione disponibile
  - **OpenCode**: scrive il provider `requrv-hive` nel file di configurazione globale (`~/.config/opencode/opencode.jsonc` o `.json`), preservando le altre impostazioni e creando un backup
   - **Codex (terminale)**: crea un profilo dedicato (`~/.codex/hive.config.toml`) con `wire_api = "responses"` e avvia la CLI con `--profile hive` e la variabile `HIVE_API_KEY`
   - **ChatGPT.app (app, solo macOS)**: punta `~/.codex/config.toml` ad AI Hive (`openai_base_url` + catalogo modelli `~/.codex/hive-models.json`) e salva la chiave in `~/.codex/auth.json` in modalità apikey; i file originali sono salvati in backup `.hive.bak` e recuperabili in un clic dal tasto "Ripristina ChatGPT". Il catalogo modelli viene letto all'avvio, quindi se l'app è già aperta viene chiesto il riavvio

Su macOS gli agenti sono applicazioni TUI: l'avvio avviene aprendo uno script nel terminale di sistema, in modo da fornire un TTY reale.

## Requisiti

- [Bun](https://bun.sh)
- [Rust](https://rustup.rs) (per il backend Tauri)
- Dipendenze di sistema per Tauri: vedi [prerequisiti](https://tauri.app/start/prerequisites/)

## Sviluppo

Installa le dipendenze:

```bash
bun install
```

Avvia l'app in modalità sviluppo (Nuxt su `http://localhost:3001` + finestra Tauri):

```bash
bun run tauri dev
```

Per sviluppare solo la parte web (senza Tauri; le funzionalità legate a Tauri sono disattivate automaticamente):

```bash
bun run dev
```

## Script

| Script              | Descrizione                                              |
| ------------------- | -------------------------------------------------------- |
| `bun run dev`       | Server di sviluppo Nuxt (solo web)                       |
| `bun run build`     | Build di produzione del frontend (SSG)                   |
| `bun run preview`   | Anteprima locale della build di produzione               |
| `bun run tauri`     | CLI Tauri (`dev`, `build`, ecc.)                         |
| `bun run lint`      | ESLint                                                   |
| `bun run typecheck` | Typecheck con `nuxt typecheck` / `vue-tsc`               |

Test Rust:

```bash
cd src-tauri
cargo test
```

## Struttura del progetto

```
app/                  # Frontend Nuxt (UI, composable useHive)
src-tauri/            # Backend Tauri (comandi, rilevazione servizi, avvio)
public/               # Asset statici (loghi, favicon)
```

Comandi Tauri esposti al frontend (`src-tauri/src/commands.rs`):

- `get_hive_key` / `set_hive_key` / `delete_hive_key` — gestione della chiave (memorizzata in `hive.json` nella cartella di configurazione dell'app, con backup `.bak`)
- `list_hive_models` — elenca i modelli da `GET /models` del gateway
- `check_services` — rileva se OpenCode e Codex sono installati (e se ChatGPT.app è configurata su Hive)
- `launch_service` — configura e avvia il servizio scelto
- `configure_chatgpt_app` / `open_chatgpt_app` / `restart_chatgpt_app` / `restore_chatgpt_app` — configurazione, apertura, riavvio e ripristino di ChatGPT.app su AI Hive

## Build e release

Build locale dei binary e dei bundle:

```bash
bun run tauri build
```

I release sono automatizzati: la CI (`.github/workflows/build.yml`) compila su macOS, Linux e Windows e pubblica un GitHub Release quando viene pushato un tag `v*`.

## CI

La CI (`.github/workflows/ci.yml`) esegue lint e typecheck ad ogni push.

## License

[MIT](./LICENSE)
