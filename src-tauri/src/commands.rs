use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tauri::Manager;

pub const HIVE_OPENAI_BASE_URL: &str = "https://hive.requrv.ai/api/v1";
// Claude Code speaks the Anthropic Messages API and appends /v1/messages to
// ANTHROPIC_BASE_URL, so the base is the gateway without the trailing /v1.
pub const HIVE_ANTHROPIC_BASE_URL: &str = "https://hive.requrv.ai/api";

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

#[derive(Serialize, Deserialize, Clone)]
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

// Error for a gateway probe status, if any. 4xx from the handler (422
// validation on the empty body, 401, 405, ...) prove the route exists, so
// only 404 (route missing) and 5xx (gateway failure) are errors.
fn probe_error(status: reqwest::StatusCode, agent: &str, endpoint: &str) -> Option<String> {
  if status == reqwest::StatusCode::NOT_FOUND {
    Some(format!(
      "{agent} richiede l'endpoint {endpoint}, che AI Hive non espone (HTTP 404). Riprova più tardi."
    ))
  } else if status.is_server_error() {
    Some(format!("AI Hive ha risposto con lo stato {status}"))
  } else {
    None
  }
}

// POST an empty body to the gateway endpoint the agent needs. A 4xx from the
// handler (e.g. 422 validation on the empty body) proves the route exists
// without invoking a model; only 404 and 5xx are errors. Probing before
// opening a session fails fast with an actionable message instead of a
// broken TUI.
async fn assert_endpoint_available(key: &str, endpoint: &str, agent: &str) -> Result<(), String> {
  let client = reqwest::Client::new();
  let url = format!("{HIVE_OPENAI_BASE_URL}{endpoint}");
  let response = client
    .post(&url)
    .bearer_auth(key.trim())
    .json(&serde_json::json!({}))
    .timeout(std::time::Duration::from_secs(10))
    .send()
    .await
    .map_err(|e| format!("Impossibile raggiungere AI Hive: {e}"))?;
  if let Some(err) = probe_error(response.status(), agent, endpoint) {
    return Err(err);
  }
  Ok(())
}

// Codex (>= 0.136) accepts only `wire_api = "responses"`, so the gateway must
// expose POST /responses. The gateway routes only POST on that path (HEAD/GET
// never complete), so the probe POSTs an empty body.
async fn assert_responses_available(key: &str) -> Result<(), String> {
  assert_endpoint_available(key, "/responses", "Codex").await
}

// Claude Code speaks the Anthropic Messages API, so the gateway must expose
// POST /messages (it must also accept `role: "system"` entries in messages[],
// which Claude Code v2.x sends).
async fn assert_messages_available(key: &str) -> Result<(), String> {
  assert_endpoint_available(key, "/messages", "Claude Code").await
}

#[derive(Serialize)]
pub struct ServiceStatus {
  pub opencode: bool,
  pub codex: bool,
  pub claude_code: bool,
  pub opencode_app: bool,
  pub opencode_cli: bool,
  pub codex_app: bool,
  pub codex_cli: bool,
  pub codex_app_configured: bool,
  pub claude_code_cli: bool,
}

// Reports every launch target separately so the UI can offer the app/terminal
// choice only when both destinations exist.
#[tauri::command]
pub fn check_services() -> ServiceStatus {
  let opencode_app = opencode_app_path().is_some();
  let opencode_cli = find_service_binary("opencode").is_some();
  let codex_app = chatgpt_app_bundle().is_some();
  let codex_cli = find_service_binary("codex").is_some() || codex_app_binary().is_some();
  let codex_app_configured = home_dir().is_some_and(|home| chatgpt_app_configured_in(&home));
  // Claude Code is terminal-only: Claude Desktop cannot be pointed at AI Hive
  // (cloud Code tab bound to the claude.ai account), so only the CLI counts.
  let claude_code_cli = find_service_binary("claude").is_some();
  ServiceStatus {
    opencode: opencode_app || opencode_cli,
    codex: codex_app || codex_cli,
    claude_code: claude_code_cli,
    opencode_app,
    opencode_cli,
    codex_app,
    codex_cli,
    codex_app_configured,
    claude_code_cli,
  }
}

// Bin directories of every nvm-managed node version, newest first. GUI apps
// inherit a minimal PATH, so version-manager installs are probed explicitly.
fn nvm_bin_dirs() -> Vec<PathBuf> {
  let Some(home) = home_dir() else {
    return Vec::new();
  };
  let Ok(entries) = std::fs::read_dir(home.join(".nvm").join("versions").join("node")) else {
    return Vec::new();
  };
  let mut dirs: Vec<(String, PathBuf)> = entries
    .flatten()
    .filter_map(|entry| {
      let name = entry.file_name().to_string_lossy().to_string();
      if name.starts_with('v') {
        Some((name, entry.path().join("bin")))
      } else {
        None
      }
    })
    .collect();
  dirs.sort_by(|a, b| compare_versions(&b.0, &a.0));
  dirs.into_iter().map(|(_, path)| path).collect()
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
  candidates.extend(nvm_bin_dirs().into_iter().map(|dir| dir.join("npm")));
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
    "claude" => {
      if let Ok(appdata) = std::env::var("APPDATA") {
        let appdata = PathBuf::from(appdata);
        vec![
          appdata.join("npm").join("claude.cmd"),
          appdata.join("npm").join("claude.exe"),
        ]
      } else {
        // npm global installs fall through to the nvm/npm-prefix fallbacks
        // below; the native installer puts the binary in ~/.local/bin.
        vec![home.join(".local").join("bin").join("claude")]
      }
    }
    _ => return None,
  };
  if let Some(found) = candidates.into_iter().find(|c| c.is_file()) {
    return Some(found);
  }

  // nvm per-version bins (macOS/Linux): the app's PATH may miss them, and the
  // active prefix is not necessarily the version the CLI was installed on.
  #[cfg(not(windows))]
  for dir in nvm_bin_dirs() {
    let candidate = dir.join(service);
    if candidate.is_file() {
      return Some(candidate);
    }
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

  // Fallback: active npm global prefix. On Windows the shims sit next to the
  // prefix, on macOS/Linux the executables live in <prefix>/bin.
  let npm_dir = npm_global_dir()?;
  let npm_candidates: Vec<PathBuf> = match service {
    "opencode" => vec![
      npm_dir.join("bin").join("opencode"),
      npm_dir.join("opencode.cmd"),
      npm_dir.join("opencode.exe"),
      npm_dir
        .join("node_modules")
        .join("opencode-ai")
        .join("bin")
        .join("opencode.exe"),
    ],
    "codex" => vec![
      npm_dir.join("bin").join("codex"),
      npm_dir.join("codex.cmd"),
      npm_dir.join("codex.exe"),
    ],
    "claude" => vec![
      npm_dir.join("bin").join("claude"),
      npm_dir.join("claude.cmd"),
      npm_dir.join("claude.exe"),
    ],
    _ => return None,
  };
  npm_candidates.into_iter().find(|c| c.is_file())
}

