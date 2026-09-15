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

#[tauri::command]
pub async fn list_hive_models(key: String) -> Result<Vec<String>, String> {
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
        .filter_map(|m| m.get("id").and_then(|id| id.as_str()).map(|s| s.to_string()))
        .collect()
    })
    .unwrap_or_default();

  Ok(models)
}

#[derive(Serialize)]
pub struct ServiceStatus {
  pub opencode: bool,
  pub codex: bool,
}

#[tauri::command]
pub fn check_services() -> ServiceStatus {
  ServiceStatus {
    opencode: find_service_binary("opencode").is_some(),
    codex: find_service_binary("codex").is_some(),
  }
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
  let npm = find_on_path("npm")?;
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

#[tauri::command]
pub fn launch_service(service: String, model: String, key: String) -> Result<(), String> {
  let model = model.trim();
  let key = key.trim();
  if model.is_empty() {
    return Err("Nessun modello selezionato".into());
  }
  if key.is_empty() {
    return Err("Chiave Hive non salvata".into());
  }
  match service.as_str() {
    "opencode" => launch_opencode(model, key),
    "codex" => launch_codex(model, key),
    _ => Err(format!("Servizio sconosciuto: {service}")),
  }
}

fn launch_opencode(model: &str, key: &str) -> Result<(), String> {
  let bin = find_service_binary("opencode")
    .ok_or_else(|| String::from("OpenCode non è installato. Installalo con: npm i -g opencode-ai"))?;

  let config = serde_json::json!({
    "$schema": "https://opencode.ai/config.json",
    "provider": {
      "hive": {
        "npm": "@ai-sdk/openai-compatible",
        "name": "ReQurv AI Hive",
        "options": {
          "baseURL": HIVE_OPENAI_BASE_URL,
          "apiKey": key,
        },
        "models": {
          model: { "name": model },
        },
      },
    },
    "model": format!("hive/{model}"),
  });
  let config_content = config.to_string();
  let env: Vec<(&str, &str)> = vec![("OPENCODE_CONFIG_CONTENT", config_content.as_str())];

  spawn_cli(&bin, &[], &env)
}

fn launch_codex(model: &str, key: &str) -> Result<(), String> {
  let bin = find_service_binary("codex")
    .ok_or_else(|| String::from("Codex non è installato. Installalo con: npm i -g @openai/codex"))?;

  let codex_dir = home_dir()
    .ok_or_else(|| "Home directory non trovata".to_string())?
    .join(".codex");
  std::fs::create_dir_all(&codex_dir).map_err(|e| e.to_string())?;

  let profile_path = codex_dir.join("hive.config.toml");
  if let Ok(old) = std::fs::read_to_string(&profile_path) {
    let _ = std::fs::write(profile_path.with_extension("toml.bak"), old);
  }

  let profile = format!(
    "model = \"{model}\"\nmodel_provider = \"hive\"\n\n[model_providers.hive]\nname = \"ReQurv AI Hive\"\nbase_url = \"{base}\"\nwire_api = \"chat\"\nenv_key = \"HIVE_API_KEY\"\n",
    base = HIVE_OPENAI_BASE_URL,
  );
  std::fs::write(&profile_path, profile).map_err(|e| e.to_string())?;

  let args = vec!["--profile".to_string(), "hive".to_string()];
  let env: Vec<(&str, &str)> = vec![("HIVE_API_KEY", key)];
  spawn_cli(&bin, &args, &env)
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
}
