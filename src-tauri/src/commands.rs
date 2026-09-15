use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::Manager;

pub const HIVE_OPENAI_BASE_URL: &str = "https://hive.requrv.ai/api/v1";

const KEY_FILE_NAME: &str = "hive.json";

fn key_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
  let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
  Ok(dir.join(KEY_FILE_NAME))
}

#[tauri::command]
pub fn get_hive_key(app: tauri::AppHandle) -> Result<Option<String>, String> {
  let path = key_file_path(&app)?;
  let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
  let value: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
  Ok(value
    .get("apiKey")
    .and_then(|k| k.as_str())
    .map(|s| s.to_string()))
}

#[tauri::command]
pub fn set_hive_key(app: tauri::AppHandle, key: String) -> Result<(), String> {
  let key = key.trim();
  if key.is_empty() {
    return Err("La chiave non può essere vuota".into());
  }
  let path = key_file_path(&app)?;
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
  }
  if let Ok(old) = std::fs::read_to_string(&path) {
    let _ = std::fs::write(path.with_extension("json.bak"), old);
  }
  let value = serde_json::json!({ "apiKey": key });
  std::fs::write(path, value.to_string()).map_err(|e| e.to_string())?;
  Ok(())
}

#[tauri::command]
pub fn delete_hive_key(app: tauri::AppHandle) -> Result<(), String> {
  let path = key_file_path(&app)?;
  match std::fs::remove_file(path) {
    Ok(()) => Ok(()),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(e) => Err(e.to_string()),
  }
}

#[derive(Serialize, Clone)]
pub struct HiveModel {
  pub id: String,
  pub model_type: String,
}

#[tauri::command]
pub async fn list_hive_models(key: String) -> Result<Vec<HiveModel>, String> {
  let client = reqwest::Client::new();
  let url = format!("{HIVE_OPENAI_BASE_URL}/models");
  let response = client
    .get(&url)
    .bearer_auth(key.trim())
    .timeout(std::time::Duration::from_secs(20))
    .send()
    .await
    .map_err(|e| format!("Impossibile raggiungere AI Hive: {e}"))?;

  let status = response.status();
  if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
    return Err("Chiave non valida: AI Hive ha rifiutato l'autenticazione (401/403)".into());
  }
  if !status.is_success() {
    return Err(format!("AI Hive ha risposto con lo stato {status}"));
  }

  let body: serde_json::Value = response
    .json()
    .await
    .map_err(|e| format!("Risposta non valida da AI Hive: {e}"))?;

  let models = body
    .get("data")
    .and_then(|d| d.as_array())
    .map(|items| {
      items
        .iter()
        .filter_map(|m| {
          let id = m.get("id")?.as_str()?.to_string();
          let model_type = m
            .get("type")
            .and_then(|t| t.as_str())
            .unwrap_or_default()
            .to_string();
          Some(HiveModel { id, model_type })
        })
        .collect()
    })
    .unwrap_or_default();

  Ok(models)
}

// Codex (>= 0.136) accepts only `wire_api = "responses"`, so the gateway must
// expose POST /responses. Probe it before opening the terminal to fail fast
// with an actionable message instead of a broken TUI session.
async fn assert_responses_available(key: &str) -> Result<(), String> {
  let client = reqwest::Client::new();
  let url = format!("{HIVE_OPENAI_BASE_URL}/responses");
  let response = client
    .head(&url)
    .bearer_auth(key.trim())
    .timeout(std::time::Duration::from_secs(10))
    .send()
    .await
    .map_err(|e| format!("Impossibile raggiungere AI Hive: {e}"))?;
  let status = response.status();
  if status == reqwest::StatusCode::NOT_FOUND {
    return Err(
      "Codex richiede l'endpoint /responses, che AI Hive non espone ancora (HTTP 404). L'aggiornamento del gateway è in corso: riprova più tardi."
        .into(),
    );
  }
  if !status.is_success() {
    return Err(format!("AI Hive ha risposto con lo stato {status}"));
  }
  Ok(())
}

#[derive(Serialize)]
pub struct ServiceStatus {
  pub opencode: bool,
  pub codex: bool,
}

#[tauri::command]
pub fn check_services() -> ServiceStatus {
  ServiceStatus {
    opencode: opencode_installed(),
    codex: find_service_binary("codex").is_some(),
  }
}