// Location of a desktop .app bundle among the given candidates.
fn app_bundle_path_in(candidates: &[PathBuf]) -> Option<PathBuf> {
  candidates.iter().find(|path| path.is_dir()).cloned()
}

#[cfg(target_os = "macos")]
fn opencode_app_path() -> Option<PathBuf> {
  let home = home_dir()?;
  app_bundle_path_in(&[
    PathBuf::from("/Applications/OpenCode.app"),
    home.join("Applications").join("OpenCode.app"),
  ])
}

#[cfg(not(target_os = "macos"))]
fn opencode_app_path() -> Option<PathBuf> {
  None
}

// The ChatGPT desktop app (macOS) is what users install as "Codex"; it also
// ships the codex engine in its resources.
#[cfg(target_os = "macos")]
fn chatgpt_app_bundle() -> Option<PathBuf> {
  let home = home_dir()?;
  app_bundle_path_in(&[
    PathBuf::from("/Applications/ChatGPT.app"),
    home.join("Applications").join("ChatGPT.app"),
  ])
}

#[cfg(not(target_os = "macos"))]
fn chatgpt_app_bundle() -> Option<PathBuf> {
  None
}

fn codex_app_binary_in(candidates: &[PathBuf]) -> Option<PathBuf> {
  candidates
    .iter()
    .map(|bundle| bundle.join("Contents").join("Resources").join("codex"))
    .find(|bin| bin.is_file())
}

// The codex engine bundled inside ChatGPT.app counts as an installation when
// the standalone CLI is absent.
fn codex_app_binary() -> Option<PathBuf> {
  let bundle = chatgpt_app_bundle()?;
  codex_app_binary_in(std::slice::from_ref(&bundle))
}