// The opencode launch flow targets the desktop app on macOS, the CLI elsewhere.
#[cfg(target_os = "macos")]
fn opencode_installed() -> bool {
  opencode_app_path().is_some()
}

#[cfg(not(target_os = "macos"))]
fn opencode_installed() -> bool {
  find_service_binary("opencode").is_some()
}

// Locate the npm executable, tolerating GUI-launched apps whose PATH misses
// version managers (nvm/volta) or Homebrew.
pub fn find_npm() -> Option<PathBuf> {
  if let Some(path) = find_on_path("npm") {
    return Some(path);
  }
  let home = home_dir()?;
  let mut candidates = vec![
    PathBuf::from("/opt/homebrew/bin/npm"),
    PathBuf::from("/usr/local/bin/npm"),
    home.join(".volta").join("bin").join("npm"),
  ];
  let nvm_versions = home.join(".nvm").join("versions").join("node");
  if let Ok(entries) = std::fs::read_dir(&nvm_versions) {
    let mut versions: Vec<(String, PathBuf)> = entries
      .flatten()
      .filter_map(|entry| {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('v') {
          Some((name, entry.path().join("bin").join("npm")))
        } else {
          None
        }
      })
      .collect();
    versions.sort_by(|a, b| compare_versions(&b.0, &a.0));
    candidates.extend(versions.into_iter().map(|(_, path)| path));
  }
  if let Some(found) = candidates.into_iter().find(|c| c.is_file()) {
    return Some(found);
  }
  npm_via_login_shell()
}

fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
  let parse = |s: &str| s.trim_start_matches('v').split('.').filter_map(|p| p.parse::<u32>().ok()).collect::<Vec<_>>();
  parse(a).cmp(&parse(b))
}

// Last resort: ask the user's login shell (which sources .zprofile/.zshrc,
// where nvm & co are usually initialized) where npm lives.
#[cfg(target_os = "macos")]
fn npm_via_login_shell() -> Option<PathBuf> {
  for shell in ["/bin/zsh", "/bin/bash"] {
    let Ok(output) = Command::new(shell)
      .args(["-lic", "command -v npm"])
      .stdin(Stdio::null())
      .output()
    else {
      continue;
    };
    if !output.status.success() {
      continue;
    }
    let raw = String::from_utf8_lossy(&output.stdout);
    let Some(line) = raw.lines().rev().find(|l| {
      let l = l.trim();
      l.starts_with('/') && l.ends_with("/npm")
    }) else {
      continue;
    };
    let path = PathBuf::from(line);
    if path.is_file() {
      return Some(path);
    }
  }
  None
}

#[cfg(not(target_os = "macos"))]
fn npm_via_login_shell() -> Option<PathBuf> {
  None
}

// PATH of the current process can be stale (e.g. the app is launched from an
// explorer that started before a PATH update), so on Windows we also read the
// live PATH values from the registry and expand their %VARS%.
#[cfg(windows)]
fn registry_env() -> std::collections::HashMap<String, String> {
  let mut map = std::collections::HashMap::new();
  for hive in [
    "HKCU\\Environment",
    "HKLM\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
  ] {
    let Ok(output) = Command::new("reg").args(["query", hive]).output() else {
      continue;
    };
    let Ok(raw) = String::from_utf8(output.stdout) else {
      continue;
    };
    for line in raw.lines() {
      let trimmed = line.trim();
      if trimmed.is_empty() || trimmed.starts_with("HKEY") || trimmed.starts_with("---") {
        continue;
      }
      let mut parts = trimmed.split_whitespace();
      let Some(name) = parts.next() else { continue };
      let Some(kind) = parts.next() else { continue };
      if kind != "REG_SZ" && kind != "REG_EXPAND_SZ" {
        continue;
      }
      let value = parts.collect::<Vec<_>>().join(" ");
      if !value.is_empty() {
        map.insert(name.to_uppercase(), value);
      }
    }
  }
  map
}

#[cfg(windows)]
fn expand_registry_vars(value: &str, registry: &std::collections::HashMap<String, String>) -> String {
  let mut out = String::new();
  let mut rest = value;
  while let Some(start) = rest.find('%') {
    out.push_str(&rest[..start]);
    let after = &rest[start + 1..];
    if let Some(end) = after.find('%') {
      let var = &after[..end];
      let expanded = registry
        .get(&var.to_uppercase())
        .cloned()
        .or_else(|| std::env::var(var).ok())
        .unwrap_or_default();
      out.push_str(&expanded);
      rest = &after[end + 1..];
    } else {
      out.push('%');
      rest = after;
    }
  }
  out.push_str(rest);
  out
}

fn effective_path_dirs() -> Vec<String> {
  let mut dirs: Vec<String> = std::env::var("PATH")
    .unwrap_or_default()
    .split(if cfg!(windows) { ';' } else { ':' })
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .collect();
  #[cfg(windows)]
  {
    let registry = registry_env();
    if let Some(raw) = registry.get("PATH") {
      let expanded = expand_registry_vars(raw, &registry);
      for entry in expanded.split(';') {
        let entry = entry.trim();
        if !entry.is_empty() {
          dirs.push(entry.to_string());
        }
      }
    }
  }
  let mut seen = std::collections::HashSet::new();
  dirs.retain(|d| seen.insert(d.clone()));
  dirs
}

fn find_on_path(name: &str) -> Option<PathBuf> {
  // On Windows the extensionless "shim" npm drops next to the .cmd/.ps1 shims
  // is a bash script and cannot be executed, so skip the empty extension.
  let extensions: &[&str] = if cfg!(windows) {
    &[".exe", ".cmd", ".bat", ".ps1"]
  } else {
    &[""]
  };
  for dir in effective_path_dirs() {
    for ext in extensions {
      let candidate = Path::new(&dir).join(format!("{name}{ext}"));
      if candidate.is_file() {
        return Some(candidate);
      }
    }
  }
  None
}

fn home_dir() -> Option<PathBuf> {
  std::env::var("USERPROFILE")
    .or_else(|_| std::env::var("HOME"))
    .ok()
    .map(PathBuf::from)
}

// Directory of the active npm global prefix (covers nvm/nvm4w layouts where
// `npm i -g` shims live outside the default %APPDATA%\npm).
fn npm_global_dir() -> Option<PathBuf> {
  // find_npm (not find_on_path): a GUI-launched app has a minimal PATH and
  // would miss nvm/Homebrew installs otherwise.
  let npm = find_npm()?;
  let mut command = if cfg!(windows) {
    let mut c = Command::new("cmd");
    c.arg("/c").arg(&npm);
    c
  } else {
    Command::new(&npm)
  };
  command.args(["config", "get", "prefix"]);
  let output = command.output().ok()?;
  if !output.status.success() {
    return None;
  }
  let raw = String::from_utf8_lossy(&output.stdout);
  let line = raw.lines().next()?.trim().to_string();
  if line.is_empty() {
    return None;
  }
  Some(PathBuf::from(line))
}

fn find_service_binary(service: &str) -> Option<PathBuf> {
  if let Some(path) = find_on_path(service) {
    return Some(path);
  }
  let home = home_dir()?;
  let candidates: Vec<PathBuf> = match service {
    "opencode" => {
      let mut candidates = vec![
        home.join(".opencode").join("bin").join("opencode"),
        home.join(".opencode").join("bin").join("opencode.exe"),
      ];
      if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata = PathBuf::from(appdata);
        candidates.push(appdata.join("npm").join("opencode.cmd"));
        candidates.push(appdata.join("npm").join("opencode.exe"));
      }
      candidates
    }
    "codex" => {
      if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata = PathBuf::from(appdata);
        vec![
          appdata.join("npm").join("codex.cmd"),
          appdata.join("npm").join("codex.exe"),
        ]
      } else {
        vec![home.join(".local").join("bin").join("codex")]
      }
    }
    _ => return None,
  };
  if let Some(found) = candidates.into_iter().find(|c| c.is_file()) {
    return Some(found);
  }

  // Fallback: nvm4w per-version directories on Windows (%APPDATA%\nvm\v*\).
  #[cfg(windows)]
  if let Ok(appdata) = std::env::var("APPDATA") {
    let nvm = Path::new(&appdata).join("nvm");
    if let Ok(entries) = std::fs::read_dir(&nvm) {
      for entry in entries.flatten() {
        if !entry.path().is_dir() {
          continue;
        }
        let candidate = entry.path().join(format!("{service}.cmd"));
        if candidate.is_file() {
          return Some(candidate);
        }
      }
    }
  }

  // Fallback: active npm global prefix (shim or real binary next to it).
  let npm_dir = npm_global_dir()?;
  let npm_candidates: Vec<PathBuf> = match service {
    "opencode" => vec![
      npm_dir.join("opencode.cmd"),
      npm_dir.join("opencode.exe"),
      npm_dir
        .join("node_modules")
        .join("opencode-ai")
        .join("bin")
        .join("opencode.exe"),
    ],
    "codex" => vec![npm_dir.join("codex.cmd"), npm_dir.join("codex.exe")],
    _ => return None,
  };
  npm_candidates.into_iter().find(|c| c.is_file())
}