#[tauri::command]
pub async fn launch_service(app: tauri::AppHandle, service: String, model: String, key: String, mode: String) -> Result<(), String> {
  let model = model.trim();
  let key = key.trim();
  if model.is_empty() {
    return Err("Nessun modello selezionato".into());
  }
  if key.is_empty() {
    return Err("Chiave Hive non salvata".into());
  }
  // ("codex", "app") is not handled here: the ChatGPT app flow needs a
  // restart confirmation, so the frontend drives it via configure_chatgpt_app,
  // open_chatgpt_app and restart_chatgpt_app. ("claude_code", "app") does not
  // exist: Claude Desktop cannot be pointed at AI Hive (see the note in the
  // Claude Desktop section).
  match (service.as_str(), mode.as_str()) {
    ("opencode", "app") => launch_opencode_app(model, key),
    ("opencode", "terminal") => launch_opencode_cli(&app, model, key),
    ("codex", "terminal") => launch_codex_cli(&app, model, key).await,
    ("claude_code", "terminal") => launch_claude_cli(&app, model, key).await,
    _ => Err(format!("Avvio non valido: {service} in modalità {mode}")),
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

// Opens a desktop .app bundle via LaunchServices.
#[cfg(target_os = "macos")]
fn open_app_bundle(bundle: &Path, label: &str) -> Result<(), String> {
  Command::new("open")
    .arg(bundle)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .map(|_| ())
    .map_err(|e| format!("Impossibile avviare {label}: {e}"))
}

// Fail fast on a missing bundle before touching the user's config file: the
// app reads the Hive provider from the global opencode config.
#[cfg(target_os = "macos")]
fn launch_opencode_app(model: &str, key: &str) -> Result<(), String> {
  let bundle = opencode_app_path()
    .ok_or_else(|| String::from("OpenCode.app non trovato in /Applications: installalo e riprova."))?;
  write_opencode_config(model, key)?;
  open_app_bundle(&bundle, "OpenCode.app")
}

#[cfg(not(target_os = "macos"))]
fn launch_opencode_app(_model: &str, _key: &str) -> Result<(), String> {
  Err(String::from("L'app OpenCode è disponibile solo su macOS."))
}

fn launch_opencode_cli(app: &tauri::AppHandle, model: &str, key: &str) -> Result<(), String> {
  let bin = find_service_binary("opencode")
    .ok_or_else(|| String::from("OpenCode CLI non trovata. Installala da https://opencode.ai/download."))?;
  write_opencode_config(model, key)?;
  launch_cli(app, "opencode", &bin, &[], &[])
}

// Opens ChatGPT.app. The Hive settings (root config, model catalog, auth) are
// written by configure_chatgpt_app before the first launch.
#[tauri::command]
#[cfg(target_os = "macos")]
pub fn open_chatgpt_app() -> Result<(), String> {
  let bundle = chatgpt_app_bundle()
    .ok_or_else(|| String::from("ChatGPT.app non trovato in /Applications: installalo e riprova."))?;
  open_app_bundle(&bundle, "ChatGPT.app")
}

#[tauri::command]
#[cfg(not(target_os = "macos"))]
pub fn open_chatgpt_app() -> Result<(), String> {
  Err(String::from("L'app ChatGPT è disponibile solo su macOS."))
}

// ---------------------------------------------------------------------------
// ChatGPT.app (Codex desktop) su AI Hive
// ---------------------------------------------------------------------------
// The ChatGPT desktop app reads the same ~/.codex files as the CLI. Since the
// 26.x builds the Responses transport defaults to WebSocket (wss://<host>/api/v1/responses),
// which the Hive gateway does not implement, the app must use a custom
// provider with supports_websockets = false (built-in providers cannot be
// overridden). The provider carries the Hive key via experimental_bearer_token
// because custom providers ignore auth.json; auth.json (apikey mode) is still
// written so the app keeps a coherent auth state. A generated model catalog
// feeds the app picker. The original config.toml/auth.json are backed up
// (.hive.bak) so the previous setup can be restored.
const HIVE_CATALOG_FILE: &str = "hive-models.json";
const HIVE_CONFIG_BACKUP: &str = "config.toml.hive.bak";
const HIVE_AUTH_BACKUP: &str = "auth.json.hive.bak";
const HIVE_PROVIDER_ID: &str = "requrv-hive";

fn codex_dir_in(home: &Path) -> PathBuf {
  home.join(".codex")
}

fn codex_config_path_in(home: &Path) -> PathBuf {
  codex_dir_in(home).join("config.toml")
}

fn codex_config_backup_path_in(home: &Path) -> PathBuf {
  codex_dir_in(home).join(HIVE_CONFIG_BACKUP)
}

fn codex_auth_path_in(home: &Path) -> PathBuf {
  codex_dir_in(home).join("auth.json")
}

fn codex_auth_backup_path_in(home: &Path) -> PathBuf {
  codex_dir_in(home).join(HIVE_AUTH_BACKUP)
}

fn codex_catalog_path_in(home: &Path) -> PathBuf {
  codex_dir_in(home).join(HIVE_CATALOG_FILE)
}

// Catalog entry for the app model picker. The Codex desktop engine's schema
// is strict, so mirror the full field set ollama ships to ChatGPT (models
// without thinking metadata get null/empty reasoning fields).
fn hive_catalog_entry(model: &str, priority: i64) -> serde_json::Value {
  serde_json::json!({
    "slug": model,
    "display_name": model,
    "description": "Modello ReQurv AI Hive",
    "default_reasoning_level": null,
    "supported_reasoning_levels": [],
    "shell_type": "unified_exec",
    "visibility": "list",
    "supported_in_api": true,
    "priority": priority,
    "additional_speed_tiers": [],
    "service_tiers": [],
    "default_service_tier": null,
    "availability_nux": null,
    "upgrade": null,
    "base_instructions": "You are Codex, a coding agent. You and the user share the same workspace and collaborate to achieve the user's goals.",
    "model_messages": null,
    "include_skills_usage_instructions": true,
    "include_plugin_usage_instructions": true,
    "include_apps_usage_instructions": true,
    "supports_reasoning_summary_parameter": false,
    "supports_reasoning_summaries": false,
    "default_reasoning_summary": "auto",
    "support_verbosity": false,
    "default_verbosity": null,
    "apply_patch_tool_type": null,
    "web_search_tool_type": "text",
    "truncation_policy": { "mode": "tokens", "limit": 10_000 },
    "supports_parallel_tool_calls": true,
    "supports_image_detail_original": false,
    "context_window": 128_000,
    "max_context_window": 128_000,
    "auto_compact_token_limit": null,
    "effective_context_window_percent": 95,
    "experimental_supported_tools": [],
    "input_modalities": ["text"],
    "supports_search_tool": true
  })
}

// Catalog for the app picker: only TEXT_GENERATION models; fall back to the
// full list if the gateway stops reporting the type (same rule as the UI).
fn build_hive_catalog(models: &[HiveModel]) -> serde_json::Value {
  let text: Vec<&HiveModel> = models.iter().filter(|m| m.model_type == "TEXT_GENERATION").collect();
  let picked: Vec<&HiveModel> = if text.is_empty() {
    models.iter().collect()
  } else {
    text
  };
  let entries = picked
    .iter()
    .enumerate()
    .map(|(i, m)| hive_catalog_entry(&m.id, i as i64))
    .collect::<Vec<_>>();
  serde_json::json!({ "models": entries })
}

// The Hive provider table when present in a parsed config.
fn hive_provider_table(table: &toml::Table) -> Option<&toml::Table> {
  table
    .get("model_providers")
    .and_then(|v| v.as_table())
    .and_then(|providers| providers.get(HIVE_PROVIDER_ID))
    .and_then(|v| v.as_table())
}

// True when the provider (or its legacy root openai_base_url) points at AI Hive.
fn hive_config_ours(table: &toml::Table) -> bool {
  table.get("openai_base_url").and_then(|v| v.as_str()) == Some(HIVE_OPENAI_BASE_URL)
    || hive_provider_table(table)
      .and_then(|p| p.get("base_url").and_then(|v| v.as_str()))
      == Some(HIVE_OPENAI_BASE_URL)
}

// True when the codex config is pointed at AI Hive by this launcher.
fn chatgpt_app_configured_in(home: &Path) -> bool {
  let Ok(raw) = std::fs::read_to_string(codex_config_path_in(home)) else {
    return false;
  };
  let Ok(table) = raw.parse::<toml::Table>() else {
    return false;
  };
  hive_config_ours(&table)
    && table
      .get("model_catalog_json")
      .and_then(|v| v.as_str())
      == Some(codex_catalog_path_in(home).to_string_lossy().as_ref())
}

// Back up (once) and rewrite the codex config, catalog and auth so the
// ChatGPT app talks to AI Hive. Every other root key is preserved; files that
// cannot be parsed are left untouched.
fn configure_chatgpt_app_in(home: &Path, model: &str, models: &[HiveModel], key: &str) -> Result<(), String> {
  let dir = codex_dir_in(home);
  std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

  let catalog_path = codex_catalog_path_in(home);
  let catalog = build_hive_catalog(models);
  let rendered = serde_json::to_string_pretty(&catalog).map_err(|e| e.to_string())?;
  std::fs::write(&catalog_path, rendered + "\n")
    .map_err(|e| format!("Impossibile scrivere {}: {e}", catalog_path.display()))?;

  let config_path = codex_config_path_in(home);
  let backup_path = codex_config_backup_path_in(home);
  let mut table: toml::Table = if config_path.exists() {
    let raw = std::fs::read_to_string(&config_path)
      .map_err(|e| format!("Impossibile leggere {}: {e}", config_path.display()))?;
    if !backup_path.exists() {
      let _ = std::fs::write(&backup_path, raw.as_str());
    }
    raw.parse().map_err(|e| {
      format!("{} non è un TOML valido: {e}. Correggi il file e riprova.", config_path.display())
    })?
  } else {
    toml::Table::new()
  };
  table.insert("model".into(), toml::Value::String(model.to_string()));
  table.insert("model_provider".into(), toml::Value::String(HIVE_PROVIDER_ID.into()));
  table.insert("model_catalog_json".into(), toml::Value::String(catalog_path.to_string_lossy().into_owned()));
  // Legacy layouts pointed the root openai_base_url at Hive; the provider
  // table supersedes it, so drop it when reconfiguring.
  table.remove("openai_base_url");
  let mut provider = toml::Table::new();
  provider.insert("name".into(), toml::Value::String("ReQurv AI Hive".into()));
  provider.insert("base_url".into(), toml::Value::String(HIVE_OPENAI_BASE_URL.into()));
  provider.insert("wire_api".into(), toml::Value::String("responses".into()));
  provider.insert("supports_websockets".into(), toml::Value::Boolean(false));
  provider.insert("experimental_bearer_token".into(), toml::Value::String(key.to_string()));
  let providers = table
    .entry("model_providers")
    .or_insert_with(|| toml::Value::Table(toml::Table::new()));
  let providers = providers
    .as_table_mut()
    .ok_or_else(|| "model_providers non modificabile in config.toml".to_string())?;
  providers.insert(HIVE_PROVIDER_ID.into(), toml::Value::Table(provider));
  let rendered = toml::to_string(&table).map_err(|e| e.to_string())?;
  std::fs::write(&config_path, rendered)
    .map_err(|e| format!("Impossibile scrivere {}: {e}", config_path.display()))?;

  // The app authenticates through auth.json: the Hive key goes in apikey mode,
  // keeping the previous content in .bak for the restore.
  let auth_path = codex_auth_path_in(home);
  let auth_backup = codex_auth_backup_path_in(home);
  if auth_path.exists() && !auth_backup.exists() {
    let raw = std::fs::read_to_string(&auth_path)
      .map_err(|e| format!("Impossibile leggere {}: {e}", auth_path.display()))?;
    let _ = std::fs::write(&auth_backup, raw.as_str());
  }
  let auth = serde_json::json!({
    "OPENAI_API_KEY": key,
    "auth_mode": "apikey"
  });
  let rendered = serde_json::to_string_pretty(&auth).map_err(|e| e.to_string())?;
  std::fs::write(&auth_path, rendered + "\n")
    .map_err(|e| format!("Impossibile scrivere {}: {e}", auth_path.display()))?;
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&auth_path, std::fs::Permissions::from_mode(0o600));
  }

  Ok(())
}