// Location of the OpenCode desktop bundle (macOS only).
fn opencode_app_path_in(candidates: &[PathBuf]) -> Option<PathBuf> {
  candidates.iter().find(|path| path.is_dir()).cloned()
}

#[cfg(target_os = "macos")]
fn opencode_app_path() -> Option<PathBuf> {
  let home = home_dir()?;
  opencode_app_path_in(&[
    PathBuf::from("/Applications/OpenCode.app"),
    home.join("Applications").join("OpenCode.app"),
  ])
}

#[cfg(not(target_os = "macos"))]
fn opencode_app_path() -> Option<PathBuf> {
  None
}

#[tauri::command]
pub async fn launch_service(app: tauri::AppHandle, service: String, model: String, key: String) -> Result<(), String> {
  let model = model.trim();
  let key = key.trim();
  if model.is_empty() {
    return Err("Nessun modello selezionato".into());
  }
  if key.is_empty() {
    return Err("Chiave Hive non salvata".into());
  }
  match service.as_str() {
    "opencode" => launch_opencode(&app, model, key),
    "codex" => launch_codex(&app, model, key).await,
    _ => Err(format!("Servizio sconosciuto: {service}")),
  }
}

const OPENCODE_PROVIDER_ID: &str = "requrv-hive";

// Strip // and /* */ comments outside of JSON strings so the JSONC config
// can be parsed with serde_json.
fn strip_jsonc_comments(input: &str) -> String {
  let mut out = String::with_capacity(input.len());
  let mut chars = input.chars().peekable();
  let mut in_string = false;
  let mut in_line_comment = false;
  let mut in_block_comment = false;
  while let Some(c) = chars.next() {
    if in_line_comment {
      if c == '\n' {
        in_line_comment = false;
        out.push(c);
      }
      continue;
    }
    if in_block_comment {
      if c == '*' && chars.peek() == Some(&'/') {
        chars.next();
        in_block_comment = false;
      }
      continue;
    }
    if in_string {
      out.push(c);
      match c {
        '\\' => {
          if let Some(escaped) = chars.next() {
            out.push(escaped);
          }
        }
        '"' => in_string = false,
        _ => {}
      }
      continue;
    }
    match c {
      '"' => {
        in_string = true;
        out.push(c);
      }
      '/' if chars.peek() == Some(&'/') => {
        chars.next();
        in_line_comment = true;
      }
      '/' if chars.peek() == Some(&'*') => {
        chars.next();
        in_block_comment = true;
      }
      _ => out.push(c),
    }
  }
  out
}

// Merge the ReQurv Hive provider block and the default model into the global
// opencode config, leaving every other field untouched.
fn merge_hive_provider(config: &mut serde_json::Value, model: &str, key: &str) {
  if !config.is_object() {
    *config = serde_json::json!({});
  }
  let Some(root) = config.as_object_mut() else {
    return;
  };
  root.entry("$schema".to_string())
    .or_insert_with(|| serde_json::Value::String("https://opencode.ai/config.json".into()));
  let provider_block = serde_json::json!({
    "npm": "@ai-sdk/openai-compatible",
    "name": "ReQurv Hive",
    "options": {
      "baseURL": HIVE_OPENAI_BASE_URL,
      "apiKey": key,
    },
    "models": {
      model: { "name": model },
    },
  });
  let existing = root
    .get("provider")
    .cloned()
    .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()));
  let mut providers = existing.as_object().cloned().unwrap_or_default();
  providers.insert(OPENCODE_PROVIDER_ID.to_string(), provider_block);
  root.insert("provider".to_string(), serde_json::Value::Object(providers));
  root.insert(
    "model".to_string(),
    serde_json::Value::String(format!("{OPENCODE_PROVIDER_ID}/{model}")),
  );
}

// Resolve the global opencode config file: prefer an existing one
// (.jsonc or .json), defaulting to creating opencode.jsonc.
fn opencode_config_path_in(home: &Path) -> PathBuf {
  let dir = home.join(".config").join("opencode");
  let jsonc = dir.join("opencode.jsonc");
  if jsonc.exists() {
    return jsonc;
  }
  let json = dir.join("opencode.json");
  if json.exists() {
    return json;
  }
  jsonc
}

// Back up and rewrite the global opencode config with the Hive provider and
// the selected model. Refuses to touch the file when it cannot be parsed.
fn write_opencode_config_in(home: &Path, model: &str, key: &str) -> Result<PathBuf, String> {
  let dir = home.join(".config").join("opencode");
  std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  let path = opencode_config_path_in(home);

  let mut config = if path.exists() {
    let raw = std::fs::read_to_string(&path)
      .map_err(|e| format!("Impossibile leggere {}: {e}", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("json");
    let backup = path.with_extension(format!("{ext}.bak"));
    let _ = std::fs::write(&backup, raw.as_str());
    let cleaned = strip_jsonc_comments(&raw);
    serde_json::from_str(&cleaned).map_err(|e| {
      format!("{} non è un JSON valido: {e}. Correggi il file e riprova.", path.display())
    })?
  } else {
    serde_json::json!({})
  };

  merge_hive_provider(&mut config, model, key);
  let mut rendered = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
  rendered.push('\n');
  std::fs::write(&path, rendered)
    .map_err(|e| format!("Impossibile scrivere {}: {e}", path.display()))?;
  Ok(path)
}

fn write_opencode_config(model: &str, key: &str) -> Result<(), String> {
  let home = home_dir().ok_or_else(|| "Home directory non trovata".to_string())?;
  write_opencode_config_in(&home, model, key).map(|_| ())
}

fn launch_opencode(app: &tauri::AppHandle, model: &str, key: &str) -> Result<(), String> {
  // Fail fast on a missing target before touching the user's config file.
  #[cfg(target_os = "macos")]
  {
    let _ = app;
    let bundle = opencode_app_path()
      .ok_or_else(|| String::from("OpenCode.app non trovato in /Applications: installalo e riprova."))?;
    write_opencode_config(model, key)?;
    Command::new("open")
      .arg(&bundle)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .map(|_| ())
      .map_err(|e| format!("Impossibile avviare OpenCode.app: {e}"))
  }
  #[cfg(not(target_os = "macos"))]
  {
    let bin = find_service_binary("opencode")
      .ok_or_else(|| String::from("OpenCode non è installato. Scaricalo da https://opencode.ai/download."))?;
    write_opencode_config(model, key)?;
    launch_cli(app, "opencode", &bin, &[], &[])
  }
}

async fn launch_codex(app: &tauri::AppHandle, model: &str, key: &str) -> Result<(), String> {
  let bin = find_service_binary("codex")
    .ok_or_else(|| String::from("Codex non è installato. Scaricalo da https://chatgpt.com/codex."))?;

  assert_responses_available(key).await?;

  let codex_dir = home_dir()
    .ok_or_else(|| "Home directory non trovata".to_string())?
    .join(".codex");
  std::fs::create_dir_all(&codex_dir).map_err(|e| e.to_string())?;

  let profile_path = codex_dir.join("hive.config.toml");
  if let Ok(old) = std::fs::read_to_string(&profile_path) {
    let _ = std::fs::write(profile_path.with_extension("toml.bak"), old);
  }

  let profile = format!(
    "model = \"{model}\"\nmodel_provider = \"hive\"\n\n[model_providers.hive]\nname = \"ReQurv AI Hive\"\nbase_url = \"{base}\"\nwire_api = \"responses\"\nenv_key = \"HIVE_API_KEY\"\n",
    base = HIVE_OPENAI_BASE_URL,
  );
  std::fs::write(&profile_path, profile).map_err(|e| e.to_string())?;

  let args = vec!["--profile".to_string(), "hive".to_string()];
  let env: Vec<(&str, &str)> = vec![("HIVE_API_KEY", key)];
  launch_cli(app, "codex", &bin, &args, &env)
}

// Quote a value for safe inclusion in a single-quoted shell word.
fn shell_quote(value: &str) -> String {
  format!("'{}'", value.replace('\'', "'\\''"))
}

// Build a bash script that sets the Hive env vars and runs the CLI so the
// terminal window stays attached to the process (TUI apps need a real TTY).
fn build_terminal_script(bin: &Path, args: &[String], env: &[(&str, &str)]) -> String {
  let mut out = String::from("#!/bin/bash\ncd \"$HOME\"\n");
  for (name, value) in env {
    out.push_str(&format!("export {name}={}\n", shell_quote(value)));
  }
  let mut command = shell_quote(&bin.to_string_lossy());
  for arg in args {
    command.push(' ');
    command.push_str(&shell_quote(arg));
  }
  out.push_str(&command);
  out.push_str(
    "\necho \"\"\necho \"Processo terminato (exit code: $?). Premi Invio per chiudere la finestra.\"\nread -r _\n",
  );
  out
}

// Single entry point for launching a CLI: on macOS the agents are TUI apps
// that need a terminal, so we hand a .command script to the system's default
// terminal via `open`; elsewhere we keep the detached spawn.
fn launch_cli(
  app: &tauri::AppHandle,
  service: &str,
  bin: &Path,
  args: &[String],
  env: &[(&str, &str)],
) -> Result<(), String> {
  #[cfg(target_os = "macos")]
  {
    let script = build_terminal_script(bin, args, env);
    return open_terminal_script(app, service, &script);
  }
  #[cfg(not(target_os = "macos"))]
  {
    let _ = app;
    spawn_cli(bin, args, env)
  }
}

#[cfg(target_os = "macos")]
fn open_terminal_script(app: &tauri::AppHandle, service: &str, script: &str) -> Result<(), String> {
  let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
  std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  let path = dir.join(format!("launch-{service}.command"));
  std::fs::write(&path, script).map_err(|e| format!("Scrittura script non riuscita: {e}"))?;
  use std::os::unix::fs::PermissionsExt;
  let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));

  // Launch via AppleScript `do script`: `open` on .command files goes through
  // LaunchServices and can silently no-op, while `do script` is deterministic
  // (first run may trigger the macOS automation permission prompt).
  let applescript = format!(
    "tell application \"Terminal\"\n  activate\n  do script \"bash {}\"\nend tell\n",
    applescript_escape(&shell_quote(&path.to_string_lossy())),
  );
  let mut child = Command::new("osascript")
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .spawn()
    .map_err(|e| format!("Impossibile avviare osascript: {e}"))?;
  use std::io::Write;
  if let Some(mut stdin) = child.stdin.take() {
    stdin
      .write_all(applescript.as_bytes())
      .map_err(|e| e.to_string())?;
  }
  let output = child
    .wait_with_output()
    .map_err(|e| format!("osascript terminato in modo anomalo: {e}"))?;
  if !output.status.success() {
    let err = String::from_utf8_lossy(&output.stderr);
    return Err(format!("Terminal non ha eseguito lo script: {err}"));
  }
  Ok(())
}