// Undo the Hive configuration: restore the backed-up files, or strip the root
// keys and the apikey auth managed by this launcher when no backup exists. A
// ChatGPT OAuth auth.json is never touched.
fn restore_chatgpt_app_in(home: &Path, key: &str) -> Result<(), String> {
  let config_path = codex_config_path_in(home);
  let backup_path = codex_config_backup_path_in(home);
  let mut strip_catalog = false;

  if config_path.exists() || backup_path.exists() {
    if backup_path.exists() {
      let raw = std::fs::read_to_string(&backup_path)
        .map_err(|e| format!("Impossibile leggere {}: {e}", backup_path.display()))?;
      std::fs::write(&config_path, raw)
        .map_err(|e| format!("Impossibile scrivere {}: {e}", config_path.display()))?;
      std::fs::remove_file(&backup_path).map_err(|e| e.to_string())?;
      strip_catalog = true;
    } else {
      let raw = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Impossibile leggere {}: {e}", config_path.display()))?;
      let mut table: toml::Table = raw.parse().map_err(|e| {
        format!("{} non è un TOML valido: {e}. Correggi il file e riprova.", config_path.display())
      })?;
      let is_hive = hive_config_ours(&table);
      if is_hive {
        table.remove("model");
        table.remove("openai_base_url");
        table.remove("model_catalog_json");
        if table.get("model_provider").and_then(|v| v.as_str()) == Some(HIVE_PROVIDER_ID) {
          table.remove("model_provider");
        }
        if let Some(providers) = table.get_mut("model_providers").and_then(|v| v.as_table_mut()) {
          let ours = providers
            .get(HIVE_PROVIDER_ID)
            .and_then(|p| p.as_table())
            .and_then(|p| p.get("base_url").and_then(|v| v.as_str()))
            == Some(HIVE_OPENAI_BASE_URL);
          if ours {
            providers.remove(HIVE_PROVIDER_ID);
          }
          if providers.is_empty() {
            table.remove("model_providers");
          }
        }
        if table.is_empty() {
          std::fs::remove_file(&config_path).map_err(|e| e.to_string())?;
        } else {
          let rendered = toml::to_string(&table).map_err(|e| e.to_string())?;
          std::fs::write(&config_path, rendered)
            .map_err(|e| format!("Impossibile scrivere {}: {e}", config_path.display()))?;
        }
        strip_catalog = true;
      }
    }
  }

  let auth_path = codex_auth_path_in(home);
  let auth_backup = codex_auth_backup_path_in(home);
  if auth_path.exists() {
    if auth_backup.exists() {
      let raw = std::fs::read_to_string(&auth_backup)
        .map_err(|e| format!("Impossibile leggere {}: {e}", auth_backup.display()))?;
      std::fs::write(&auth_path, raw)
        .map_err(|e| format!("Impossibile scrivere {}: {e}", auth_path.display()))?;
      std::fs::remove_file(&auth_backup).map_err(|e| e.to_string())?;
    } else if let Ok(raw) = std::fs::read_to_string(&auth_path) {
      // Without a backup the auth was created by this launcher: remove it only
      // while it still holds the Hive key, never a user login or other key.
      let is_ours = serde_json::from_str::<serde_json::Value>(&raw)
        .map(|v| {
          v.get("auth_mode").and_then(|m| m.as_str()) == Some("apikey")
            && v.get("OPENAI_API_KEY").and_then(|k| k.as_str()) == Some(key)
        })
        .unwrap_or(false);
      if is_ours {
        std::fs::remove_file(&auth_path).map_err(|e| e.to_string())?;
      }
    }
  }

  if strip_catalog {
    let _ = std::fs::remove_file(codex_catalog_path_in(home));
  }

  Ok(())
}