#[cfg(target_os = "macos")]
fn applescript_escape(value: &str) -> String {
  value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(windows)]
const CREATE_NEW_CONSOLE: u32 = 0x00000010;

#[cfg(windows)]
fn is_shell_shim(path: &Path) -> bool {
  path.extension()
    .and_then(|e| e.to_str())
    .is_some_and(|e| matches!(e.to_lowercase().as_str(), "cmd" | "bat" | "ps1"))
}

#[cfg(windows)]
fn is_powershell_shim(path: &Path) -> bool {
  path.extension()
    .and_then(|e| e.to_str())
    .is_some_and(|e| e.to_lowercase().as_str() == "ps1")
}

// On macOS every launch goes through the terminal script; the detached spawn
// is the fallback for Windows (new console) and Linux.
#[cfg_attr(target_os = "macos", allow(dead_code))]
fn spawn_cli(bin: &Path, args: &[String], env: &[(&str, &str)]) -> Result<(), String> {
  let mut command = Command::new(bin);
  #[cfg(windows)]
  if is_shell_shim(bin) {
    if is_powershell_shim(bin) {
      command = Command::new("powershell.exe");
      command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]);
      command.arg(bin);
    } else {
      command = Command::new("cmd");
      command.arg("/c");
      command.arg(bin);
    }
  }
  command.args(args);
  command.stdin(Stdio::null());
  command.stdout(Stdio::null());
  command.stderr(Stdio::null());
  for (name, value) in env {
    command.env(name, value);
  }
  #[cfg(windows)]
  {
    use std::os::windows::process::CommandExt;
    command.creation_flags(CREATE_NEW_CONSOLE);
  }
  command
    .spawn()
    .map_err(|e| format!("Avvio non riuscito: {e}"))?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_opencode_if_installed() {
    let bin = find_service_binary("opencode");
    println!("opencode rilevato come: {bin:?}");
    if let Some(bin) = &bin {
      assert!(bin.is_file());
    }
  }

  #[test]
  fn finds_npm_if_installed() {
    let npm = find_npm();
    println!("npm rilevato come: {npm:?}");
    if let Some(npm) = &npm {
      assert!(npm.is_file());
    }
  }

  #[test]
  fn shell_quote_wraps_and_escapes_single_quotes() {
    assert_eq!(shell_quote("abc"), "'abc'");
    assert_eq!(shell_quote(""), "''");
    assert_eq!(shell_quote("it's"), "'it'\\''s'");
    assert_eq!(shell_quote("a b c"), "'a b c'");
  }

  #[test]
  fn builds_opencode_terminal_script() {
    let script = build_terminal_script(
      Path::new("/opt/homebrew/bin/opencode"),
      &[],
      &[("OPENCODE_CONFIG_CONTENT", "{\"model\":\"hive/x\"}")],
    );
    assert!(script.starts_with("#!/bin/bash\ncd \"$HOME\"\n"));
    assert!(script.contains(r#"export OPENCODE_CONFIG_CONTENT='{"model":"hive/x"}'"#));
    assert!(script.contains("'/opt/homebrew/bin/opencode'"));
    assert!(script.ends_with("read -r _\n"));
  }

  #[test]
  fn builds_codex_terminal_script() {
    let script = build_terminal_script(
      Path::new("/Users/x/.nvm/versions/node/v24/bin/codex"),
      &["--profile".to_string(), "hive".to_string()],
      &[("HIVE_API_KEY", "sk-test")],
    );
    assert!(script.contains("export HIVE_API_KEY='sk-test'"));
    assert!(script.contains("'--profile' 'hive'"));
    assert!(script.contains("'/Users/x/.nvm/versions/node/v24/bin/codex' '--profile' 'hive'"));
  }

  // Simula un processo con PATH vecchio/stripped (es. app lanciata da un
  // explorer partito prima dell'aggiornamento PATH): su Windows la
  // rilevazione deve comunque funzionare leggendo il PATH dal registry.
  #[test]
  fn finds_opencode_with_stale_path() {
    let original = std::env::var("PATH").unwrap_or_default();
    std::env::set_var(
      "PATH",
      if cfg!(windows) {
        "C:\\Windows\\system32"
      } else {
        "/usr/bin:/bin"
      },
    );
    let bin = find_service_binary("opencode");
    std::env::set_var("PATH", original);
    println!("opencode con PATH strippato: {bin:?}");
    if cfg!(windows) {
      assert!(bin.is_some(), "opencode non trovato con PATH strippato");
    }
  }

  #[test]
  fn strips_jsonc_comments_outside_strings() {
    let input = r#"{
  // commento di riga
  "a": "https://hive.requrv.ai//x", /* commento di blocco */
  "b": 1
}"#;
    let cleaned = strip_jsonc_comments(input);
    let value: serde_json::Value = serde_json::from_str(&cleaned).expect("parsable");
    assert_eq!(value["a"], "https://hive.requrv.ai//x");
    assert_eq!(value["b"], 1);
  }

  #[test]
  fn strips_jsonc_keeps_slashes_inside_strings() {
    let input = r#"{"note": "dire // non è un commento", "url": "https://x.ai/y"}"#;
    let cleaned = strip_jsonc_comments(input);
    let value: serde_json::Value = serde_json::from_str(&cleaned).expect("parsable");
    assert_eq!(value["note"], "dire // non è un commento");
    assert_eq!(value["url"], "https://x.ai/y");
  }

  #[test]
  fn merge_creates_provider_block_in_empty_config() {
    let mut config = serde_json::json!({});
    merge_hive_provider(&mut config, "requrv-small-3.8", "requrv_sk_test");
    assert_eq!(config["$schema"], "https://opencode.ai/config.json");
    let provider = &config["provider"]["requrv-hive"];
    assert_eq!(provider["npm"], "@ai-sdk/openai-compatible");
    assert_eq!(provider["name"], "ReQurv Hive");
    assert_eq!(provider["options"]["baseURL"], HIVE_OPENAI_BASE_URL);
    assert_eq!(provider["options"]["apiKey"], "requrv_sk_test");
    assert_eq!(provider["models"]["requrv-small-3.8"]["name"], "requrv-small-3.8");
    assert_eq!(config["model"], "requrv-hive/requrv-small-3.8");
  }

  #[test]
  fn merge_preserves_foreign_fields_and_updates_hive_block() {
    let mut config = serde_json::json!({
      "$schema": "https://opencode.ai/config.json",
      "permission": { "edit": "allow" },
      "model": "anthropic/claude-sonnet-4-6",
      "provider": {
        "anthropic": { "name": "Anthropic" },
        "requrv-hive": { "npm": "@ai-sdk/openai-compatible", "options": { "apiKey": "vecchia" } }
      }
    });
    merge_hive_provider(&mut config, "requrv-small-3.8", "requrv_sk_nuova");
    assert_eq!(config["permission"]["edit"], "allow");
    assert_eq!(config["provider"]["anthropic"]["name"], "Anthropic");
    assert_eq!(config["provider"]["requrv-hive"]["options"]["apiKey"], "requrv_sk_nuova");
    assert_eq!(config["provider"]["requrv-hive"]["models"]["requrv-small-3.8"]["name"], "requrv-small-3.8");
    assert_eq!(config["model"], "requrv-hive/requrv-small-3.8");
  }

  #[test]
  fn config_path_prefers_existing_jsonc() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-cfg-{}", std::process::id()));
    let dir = tmp.join(".config").join("opencode");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    std::fs::write(dir.join("opencode.jsonc"), "{}").expect("write jsonc");
    std::fs::write(dir.join("opencode.json"), "{}").expect("write json");
    let resolved = opencode_config_path_in(&tmp);
    assert!(resolved.to_string_lossy().ends_with("opencode.jsonc"));
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn config_path_defaults_to_jsonc_when_missing() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-cfg-empty-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&tmp);
    let resolved = opencode_config_path_in(&tmp);
    assert!(resolved.to_string_lossy().ends_with("opencode.jsonc"));
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn writes_and_backs_up_opencode_config() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-write-{}", std::process::id()));
    let dir = tmp.join(".config").join("opencode");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let existing = r#"{
  // commento
  "share": "manual",
  "provider": { "anthropic": { "name": "Anthropic" } }
}"#;
    std::fs::write(dir.join("opencode.jsonc"), existing).expect("write jsonc");

    let path = write_opencode_config_in(&tmp, "requrv-small-3.8", "requrv_sk_test").expect("write");
    assert!(path.to_string_lossy().ends_with("opencode.jsonc"));

    let rendered = std::fs::read_to_string(&path).expect("read back");
    let value: serde_json::Value = serde_json::from_str(&rendered).expect("valid json");
    assert_eq!(value["share"], "manual");
    assert_eq!(value["provider"]["anthropic"]["name"], "Anthropic");
    assert_eq!(value["provider"]["requrv-hive"]["options"]["apiKey"], "requrv_sk_test");
    assert_eq!(value["model"], "requrv-hive/requrv-small-3.8");
    assert!(!rendered.contains("commento"));

    let backup = dir.join("opencode.jsonc.bak");
    let backed = std::fs::read_to_string(&backup).expect("backup exists");
    assert!(backed.contains("commento"));
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn finds_opencode_app_bundle_among_candidates() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-app-{}", std::process::id()));
    let bundle = tmp.join("OpenCode.app");
    std::fs::create_dir_all(&bundle).expect("create fake bundle");
    let missing = tmp.join("Assente.app");
    assert_eq!(
      opencode_app_path_in(&[missing.clone(), bundle.clone()]),
      Some(bundle)
    );
    assert_eq!(opencode_app_path_in(&[missing]), None);
    let _ = std::fs::remove_dir_all(&tmp);
  }
}