// The model catalog is read at startup, so a running instance keeps the old
// models until it is restarted.
#[cfg(target_os = "macos")]
fn chatgpt_app_running() -> bool {
  Command::new("pgrep")
    .args(["-x", "ChatGPT"])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .is_ok_and(|s| s.success())
}

#[cfg(not(target_os = "macos"))]
fn chatgpt_app_running() -> bool {
  false
}

// Quit ChatGPT (gracefully, then forcefully) and relaunch it so the app picks
// up the new model catalog.
#[cfg(target_os = "macos")]
fn quit_and_reopen_chatgpt() -> Result<(), String> {
  let bundle = chatgpt_app_bundle()
    .ok_or_else(|| String::from("ChatGPT.app non trovato in /Applications: installalo e riprova."))?;
  if chatgpt_app_running() {
    Command::new("osascript")
      .args(["-e", "tell application \"ChatGPT\" to quit"])
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(Stdio::null())
      .spawn()
      .map(|_| ())
      .map_err(|e| format!("Impossibile chiudere ChatGPT: {e}"))?;
    // Graceful quit can take a moment; give it a chance before forcing.
    for _ in 0..20 {
      if !chatgpt_app_running() {
        break;
      }
      std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if chatgpt_app_running() {
      Command::new("pkill")
        .args(["-x", "ChatGPT"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Impossibile chiudere ChatGPT: {e}"))?;
      for _ in 0..10 {
        if !chatgpt_app_running() {
          break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
      }
    }
  }
  open_app_bundle(&bundle, "ChatGPT.app")
}

#[cfg(not(target_os = "macos"))]
fn quit_and_reopen_chatgpt() -> Result<(), String> {
  Err(String::from("L'app ChatGPT è disponibile solo su macOS."))
}

#[derive(Serialize)]
pub struct AppRestartResult {
  pub restart_required: bool,
}

// Point ChatGPT.app at AI Hive (config, catalog, auth) and report whether a
// running instance needs a restart to load the new catalog.
#[tauri::command]
pub async fn configure_chatgpt_app(model: String, key: String, models: Vec<HiveModel>) -> Result<AppRestartResult, String> {
  let model = model.trim();
  let key = key.trim();
  if model.is_empty() {
    return Err("Nessun modello selezionato".into());
  }
  if key.is_empty() {
    return Err("Chiave Hive non salvata".into());
  }
  if models.is_empty() {
    return Err("Nessun modello disponibile da AI Hive".into());
  }
  assert_responses_available(key).await?;
  let home = home_dir().ok_or_else(|| "Home directory non trovata".to_string())?;
  configure_chatgpt_app_in(&home, model, &models, key)?;
  Ok(AppRestartResult {
    restart_required: chatgpt_app_running(),
  })
}

#[tauri::command]
pub fn restart_chatgpt_app() -> Result<(), String> {
  quit_and_reopen_chatgpt()
}

// Restore the original ChatGPT setup (config.toml, auth.json, catalog) and
// report whether a running instance needs a restart to pick it up.
#[tauri::command]
pub fn restore_chatgpt_app(app: tauri::AppHandle) -> Result<AppRestartResult, String> {
  let home = home_dir().ok_or_else(|| "Home directory non trovata".to_string())?;
  let key = get_hive_key(app).ok().flatten().unwrap_or_default();
  restore_chatgpt_app_in(&home, &key)?;
  Ok(AppRestartResult {
    restart_required: chatgpt_app_running(),
  })
}

// Note on Claude Desktop: unlike ChatGPT.app, the Claude desktop app cannot be
// pointed at AI Hive. Its Code tab runs cloud sessions authenticated with the
// claude.ai account (CLAUDE_CODE_OAUTH_TOKEN, subscription-scoped model
// catalog) and ignores ~/.claude/settings.json (the SDK is launched with empty
// setting sources); custom gateways are an enterprise-only ("3p") feature that
// is disabled in consumer builds. So Claude Code is terminal-only here.

async fn launch_codex_cli(app: &tauri::AppHandle, model: &str, key: &str) -> Result<(), String> {
  let bin = find_service_binary("codex")
    .or_else(codex_app_binary)
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

// Claude Code speaks the Anthropic Messages API, so the launcher only sets
// env vars and never writes to ~/.claude: ANTHROPIC_BASE_URL routes the client
// through the gateway (it appends /v1/messages), ANTHROPIC_API_KEY is sent as
// the x-api-key header in place of a Claude subscription, and ANTHROPIC_MODEL
// pins the selected Hive model. No file is persisted, so nothing to restore.
async fn launch_claude_cli(app: &tauri::AppHandle, model: &str, key: &str) -> Result<(), String> {
  let bin = find_service_binary("claude")
    .ok_or_else(|| String::from("Claude Code non è installato. Installalo con npm install -g @anthropic-ai/claude-code."))?;

  assert_messages_available(key).await?;

  let env: Vec<(&str, &str)> = vec![
    ("ANTHROPIC_BASE_URL", HIVE_ANTHROPIC_BASE_URL),
    ("ANTHROPIC_API_KEY", key),
    ("ANTHROPIC_MODEL", model),
    // The model is not in the client's catalog: pin a sane context window
    // instead of letting auto-compact assume 200k.
    ("CLAUDE_CODE_MAX_CONTEXT_TOKENS", "128000"),
  ];
  launch_cli(app, "claude", &bin, &[], &env)
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

  // Claude Code non prende argomenti: il terminale lo punta ad AI Hive solo
  // con le variabili d'ambiente, senza toccare ~/.claude.
  #[test]
  fn builds_claude_terminal_script() {
    let script = build_terminal_script(
      Path::new("/Users/x/.local/bin/claude"),
      &[],
      &[
        ("ANTHROPIC_BASE_URL", HIVE_ANTHROPIC_BASE_URL),
        ("ANTHROPIC_API_KEY", "sk-test"),
        ("ANTHROPIC_MODEL", "model-a"),
        ("CLAUDE_CODE_MAX_CONTEXT_TOKENS", "128000"),
      ],
    );
    assert!(script.contains("export ANTHROPIC_BASE_URL='https://hive.requrv.ai/api'"));
    assert!(script.contains("export ANTHROPIC_API_KEY='sk-test'"));
    assert!(script.contains("export ANTHROPIC_MODEL='model-a'"));
    assert!(script.contains("export CLAUDE_CODE_MAX_CONTEXT_TOKENS='128000'"));
    assert!(script.contains("'/Users/x/.local/bin/claude'"));
    assert!(!script.contains("--"));
  }

  // Il gateway risponde 422 al POST di prova con corpo vuoto (validazione del
  // body prima dell'invocazione del modello): qualsiasi 4xx dal handler prova
  // che il percorso esiste. Solo 404 e 5xx bloccano l'avvio.
  #[test]
  fn probe_treats_handler_rejections_as_available() {
    for status in [
      reqwest::StatusCode::OK,
      reqwest::StatusCode::UNAUTHORIZED,
      reqwest::StatusCode::FORBIDDEN,
      reqwest::StatusCode::METHOD_NOT_ALLOWED,
      reqwest::StatusCode::UNPROCESSABLE_ENTITY,
      reqwest::StatusCode::TOO_MANY_REQUESTS,
    ] {
      assert_eq!(probe_error(status, "Codex", "/responses"), None, "status {status}");
    }
    let missing = probe_error(reqwest::StatusCode::NOT_FOUND, "Claude Code", "/messages").expect("404");
    assert!(missing.contains("Claude Code"));
    assert!(missing.contains("/messages"));
    assert!(probe_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "Codex", "/responses").is_some());
    assert!(probe_error(reqwest::StatusCode::BAD_GATEWAY, "Codex", "/responses").is_some());
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

  // Simula un'installazione under nvm con PATH minimo e HOME fittizia (app
  // lanciata dal Dock/Finder): la rilevazione deve trovare il bin nella
  // directory bin della versione node, senza passare dal PATH.
  #[cfg(not(windows))]
  #[test]
  fn finds_codex_in_nvm_bin_with_minimal_path() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-nvm-{}", std::process::id()));
    let bin_dir = tmp.join(".nvm").join("versions").join("node").join("v24.0.0").join("bin");
    std::fs::create_dir_all(&bin_dir).expect("create fake nvm bin");
    let fake = bin_dir.join("codex");
    std::fs::write(&fake, "#!/bin/sh\n").expect("write fake codex");

    let original_path = std::env::var("PATH").unwrap_or_default();
    let original_home = std::env::var("HOME").unwrap_or_default();
    std::env::set_var("PATH", "/usr/bin:/bin");
    std::env::set_var("HOME", &tmp);
    let bin = find_service_binary("codex");
    std::env::set_var("PATH", &original_path);
    std::env::set_var("HOME", &original_home);

    assert_eq!(bin, Some(fake));
    let _ = std::fs::remove_dir_all(&tmp);
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
  fn finds_desktop_app_bundle_among_candidates() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-app-{}", std::process::id()));
    let bundle = tmp.join("OpenCode.app");
    std::fs::create_dir_all(&bundle).expect("create fake bundle");
    let missing = tmp.join("Assente.app");
    assert_eq!(app_bundle_path_in(&[missing.clone(), bundle.clone()]), Some(bundle));
    assert_eq!(app_bundle_path_in(&[missing]), None);
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn finds_chatgpt_bundle_among_candidates() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-chatgpt-bundle-{}", std::process::id()));
    let bundle = tmp.join("ChatGPT.app");
    std::fs::create_dir_all(&bundle).expect("create fake bundle");
    let missing = tmp.join("Assente.app");
    assert_eq!(app_bundle_path_in(&[missing.clone(), bundle.clone()]), Some(bundle));
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn finds_codex_binary_in_chatgpt_bundle() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-chatgpt-{}", std::process::id()));
    let resources = tmp.join("ChatGPT.app").join("Contents").join("Resources");
    std::fs::create_dir_all(&resources).expect("create fake bundle");
    std::fs::write(resources.join("codex"), "fake").expect("write fake codex");
    let bundle = tmp.join("ChatGPT.app");
    let expected = bundle.join("Contents").join("Resources").join("codex");
    let missing = tmp.join("Assente.app");
    assert_eq!(codex_app_binary_in(&[missing.clone(), bundle.clone()]), Some(expected));
    assert_eq!(codex_app_binary_in(&[missing]), None);
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn builds_hive_model_catalog_with_text_models_only() {
    let models = vec![
      HiveModel { id: "model-a".into(), model_type: "TEXT_GENERATION".into() },
      HiveModel { id: "image-x".into(), model_type: "IMAGE_GENERATION".into() },
      HiveModel { id: "model-b".into(), model_type: "TEXT_GENERATION".into() },
    ];
    let catalog = build_hive_catalog(&models);
    let entries = catalog["models"].as_array().expect("models array");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["slug"], "model-a");
    assert_eq!(entries[1]["slug"], "model-b");
    assert_eq!(entries[0]["display_name"], "model-a");
    assert_eq!(entries[0]["supported_in_api"], true);
    assert_eq!(entries[0]["context_window"], 128_000);
    // The app schema requires the reasoning fields even when empty.
    assert_eq!(entries[0]["supported_reasoning_levels"].as_array().unwrap().len(), 0);
    assert!(entries[0].get("default_reasoning_level").is_some());
  }

  #[test]
  fn hive_catalog_falls_back_to_full_list_without_type() {
    let models = vec![HiveModel { id: "model-x".into(), model_type: String::new() }];
    let catalog = build_hive_catalog(&models);
    assert_eq!(catalog["models"].as_array().expect("models array").len(), 1);
  }

  #[test]
  fn configure_chatgpt_app_writes_config_catalog_auth_and_backups() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-chatgpt-cfg-{}", std::process::id()));
    let dir = tmp.join(".codex");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let original_config = format!(
      "model = \"gpt-5-codex\"\nnotify = [\"bar\"]\nopenai_base_url = \"{HIVE_OPENAI_BASE_URL}\"\n[desktop]\ntheme = \"dark\"\n"
    );
    let original_auth = r#"{"auth_mode": "chatgpt"}"#;
    std::fs::write(dir.join("config.toml"), original_config.as_str()).expect("write config");
    std::fs::write(dir.join("auth.json"), original_auth).expect("write auth");

    let models = vec![HiveModel { id: "model-a".into(), model_type: "TEXT_GENERATION".into() }];
    configure_chatgpt_app_in(&tmp, "model-a", &models, "requrv_sk_test").expect("configure");

    let rendered = std::fs::read_to_string(dir.join("config.toml")).expect("read config back");
    let value: toml::Table = rendered.parse().expect("valid toml");
    assert_eq!(value.get("model").and_then(|v| v.as_str()), Some("model-a"));
    assert_eq!(value.get("model_provider").and_then(|v| v.as_str()), Some(HIVE_PROVIDER_ID));
    // The legacy root key is superseded by the provider table.
    assert!(value.get("openai_base_url").is_none());
    assert!(value
      .get("model_catalog_json")
      .and_then(|v| v.as_str())
      .unwrap()
      .ends_with("hive-models.json"));
    let provider = value
      .get("model_providers")
      .and_then(|v| v.as_table())
      .and_then(|p| p.get(HIVE_PROVIDER_ID))
      .and_then(|v| v.as_table())
      .expect("hive provider table");
    assert_eq!(provider.get("base_url").and_then(|v| v.as_str()), Some(HIVE_OPENAI_BASE_URL));
    assert_eq!(provider.get("wire_api").and_then(|v| v.as_str()), Some("responses"));
    assert_eq!(provider.get("supports_websockets").and_then(|v| v.as_bool()), Some(false));
    assert_eq!(
      provider.get("experimental_bearer_token").and_then(|v| v.as_str()),
      Some("requrv_sk_test")
    );
    assert_eq!(value.get("notify").and_then(|v| v.as_array()).unwrap()[0].as_str(), Some("bar"));
    assert_eq!(
      value.get("desktop").and_then(|v| v.as_table()).and_then(|t| t.get("theme")).and_then(|v| v.as_str()),
      Some("dark")
    );

    let catalog_raw = std::fs::read_to_string(dir.join("hive-models.json")).expect("read catalog");
    let catalog: serde_json::Value = serde_json::from_str(&catalog_raw).expect("valid catalog");
    assert_eq!(catalog["models"][0]["slug"], "model-a");

    let auth_raw = std::fs::read_to_string(dir.join("auth.json")).expect("read auth back");
    let auth: serde_json::Value = serde_json::from_str(&auth_raw).expect("valid auth");
    assert_eq!(auth["auth_mode"], "apikey");
    assert_eq!(auth["OPENAI_API_KEY"], "requrv_sk_test");

    assert_eq!(
      std::fs::read_to_string(dir.join("config.toml.hive.bak")).expect("config backup"),
      original_config
    );
    assert_eq!(
      std::fs::read_to_string(dir.join("auth.json.hive.bak")).expect("auth backup"),
      original_auth
    );

    assert!(chatgpt_app_configured_in(&tmp));
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn configure_twice_keeps_the_original_backup() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-chatgpt-again-{}", std::process::id()));
    let dir = tmp.join(".codex");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let original = "model = \"gpt-5-codex\"\n";
    std::fs::write(dir.join("config.toml"), original).expect("write config");

    let models = vec![
      HiveModel { id: "model-a".into(), model_type: "TEXT_GENERATION".into() },
      HiveModel { id: "model-b".into(), model_type: "TEXT_GENERATION".into() },
    ];
    configure_chatgpt_app_in(&tmp, "model-a", &models, "requrv_sk_test").expect("first configure");
    configure_chatgpt_app_in(&tmp, "model-b", &models, "requrv_sk_other").expect("second configure");

    let value: toml::Table = std::fs::read_to_string(dir.join("config.toml")).expect("read back").parse().expect("valid toml");
    assert_eq!(value.get("model").and_then(|v| v.as_str()), Some("model-b"));
    assert_eq!(std::fs::read_to_string(dir.join("config.toml.hive.bak")).expect("original backup"), original);
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn restore_chatgpt_app_roundtrip() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-chatgpt-restore-{}", std::process::id()));
    let dir = tmp.join(".codex");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let original_config = "model = \"gpt-5-codex\"\n";
    let original_auth = r#"{"auth_mode": "chatgpt"}"#;
    std::fs::write(dir.join("config.toml"), original_config).expect("write config");
    std::fs::write(dir.join("auth.json"), original_auth).expect("write auth");

    let models = vec![HiveModel { id: "model-a".into(), model_type: "TEXT_GENERATION".into() }];
    configure_chatgpt_app_in(&tmp, "model-a", &models, "requrv_sk_test").expect("configure");
    restore_chatgpt_app_in(&tmp, "requrv_sk_test").expect("restore");

    assert_eq!(std::fs::read_to_string(dir.join("config.toml")).expect("config restored"), original_config);
    assert_eq!(std::fs::read_to_string(dir.join("auth.json")).expect("auth restored"), original_auth);
    assert!(!dir.join("config.toml.hive.bak").exists());
    assert!(!dir.join("auth.json.hive.bak").exists());
    assert!(!dir.join("hive-models.json").exists());
    assert!(!chatgpt_app_configured_in(&tmp));
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn restore_without_backup_strips_legacy_hive_keys_and_own_auth() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-chatgpt-strip-{}", std::process::id()));
    let dir = tmp.join(".codex");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let configured = format!(
      "model = \"model-a\"\nnotify = [\"bar\"]\nopenai_base_url = \"{HIVE_OPENAI_BASE_URL}\"\nmodel_catalog_json = \"{}\"\n",
      dir.join("hive-models.json").display()
    );
    std::fs::write(dir.join("config.toml"), configured).expect("write config");
    std::fs::write(dir.join("auth.json"), r#"{"OPENAI_API_KEY": "requrv_sk_test", "auth_mode": "apikey"}"#)
      .expect("write auth");

    restore_chatgpt_app_in(&tmp, "requrv_sk_test").expect("restore");

    let value: toml::Table = std::fs::read_to_string(dir.join("config.toml")).expect("config kept").parse().expect("valid toml");
    assert!(value.get("model").is_none());
    assert!(value.get("model_provider").is_none());
    assert!(value.get("openai_base_url").is_none());
    assert!(value.get("model_catalog_json").is_none());
    assert_eq!(value.get("notify").and_then(|v| v.as_array()).unwrap()[0].as_str(), Some("bar"));
    assert!(!dir.join("auth.json").exists());
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn restore_without_backup_strips_hive_provider_and_keeps_foreign_ones() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-chatgpt-provider-{}", std::process::id()));
    let dir = tmp.join(".codex");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let configured = format!(
      "model = \"model-a\"\nnotify = [\"bar\"]\nmodel_provider = \"{HIVE_PROVIDER_ID}\"\nmodel_catalog_json = \"{}\"\n\n[model_providers.{HIVE_PROVIDER_ID}]\nbase_url = \"{HIVE_OPENAI_BASE_URL}\"\nwire_api = \"responses\"\nsupports_websockets = false\n\n[model_providers.other]\nbase_url = \"https://example.com/v1\"\n",
      dir.join("hive-models.json").display()
    );
    std::fs::write(dir.join("config.toml"), configured).expect("write config");

    restore_chatgpt_app_in(&tmp, "requrv_sk_test").expect("restore");

    let value: toml::Table = std::fs::read_to_string(dir.join("config.toml")).expect("config kept").parse().expect("valid toml");
    assert!(value.get("model").is_none());
    assert!(value.get("model_provider").is_none());
    assert!(value.get("model_catalog_json").is_none());
    let providers = value.get("model_providers").and_then(|v| v.as_table()).expect("providers kept");
    assert!(providers.get(HIVE_PROVIDER_ID).is_none());
    assert_eq!(
      providers.get("other").and_then(|p| p.as_table()).and_then(|p| p.get("base_url")).and_then(|v| v.as_str()),
      Some("https://example.com/v1")
    );
    assert_eq!(value.get("notify").and_then(|v| v.as_array()).unwrap()[0].as_str(), Some("bar"));
    let _ = std::fs::remove_dir_all(&tmp);
  }

  #[test]
  fn restore_without_backup_keeps_foreign_auth() {
    let tmp = std::env::temp_dir().join(format!("requrv-launch-test-chatgpt-auth-{}", std::process::id()));
    let dir = tmp.join(".codex");
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let configured = format!("model = \"model-a\"\nopenai_base_url = \"{HIVE_OPENAI_BASE_URL}\"\n");
    std::fs::write(dir.join("config.toml"), configured).expect("write config");
    let foreign_auth = r#"{"OPENAI_API_KEY": "sk-altra-chiave", "auth_mode": "apikey"}"#;
    std::fs::write(dir.join("auth.json"), foreign_auth).expect("write auth");

    restore_chatgpt_app_in(&tmp, "requrv_sk_test").expect("restore");

    assert_eq!(std::fs::read_to_string(dir.join("auth.json")).expect("auth kept"), foreign_auth);
    let _ = std::fs::remove_dir_all(&tmp);
  }
}
