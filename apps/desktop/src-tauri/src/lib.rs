//! warsha-desktop — the Tauri 2 shell around the `warshad` daemon.
//!
//! The window is a webview running `apps/desktop/src`, and that frontend talks to the daemon
//! over loopback HTTP (`POST <base_url>/rpc`) and its `/events` socket. It therefore needs two
//! things the browser build cannot discover: the port the daemon actually bound, and the bearer
//! token it expects. Both travel to the webview through `get_config` — `daemon_url`,
//! `daemon_ws_url` and `daemon_token` — which is the same Tauri command the frontend already
//! uses for `onboarding_complete` and `app_mode`, so no new channel, permission or event had to
//! be invented for it.
//!
//! The daemon runs *in this process*: `warshad::start_daemon` is the library entry point the
//! `warshad` binary itself calls, so the shell gets the bound ports back directly and can shut
//! the daemon down on exit. The reasoning, and why a sidecar was not used, is in `DECISIONS.md`.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{Manager, RunEvent, State};
use warshad::{DaemonConfig, DaemonError, DaemonHandle};

/// Config key reporting the daemon's JSON-RPC base URL, e.g. `http://127.0.0.1:8766`.
pub const CONFIG_DAEMON_URL: &str = "daemon_url";

/// Config key reporting the daemon's `/events` endpoint, e.g. `ws://127.0.0.1:8765`.
pub const CONFIG_DAEMON_WS_URL: &str = "daemon_ws_url";

/// Config key reporting the bearer token the daemon expects on every request.
pub const CONFIG_DAEMON_TOKEN: &str = "daemon_token";

/// Config key reporting the data directory the daemon and desktop settings live in.
pub const CONFIG_DATA_DIR: &str = "data_dir";

/// Config key naming where the daemon the frontend talks to lives: `local` or `server`.
///
/// Unset is `local`, which is the behaviour every earlier build had: the three `daemon_*`
/// keys answer from the daemon this process started (or found already running). `server`
/// makes them answer from the values the owner stored in `desktop.json` instead, which is
/// how the onboarding wizard points a desktop install at a daemon on another machine. The
/// local daemon keeps starting either way, so switching back is one write and no restart.
pub const CONFIG_CONNECTION_MODE: &str = "connection_mode";

/// The two values [`CONFIG_CONNECTION_MODE`] accepts.
const CONNECTION_MODE_LOCAL: &str = "local";
const CONNECTION_MODE_SERVER: &str = "server";

/// The one key the shell alone answers: `set_config` refuses it, so a stray write from the
/// webview cannot alter where the daemon's data lives.
///
/// The three `daemon_*` keys used to be here too. They are writable now — that is what
/// `connection_mode = server` needs — and the local endpoint they used to report is still
/// answered from the shell's own state, never from a stale file, because the stored values
/// are only served when the owner chose `server` and gave a non-empty `daemon_url`.
const SHELL_OWNED_KEYS: [&str; 1] = [CONFIG_DATA_DIR];

/// How long `test_daemon_connection` waits for an answer before calling an address unreachable.
const DAEMON_TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Name of the settings file the non-daemon preferences live in, inside the daemon's data dir.
///
/// `onboarding_complete` and `app_mode` are the two keys the frontend writes today; keeping them
/// beside the daemon's own state means one directory to back up, delete or inspect for the whole
/// app.
const SETTINGS_FILE: &str = "desktop.json";

/// Name of the plain-text log this shell appends start-up failures to.
///
/// A release Windows build has no console (`windows_subsystem = "windows"` in `main.rs`), so an
/// `eprintln!` on a machine whose daemon refuses to start would be invisible; the installer
/// documentation points at this file instead.
const LOG_FILE: &str = "desktop.log";

/// Where the daemon is, and the token that reaches it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DaemonEndpoint {
    /// JSON-RPC base URL — the frontend's `createClient` argument.
    pub base_url: String,
    /// The `/events` WebSocket endpoint.
    pub ws_url: String,
    /// Bearer token the daemon was started with.
    pub token: String,
    /// `true` when this shell started the daemon and therefore stops it on exit. A second launch
    /// of the app (or a daemon someone started with `warshad`) reports `false`: it uses that
    /// daemon and leaves it running.
    pub owned: bool,
}

/// The shell's live state: the daemon it owns, where it is, and the webview's own preferences.
pub struct ShellState {
    endpoint: DaemonEndpoint,
    /// The daemon this shell started, kept so `RunEvent::Exit` can stop it.
    process: Mutex<Option<DaemonHandle>>,
    settings: Mutex<BTreeMap<String, String>>,
    settings_path: PathBuf,
    log_path: PathBuf,
    data_dir: PathBuf,
}

impl ShellState {
    /// The webview's preferences, with a poisoned lock read as the map it holds.
    ///
    /// A `Mutex` is poisoned when its holder panicked, and the map behind it is a plain
    /// `BTreeMap` that did not participate: answering "no settings" (on a read) or
    /// "cannot save" (on a write) would throw away the owner's mode and theme because of
    /// a panic somewhere else in the shell. This is the same reading
    /// `InMemorySecretStore` applies to its own map.
    fn preferences(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, String>> {
        match self.settings.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The value `get_config` answers for `key`.
    ///
    /// The daemon keys are answered from the endpoint this shell started — or, when the owner
    /// switched [`CONFIG_CONNECTION_MODE`] to `server` and stored a `daemon_url`, from the server
    /// values in `desktop.json`. `data_dir` is always the shell's own. Every other key is the
    /// webview's own, and an unknown key is the empty string — which is what the frontend's
    /// `checkOnboarding` already expects for "not set yet".
    fn get(&self, key: &str) -> String {
        match key {
            CONFIG_DAEMON_URL | CONFIG_DAEMON_WS_URL | CONFIG_DAEMON_TOKEN => {
                self.daemon_value(key)
            }
            CONFIG_CONNECTION_MODE => self.connection_mode(),
            CONFIG_DATA_DIR => self.data_dir.display().to_string(),
            _ => self.preferences().get(key).cloned().unwrap_or_default(),
        }
    }

    /// Where the frontend should look for the daemon: `local` until the owner says otherwise.
    fn connection_mode(&self) -> String {
        match self.preferences().get(CONFIG_CONNECTION_MODE) {
            Some(mode) if mode.trim().eq_ignore_ascii_case(CONNECTION_MODE_SERVER) => {
                CONNECTION_MODE_SERVER.to_string()
            }
            _ => CONNECTION_MODE_LOCAL.to_string(),
        }
    }

    /// One of the three `daemon_*` keys: the stored server value, or the local endpoint.
    ///
    /// The switch is the owner's stored `connection_mode` **and** a non-empty stored
    /// `daemon_url`: a mode without an address is not a server to talk to, so the local endpoint
    /// stays the answer instead of an empty string the frontend would have to guess about. A
    /// single stored key that is empty (a token the owner has not typed yet) falls back to the
    /// local endpoint's value for that key rather than leaving the frontend with nothing.
    fn daemon_value(&self, key: &str) -> String {
        let local = match key {
            CONFIG_DAEMON_URL => self.endpoint.base_url.clone(),
            CONFIG_DAEMON_WS_URL => self.endpoint.ws_url.clone(),
            _ => self.endpoint.token.clone(),
        };
        if self.connection_mode() != CONNECTION_MODE_SERVER {
            return local;
        }
        let preferences = self.preferences();
        let stored_url = preferences
            .get(CONFIG_DAEMON_URL)
            .map(|url| url.trim().to_string())
            .unwrap_or_default();
        if stored_url.is_empty() {
            return local;
        }
        match preferences.get(key).map(|value| value.trim().to_string()) {
            Some(value) if !value.is_empty() => value,
            _ => local,
        }
    }

    /// The value one writable key is stored as, after the validation its shape needs.
    ///
    /// The three server keys are validated here rather than in the webview because this is the
    /// process that will hand them to the frontend and, for `daemon_ws_url`, derive what the
    /// owner left blank: a URL that does not parse is refused with a sentence the settings screen
    /// can show, not stored and then failed at connect time.
    fn stored_value(&self, key: &str, value: &str) -> Result<String, String> {
        let trimmed = value.trim();
        match key {
            CONFIG_DAEMON_URL => {
                if trimmed.is_empty() {
                    Ok(String::new())
                } else {
                    http_base(trimmed).ok_or_else(|| {
                        "daemon_url must be an http:// or https:// address, or empty to clear it"
                            .to_string()
                    })
                }
            }
            CONFIG_DAEMON_WS_URL => {
                if trimmed.is_empty() {
                    // An empty ws url is not a value to store: it is the owner asking for the one
                    // that matches the address they gave. The stored server url answers when
                    // there is one, the local endpoint otherwise, so clearing the field always
                    // leaves a usable pair behind.
                    let stored = self
                        .preferences()
                        .get(CONFIG_DAEMON_URL)
                        .map(|url| url.trim().to_string())
                        .unwrap_or_default();
                    let source = if stored.is_empty() {
                        self.endpoint.base_url.clone()
                    } else {
                        stored
                    };
                    Ok(derive_ws_url(&source).unwrap_or_default())
                } else {
                    ws_base(trimmed).ok_or_else(|| {
                        "daemon_ws_url must be a ws:// or wss:// address, or empty to derive it"
                            .to_string()
                    })
                }
            }
            CONFIG_DAEMON_TOKEN => Ok(trimmed.to_string()),
            CONFIG_CONNECTION_MODE => match trimmed.to_ascii_lowercase().as_str() {
                "" | CONNECTION_MODE_LOCAL => Ok(CONNECTION_MODE_LOCAL.to_string()),
                CONNECTION_MODE_SERVER => Ok(CONNECTION_MODE_SERVER.to_string()),
                other => Err(format!(
                    "connection_mode must be \"local\" or \"server\", not \"{other}\""
                )),
            },
            _ => Ok(value.to_string()),
        }
    }

    /// Persist `key = value` for the webview's own keys and the four server-connection keys.
    ///
    /// Three things a stored preference owes its owner, only one of which the first
    /// version of this shell did:
    ///
    /// - **the write is atomic** (see [`write_settings`]), because an app killed mid-write left
    ///   a file that does not parse, and [`read_settings`] answers that with an empty map — so
    ///   the owner's mode and theme would come back as "never set" after the crash that lost
    ///   them;
    /// - **it does not lose what it did not write**: the file is read again and merged, so a key
    ///   the owner set from another window of the same install survives;
    /// - **a failure is said**, in the error string the webview shows and in `desktop.log`.
    ///
    /// Storing a server address without a ws address fills the ws address in from it, so the
    /// pair the frontend reads back is never half-derived. The token is logged as "stored" or
    /// "cleared" and never by value: `desktop.log` is a plain-text file the installer
    /// documentation points owners at.
    fn set(&self, key: &str, value: &str) -> Result<(), String> {
        if SHELL_OWNED_KEYS.contains(&key) {
            return Err(format!("{key} is reported by the shell and cannot be set"));
        }
        if key.trim().is_empty() {
            return Err("a setting needs a name".to_string());
        }
        let stored = self.stored_value(key, value)?;
        let snapshot = {
            let mut settings = self.preferences();
            settings.insert(key.to_string(), stored.clone());
            if key == CONFIG_DAEMON_URL && !stored.is_empty() {
                let missing_ws = settings
                    .get(CONFIG_DAEMON_WS_URL)
                    .map(|ws| ws.trim().is_empty())
                    .unwrap_or(true);
                if missing_ws {
                    if let Some(derived) = derive_ws_url(&stored) {
                        settings.insert(CONFIG_DAEMON_WS_URL.to_string(), derived);
                    }
                }
            }
            settings.clone()
        };
        let mut merged = read_settings(&self.settings_path);
        // This process's map wins on the keys it holds: it is the state the owner is looking
        // at, and it is the write that was just asked for.
        for (name, value) in snapshot {
            merged.insert(name, value);
        }
        write_settings(&self.settings_path, &merged).map_err(|error| {
            self.note(&format!("could not store {key}: {error}"));
            error
        })?;
        self.note(&format!("{key} {}", logged_value(key, &stored)));
        Ok(())
    }

    /// Append one line to the shell's log, best effort: a log that cannot be written must not be
    /// the reason the app fails to start.
    fn note(&self, message: &str) {
        append_note(&self.log_path, message);
    }
}

/// Append one stamped line to a log file, creating it and its directory when they are not there.
///
/// Shared by [`ShellState::note`] and [`daemon_for`], because the one message an owner needs
/// about a daemon that refused to start is written before there is a shell state to write it
/// with.
fn append_note(path: &Path, message: &str) {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "[{stamp}] {message}");
    }
}

/// Read the shell's settings file; a missing or unreadable file is simply an empty map.
///
/// An empty map rather than an error is the frontend's contract: `checkOnboarding` reads
/// `onboarding_complete` before the owner has ever set it, and a first launch must not fail
/// because nothing was stored yet. What that means for durability is the other half — an
/// unreadable file reads as "nothing was ever set" — which is exactly why [`write_settings`]
/// is atomic.
fn read_settings(path: &Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// The counter that makes each settings write's scratch file unique.
static SETTINGS_WRITE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Write the shell's settings file, creating its directory if it is not there yet.
///
/// A temp file and a rename, not `fs::write` over the live file: a rename replaces the
/// target atomically, while a write that is interrupted by a crash, a power loss or a
/// killed process leaves the file half-written — and a half-written JSON file reads back
/// as an empty map, which is an owner who lost their mode, their theme and their
/// onboarding answer without a word about it.
fn write_settings(path: &Path, settings: &BTreeMap<String, String>) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{path:?} has no directory"))?;
    std::fs::create_dir_all(parent).map_err(|error| format!("{parent:?}: {error}"))?;
    let json = serde_json::to_string_pretty(settings).map_err(|error| error.to_string())?;

    let sequence = SETTINGS_WRITE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch = parent.join(format!(
        ".{}.{}.{}.tmp",
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "settings".to_string()),
        std::process::id(),
        sequence
    ));
    std::fs::write(&scratch, json.as_bytes()).map_err(|error| format!("{scratch:?}: {error}"))?;
    std::fs::rename(&scratch, path).map_err(|error| {
        // The live file is untouched either way; the scratch file is nothing the next
        // write should inherit.
        let _ = std::fs::remove_file(&scratch);
        format!("{path:?}: {error}")
    })
}

/// The base address form of an `http`/`https` URL, or `None` when it is not one.
///
/// A server address the frontend will `fetch` has to be an absolute `http(s)` URL with a host;
/// anything else — a bare `localhost:8787`, a `file:` path, a `ws:` address typed into the wrong
/// field — is refused where the owner typed it rather than stored and failed at connect time.
/// The trailing slash is dropped so `{base}/rpc` never grows a double slash.
fn http_base(value: &str) -> Option<String> {
    let url = tauri::Url::parse(value.trim()).ok()?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return None;
    }
    Some(value.trim().trim_end_matches('/').to_string())
}

/// The base address form of a `ws`/`wss` URL, or `None` when it is not one.
fn ws_base(value: &str) -> Option<String> {
    let url = tauri::Url::parse(value.trim()).ok()?;
    if !matches!(url.scheme(), "ws" | "wss") || url.host_str().is_none_or(str::is_empty) {
        return None;
    }
    Some(value.trim().trim_end_matches('/').to_string())
}

/// The websocket address that belongs to an HTTP base address: `http` → `ws`, `https` → `wss`,
/// same host, port and path.
///
/// This is the derivation `daemon_ws_url` promises when the owner clears it, so an installer who
/// knows only the server's HTTP address never has to work out the ws one: a daemon behind TLS is
/// reachable at `wss://` and a plain one at `ws://`, and guessing wrong is a connect that hangs
/// rather than an error that says why.
fn derive_ws_url(http_url: &str) -> Option<String> {
    let base = http_base(http_url)?;
    let mut url = tauri::Url::parse(&base).ok()?;
    let scheme = match url.scheme() {
        "https" => "wss",
        _ => "ws",
    };
    url.set_scheme(scheme).ok()?;
    Some(url.as_str().trim_end_matches('/').to_string())
}

/// What a stored setting says in `desktop.log`.
///
/// The token is the one value that never reaches the log by value: a note saying "daemon_token is
/// now \"ghp_…\"" would put a credential in a plain-text file the installer documentation tells
/// owners to send in with a bug report. The rest are addresses and a mode, which are what makes a
/// connection report readable.
fn logged_value(key: &str, value: &str) -> String {
    if key == CONFIG_DAEMON_TOKEN {
        return if value.is_empty() {
            "cleared".to_string()
        } else {
            "stored".to_string()
        };
    }
    if value.is_empty() {
        "cleared".to_string()
    } else {
        format!("set to {value:?}")
    }
}

/// The origins the daemon has to accept the webview's requests from.
///
/// The webview is not served from `http://127.0.0.1:<port>`, so every `fetch` it makes to the
/// daemon is cross-origin and the daemon has to name it. Windows serves the app from
/// `http://tauri.localhost` (Tauri 2's webview origin), macOS and Linux from `tauri://localhost`,
/// and `http://localhost:5173` is `pnpm tauri dev`'s Vite server.
fn webview_origins() -> Vec<String> {
    #[cfg_attr(not(debug_assertions), allow(unused_mut))]
    let mut origins = vec![
        "http://tauri.localhost".to_string(),
        "https://tauri.localhost".to_string(),
        "tauri://localhost".to_string(),
    ];
    #[cfg(debug_assertions)]
    {
        origins.push("http://localhost:5173".to_string());
    }
    origins
}

/// A copy of `config` with the webview's origins added to whatever it already allowed.
fn with_webview_origins(mut config: DaemonConfig) -> DaemonConfig {
    for origin in webview_origins() {
        if !config.allowed_origins.contains(&origin) {
            config.allowed_origins.push(origin);
        }
    }
    config
}

/// The endpoint of a daemon that is already running on the ports `config` names.
fn configured_endpoint(config: &DaemonConfig) -> DaemonEndpoint {
    DaemonEndpoint {
        base_url: format!("http://127.0.0.1:{}", config.ws_port),
        ws_url: format!("ws://127.0.0.1:{}", config.http_port),
        token: config.auth_token.clone(),
        owned: false,
    }
}

/// Start the daemon this shell owns, or describe the one that is already there.
///
/// Three outcomes, in order of preference:
///
/// 1. the daemon starts and the shell owns it (the normal first launch);
/// 2. it refuses because another instance holds the data directory's lock, in which case that
///    instance's daemon *is* reachable — both read the same `config.toml`, so the token and the
///    ports agree, and the shell simply does not own what it did not start;
/// 3. it refuses for any other reason (a port taken by an unrelated program is the realistic
///    one), and the shell retries once with OS-chosen ports, which cannot collide.
///
/// The data directory comes back with the outcome either way, because the shell's own settings
/// and log file live in it. Nothing is started when the configuration cannot be read at all: a
/// daemon without that file would have to invent a token, and an invented token is one the
/// frontend cannot be told.
fn start_shell() -> (
    PathBuf,
    Result<(DaemonEndpoint, Option<DaemonHandle>), String>,
) {
    let config = match tauri::async_runtime::block_on(warshad::load_or_create_config(None)) {
        Ok(config) => config,
        Err(error) => {
            let fallback = std::env::temp_dir().join("warsha");
            return (
                fallback,
                Err(format!("could not read the daemon configuration: {error}")),
            );
        }
    };
    let data_dir = config.data_dir.clone();

    (data_dir, daemon_for(config))
}

/// Start the daemon `config` describes, or describe the one already running on its ports.
fn daemon_for(config: DaemonConfig) -> Result<(DaemonEndpoint, Option<DaemonHandle>), String> {
    let config = with_webview_origins(config);
    let log_path = config.data_dir.join(LOG_FILE);
    match tauri::async_runtime::block_on(warshad::start_daemon(config.clone())) {
        Ok(handle) => Ok((
            DaemonEndpoint {
                base_url: format!("http://127.0.0.1:{}", handle.rpc_port()),
                ws_url: format!("ws://127.0.0.1:{}", handle.http_port()),
                token: config.auth_token.clone(),
                owned: true,
            },
            Some(handle),
        )),
        Err(DaemonError::Lock(reason)) => {
            // The daemon's own words name the process holding the data directory. They go in the
            // log the installer documentation points at, because this is the one start outcome an
            // owner has no way to see from the window: the app looks normal, and a stray
            // `warshad` from an earlier crash is what is answering it.
            append_note(
                &log_path,
                &format!("using a daemon another instance already started: {reason}"),
            );
            Ok((configured_endpoint(&config), None))
        }
        Err(first_failure) => {
            let mut free_ports = config.clone();
            free_ports.http_port = 0;
            free_ports.ws_port = 0;
            match tauri::async_runtime::block_on(warshad::start_daemon(free_ports)) {
                Ok(handle) => Ok((
                    DaemonEndpoint {
                        base_url: format!("http://127.0.0.1:{}", handle.rpc_port()),
                        ws_url: format!("ws://127.0.0.1:{}", handle.http_port()),
                        token: config.auth_token.clone(),
                        owned: true,
                    },
                    Some(handle),
                )),
                Err(second_failure) => Err(format!(
                    "warshad did not start: {first_failure}; retrying on OS-chosen ports: {second_failure}"
                )),
            }
        }
    }
}

/// Send one hand-written HTTP/1.1 request to `addr` and return the whole response as text.
///
/// Hand-written because the shell needs exactly three requests — an authorized JSON-RPC POST, an
/// unauthorized one, and a CORS preflight — and a full HTTP client would be a dependency this
/// crate does not otherwise use. `Connection: close` makes the daemon close the socket after its
/// answer, so reading to end-of-stream is the whole response.
fn raw_request(addr: &str, request: &str) -> Result<String, String> {
    use std::io::{Read, Write};

    let mut stream =
        std::net::TcpStream::connect(addr).map_err(|error| format!("connect {addr}: {error}"))?;
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("write to {addr}: {error}"))?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| format!("read from {addr}: {error}"))?;
    Ok(response)
}

/// The JSON-RPC POST `warsha-client` sends: `POST <base>/rpc`, bearer token, JSON body.
fn rpc_request(host: &str, token: &str, method: &str) -> String {
    let body = format!(r#"{{"jsonrpc":"2.0","id":1,"method":"{method}"}}"#);
    format!(
        "POST /rpc HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\n\r\n{body}",
        body.len()
    )
}

/// The preflight a browser sends before that POST, from the webview's own origin.
fn preflight_request(host: &str, origin: &str) -> String {
    format!(
        "OPTIONS /rpc HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nAccess-Control-Request-Method: POST\r\nAccess-Control-Request-Headers: authorization,content-type\r\nConnection: close\r\n\r\n"
    )
}

/// A JSON-RPC POST with a caller-supplied `params` value.
///
/// `warsha-client` sends its params as a named map, and the daemon binds both the snake_case
/// and the camelCase spelling (`crates/warshad/tests/gates_list.rs` proves it), so the smoke
/// calls below use exactly the shapes that client uses rather than inventing a third.
fn rpc_post_request(host: &str, token: &str, method: &str, params: serde_json::Value) -> String {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    })
    .to_string();
    format!(
        "POST /rpc HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nAuthorization: Bearer {token}\r\n\r\n{body}",
        body.len()
    )
}

/// The `result` of a JSON-RPC answer, or `None` when the response is not a 200 with a result.
///
/// The smoke calls need three things from one response — the status line says 200, there is no
/// `error` object, and the `result` is there to read ids and artifact names out of — and they
/// need them from the *whole* response, headers included, so the body is separated here once.
fn rpc_json_body(response: &str) -> Option<serde_json::Value> {
    let (headers, body) = response.split_once("\r\n\r\n")?;
    if !headers.contains("200") {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(body.trim()).ok()?;
    if value.get("error").is_some() {
        return None;
    }
    value.get("result").cloned()
}

/// The daemon's `result` from a [`raw_request`] outcome: `None` unless the socket round trip
/// succeeded *and* the answer is a 200 with a JSON-RPC result.
fn rpc_result(outcome: &Result<String, String>) -> Option<serde_json::Value> {
    outcome.as_deref().ok().and_then(rpc_json_body)
}

/// The one-line detail for a failed smoke call: the status line when the daemon answered,
/// the connect/write error when it did not.
fn response_detail(outcome: &Result<String, String>) -> String {
    match outcome {
        Ok(response) => first_line(response),
        Err(error) => error.clone(),
    }
}

/// The first `key` found as a string anywhere in a JSON-RPC `result`, depth first.
///
/// The smoke run needs two ids out of two different replies — the project's from
/// `projects.create` and the run's from `runs.start` — and neither call site should have to
/// restate the daemon's exact field layout for the second time; the field names those replies
/// use (`id`) are the same in both, and a wrong shape reads as `None`, which the caller reports
/// as a failed check rather than a panic.
fn first_string(value: &serde_json::Value, key: &str) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => map
            .get(key)
            .and_then(|found| found.as_str())
            .map(str::to_string)
            .or_else(|| map.values().find_map(|child| first_string(child, key))),
        serde_json::Value::Array(items) => items.iter().find_map(|item| first_string(item, key)),
        _ => None,
    }
}

/// `GET /health` on the daemon's HTTP port, with no credential.
///
/// `/health` is the one route the daemon answers without a bearer token (the same exemption
/// documented in `crates/warshad/src/lib.rs`), so it is also the one the installer check can
/// aim at the `ws_url` the frontend gets: `ws://` becomes `http://` and the port is the
/// daemon's events port, which is exactly where the window's live feed would connect.
fn health_request(host: &str) -> String {
    format!("GET /health HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")
}

/// The shape of the `PASS`/`FAIL` recorder [`self_check`] defines for its own run: it takes
/// the run's failure list, the condition being proved, the label to print, and the one-line
/// detail for the failure. Named here because the installer checks receive it as a trait
/// object and clippy is right that the inline type is too dense to repeat.
type CheckFn = dyn for<'a> Fn(&'a mut Vec<String>, bool, &'a str, &'a str);

/// Prove `GET /health` answers 200 with a status field, and record the outcome.
fn check_health(failures: &mut Vec<String>, ws_url: &str) {
    let health_host = ws_url.trim_start_matches("ws://");
    let outcome = raw_request(health_host, &health_request(health_host));
    match &outcome {
        Ok(response) => {
            let answered = response.contains("200")
                && response.contains(r#""status""#)
                && (response.contains("healthy") || response.contains("ok"));
            if answered {
                println!(
                    "PASS  GET /health on the daemon's HTTP port answers 200 with a healthy status"
                );
            } else {
                println!(
                    "FAIL  GET /health on the daemon's HTTP port answers 200 with a healthy status — {}",
                    first_line(response)
                );
                failures.push("GET /health answers 200 with a healthy status".to_string());
            }
        }
        Err(error) => {
            println!("FAIL  GET /health on the daemon's HTTP port answers 200 with a healthy status — {error}");
            failures.push("GET /health answers 200 with a healthy status".to_string());
        }
    }
}

/// The installer smoke test: one complete pipeline run driven over JSON-RPC with **no API
/// key at all**, against a daemon whose data lives in a scratch directory.
///
/// This is the zero-configuration path a fresh install takes its first steps on, proved
/// end-to-end rather than by a unit test in one crate: `projects.create` → `runs.start` →
/// `runs.advance` (which walks Idea → GateA and stops at the pending Gate A — the
/// gate-integrity rules require a gate to be the run's *current* stage before it can be
/// decided) → `gates.approve` (Gate A) → `runs.advance` again (which passes the approved
/// Gate A, walks Research → PrdSpec and stops at the pending Gate B) → `runs.artifacts`.
/// The stage calls are answered by the pipeline's deterministic `FakeProvider`
/// (`crates/warsha-pipeline` dispatch case 3), which makes no network call;
/// `WARSHA_LLM_PROVIDER=fake` is set here so a `WARSHA_LLM_API_KEY` inherited from this
/// machine's environment cannot quietly turn the smoke run into a real, paid model call —
/// the same guarantee the repository's rules make for every test.
///
/// The checks this appends are only run when the daemon answered with a result; a refusal at
/// any step stops the walk early, because a `runs.advance` over a run that was never created
/// would prove nothing except that the store says "not found".
fn check_fake_provider_pipeline(
    failures: &mut Vec<String>,
    host: &str,
    token: &str,
    check: &CheckFn,
) {
    std::env::set_var("WARSHA_LLM_PROVIDER", "fake");

    let create = raw_request(
        host,
        &rpc_post_request(
            host,
            token,
            "projects.create",
            serde_json::json!({
                "name": "smoke-project",
                "description": "Installer smoke test",
                "language": "ar"
            }),
        ),
    );
    let project_id = rpc_result(&create).and_then(|result| first_string(&result, "id"));
    check(
        failures,
        project_id.is_some(),
        "projects.create creates the smoke project",
        &response_detail(&create),
    );
    let Some(project_id) = project_id else {
        failures.push("the zero-key FakeProvider run needs a project".to_string());
        return;
    };

    let start = raw_request(
        host,
        &rpc_post_request(
            host,
            token,
            "runs.start",
            serde_json::json!({ "projectId": project_id, "idea": "Build an Arabic task tracker" }),
        ),
    );
    let run_id = rpc_result(&start).and_then(|result| first_string(&result, "id"));
    check(
        failures,
        run_id.is_some(),
        "runs.start starts the smoke run at the Idea stage",
        &response_detail(&start),
    );
    let Some(run_id) = run_id else {
        failures.push("the zero-key FakeProvider run needs a run".to_string());
        return;
    };

    let to_gate_a = raw_request(
        host,
        &rpc_post_request(
            host,
            token,
            "runs.advance",
            serde_json::json!({ "run_id": run_id }),
        ),
    );
    let at_gate_a = rpc_result(&to_gate_a);
    check(
        failures,
        at_gate_a
            .as_ref()
            .and_then(|run| run.get("current_stage"))
            .and_then(|stage| stage.as_str())
            == Some("gate_a"),
        "runs.advance walks Idea → GateA and stops at the pending gate",
        &response_detail(&to_gate_a),
    );
    if at_gate_a.is_none() {
        failures.push("the zero-key FakeProvider run needs to reach Gate A".to_string());
        return;
    }

    let approve = raw_request(
        host,
        &rpc_post_request(
            host,
            token,
            "gates.approve",
            serde_json::json!({ "run_id": run_id, "stage": "gate_a", "device_id": null }),
        ),
    );
    let approved = rpc_result(&approve).is_some();
    check(
        failures,
        approved,
        "gates.approve records the Gate A decision over RPC",
        &response_detail(&approve),
    );
    if !approved {
        failures.push("the zero-key FakeProvider run needs Gate A approved".to_string());
        return;
    }

    let advance = raw_request(
        host,
        &rpc_post_request(
            host,
            token,
            "runs.advance",
            serde_json::json!({ "run_id": run_id }),
        ),
    );
    let advanced = rpc_result(&advance);
    check(
        failures,
        advanced.is_some(),
        "runs.advance passes the approved Gate A, walks Research → PrdSpec, stops at Gate B",
        &response_detail(&advance),
    );
    if let Some(run) = &advanced {
        // `Stage` serializes snake_case on the wire (`serde(rename_all = "snake_case")` in
        // `warsha-core`), and the walk is defined to stop at the first gate with no decision,
        // which after approving Gate A is Gate B.
        check(
            failures,
            run.get("current_stage").and_then(|stage| stage.as_str()) == Some("gate_b"),
            "the smoke run is waiting at Gate B",
            &response_detail(&advance),
        );
    } else {
        failures.push("the zero-key FakeProvider run must reach Gate B".to_string());
        return;
    }

    let artifacts = raw_request(
        host,
        &rpc_post_request(
            host,
            token,
            "runs.artifacts",
            serde_json::json!({ "run_id": run_id }),
        ),
    );
    let names: Vec<String> = rpc_result(&artifacts)
        .and_then(|result| result.as_array().cloned())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("name"))
                .filter_map(|name| name.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let produced = ["idea", "research", "prd_spec"]
        .iter()
        .all(|wanted| names.iter().any(|name| name == *wanted));
    if produced {
        println!("PASS  zero-key FakeProvider run produced research and prd_spec artifacts");
    } else {
        println!(
            "FAIL  zero-key FakeProvider run produced research and prd_spec artifacts — the run carries {:?}",
            names
        );
        failures
            .push("zero-key FakeProvider run produced research and prd_spec artifacts".to_string());
    }
}

/// `--self-check [--data-dir <path>]` — start the daemon exactly as the window does, prove the
/// webview could reach it, and stop it again. Returns an error naming every check that failed.
///
/// This exists because the half of this app that needs no window is the half that breaks in
/// silence: a host that cannot create a webview (a headless CI runner, a locked-down session) still
/// has to be able to answer "does `warshad` start, and can the frontend reach it?".
///
/// `--data-dir` points the check at a scratch directory instead of the owner's real one, which is
/// what a CI runner wants: the check must not touch the projects of whoever runs it. It is also
/// the only case that runs [`restart_safety_report`] and the installer checks
/// ([`check_health`] and [`check_fake_provider_pipeline`]), because those start and stop daemons
/// over the directory twice and write a smoke project and a full pipeline run into it, and no
/// check has any business doing that to a live install.
pub fn self_check(scratch: Option<&Path>) -> anyhow::Result<()> {
    let (data_dir, outcome, restart_config) = match scratch {
        None => {
            let (data_dir, outcome) = start_shell();
            (data_dir, outcome, None)
        }
        Some(dir) => {
            let mut config = tauri::async_runtime::block_on(warshad::load_or_create_config(Some(
                &dir.join("config.toml"),
            )))
            .map_err(|error| anyhow::anyhow!("could not write a config in {dir:?}: {error}"))?;
            // The daemon keeps its database, its lock and its socket in `data_dir`, so a scratch
            // run has to move all three: a config file written elsewhere is not enough.
            config.data_dir = dir.to_path_buf();
            config.socket_path = dir.join("warshad.sock");
            // And OS-chosen ports, because the check starts and stops daemons more than once and
            // a fixed port would make the second start fail for a reason that has nothing to do
            // with what is being proved.
            config.http_port = 0;
            config.ws_port = 0;
            let (data_dir, outcome) = (config.data_dir.clone(), daemon_for(config.clone()));
            (data_dir, outcome, Some(config))
        }
    };
    println!("data dir: {}", data_dir.display());

    let (endpoint, process) = match outcome {
        Ok(started) => started,
        Err(error) => anyhow::bail!("{error}"),
    };
    let host = endpoint.base_url.trim_start_matches("http://").to_string();
    println!(
        "daemon:     {} (started by this process: {})",
        endpoint.base_url, endpoint.owned
    );
    println!("events ws:  {}", endpoint.ws_url);
    println!("token:      {} bytes", endpoint.token.len());

    let mut failures: Vec<String> = Vec::new();
    let check = |failures: &mut Vec<String>, condition: bool, label: &str, detail: &str| {
        if condition {
            println!("PASS  {label}");
        } else {
            println!("FAIL  {label} — {detail}");
            failures.push(label.to_string());
        }
    };

    let authorized = raw_request(&host, &rpc_request(&host, &endpoint.token, "projects.list"));
    match &authorized {
        Ok(response) => check(
            &mut failures,
            response.contains("\"jsonrpc\"") && response.contains("\"result\""),
            "an authorized JSON-RPC call answers",
            &first_line(response),
        ),
        Err(error) => check(
            &mut failures,
            false,
            "an authorized JSON-RPC call answers",
            error,
        ),
    }

    let refused = raw_request(
        &host,
        &rpc_request(&host, "not-the-daemons-token", "projects.list"),
    );
    match &refused {
        Ok(response) => check(
            &mut failures,
            response.contains("unauthorized") || response.contains("invalid token"),
            "a wrong token is refused",
            &first_line(response),
        ),
        Err(error) => check(&mut failures, false, "a wrong token is refused", error),
    }

    let preflight = raw_request(&host, &preflight_request(&host, "http://tauri.localhost"));
    match &preflight {
        Ok(response) => check(
            &mut failures,
            response.contains("200") && response.contains("access-control-allow-origin"),
            "the webview's CORS preflight is answered",
            &first_line(response),
        ),
        Err(error) => check(
            &mut failures,
            false,
            "the webview's CORS preflight is answered",
            error,
        ),
    }

    if scratch.is_some() {
        // The two installer checks live here, in the scratch-only half, for the same reason
        // the restart check does: one of them writes nothing to the daemon it shares with the
        // owner, and the other creates a smoke project and a whole run. Neither has any
        // business touching the owner's real data directory.
        check_health(&mut failures, &endpoint.ws_url);
        check_fake_provider_pipeline(&mut failures, &host, &endpoint.token, &check);
    }

    if let Some(handle) = process {
        tauri::async_runtime::block_on(handle.shutdown());
        println!("daemon stopped (children reaped, lock released)");
    } else {
        println!("daemon was already running: left alone");
    }

    // The lock is what makes a second launch and a restart safe, so it is proved here rather
    // than assumed. Only the scratch directory is used, because this starts and stops daemons
    // over the same data directory twice.
    if let Some(config) = restart_config {
        let lock_failures = restart_safety_report(&config);
        if lock_failures.is_empty() {
            println!("PASS  one daemon holds the data directory, and a restart gets it back");
        } else {
            for label in &lock_failures {
                println!("FAIL  {label}");
            }
            failures.extend(lock_failures);
        }
    }

    if failures.is_empty() {
        println!("self-check: OK");
        Ok(())
    } else {
        anyhow::bail!("self-check failed: {}", failures.join(", "))
    }
}

/// Start, refuse, stop and restart a daemon over `config`'s data directory, and say what did not
/// hold.
///
/// Three facts the desktop's "second window" and "it would not start again" both rest on:
///
/// 1. **one daemon holds the directory** while it runs — a second start is refused, and the
///    refusal names the holder's process id rather than looking like a crash, which is what the
///    shell reads as "there is a daemon already, and it is reachable";
/// 2. **a shutdown gives it back** — the next start over the same data directory succeeds, so an
///    owner who closed the app opens it again without clearing anything by hand;
/// 3. **the file left behind is not the lock** — the lock is the operating system's advisory lock
///    on that file, released when its holder exits, clean or not. That is why a daemon that was
///    killed outright does not lock its own data directory out of use forever, and why the lock
///    file is never deleted (see `warshad`'s `LockFile`: deleting it is how two instances end up
///    locking two different inodes with the same name).
///
/// The failures are returned rather than exited from, so the caller decides whether a failed
/// check ends the run.
fn restart_safety_report(config: &DaemonConfig) -> Vec<String> {
    let mut failures = Vec::new();
    let Ok(first) = tauri::async_runtime::block_on(warshad::start_daemon(config.clone())) else {
        failures.push(format!(
            "a daemon could not start over {}",
            config.data_dir.display()
        ));
        return failures;
    };

    match tauri::async_runtime::block_on(warshad::start_daemon(config.clone())) {
        Err(warshad::DaemonError::Lock(reason)) => {
            println!("second daemon refused, as it must: {reason}");
        }
        Err(other) => failures.push(format!(
            "a second daemon on the same data directory failed for the wrong reason: {other}"
        )),
        Ok(duplicate) => {
            // Two daemons over one data directory is how an owner's settings get written twice
            // at once, so it is ended here rather than left running for the next check.
            tauri::async_runtime::block_on(duplicate.shutdown());
            failures.push(
                "a second daemon started over a data directory another daemon was holding"
                    .to_string(),
            );
        }
    }

    tauri::async_runtime::block_on(first.shutdown());

    match tauri::async_runtime::block_on(warshad::start_daemon(config.clone())) {
        Ok(restarted) => tauri::async_runtime::block_on(restarted.shutdown()),
        Err(error) => failures.push(format!(
            "the data directory was not reusable after a clean shutdown: {error}"
        )),
    }
    failures
}

/// The status line of an HTTP response, for the one-line report.
fn first_line(response: &str) -> String {
    response
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// What one candidate daemon address answered when asked for `health`.
///
/// camelCase on the way out, because this crosses to the frontend as JSON and the settings screen
/// reads `wsUrl` to fill the websocket field and `reason` to pick the sentence it shows. `reason`
/// is one of `invalid_url`, `unreachable`, `unauthorized` or `bad_response` — a closed set, so the
/// UI can branch on it without matching on English prose — and `detail` is the prose, short and
/// already written for an owner rather than for a log.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonConnectionTest {
    pub ok: bool,
    pub reason: Option<String>,
    pub detail: String,
    pub ws_url: String,
    pub version: Option<String>,
}

impl DaemonConnectionTest {
    fn reachable(ws_url: String, version: Option<String>) -> Self {
        let detail = match &version {
            Some(version) => format!("warshad {version} answered the health call"),
            None => "the daemon answered the health call".to_string(),
        };
        Self {
            ok: true,
            reason: None,
            detail,
            ws_url,
            version,
        }
    }

    fn failed(reason: &str, ws_url: String, detail: String) -> Self {
        Self {
            ok: false,
            reason: Some(reason.to_string()),
            detail,
            ws_url,
            version: None,
        }
    }
}

/// Ask `url` for `health` over JSON-RPC and say what came back, in the four reasons the frontend
/// knows.
///
/// This is the one place the shell makes a request to an address the owner typed, so it is also
/// the one place that has to tell four very different failures apart:
///
/// - **`invalid_url`** — the address is not an absolute `http(s)` URL, so nothing was sent;
/// - **`unreachable`** — nothing is listening there, or nothing answered inside
///   [`DAEMON_TEST_TIMEOUT`]. On a slow link this is the honest answer to "is it up", not a
///   failure of the daemon;
/// - **`unauthorized`** — something answered and refused the token (HTTP 401/403, or a JSON-RPC
///   auth error), which is the case an owner can fix without reading a log;
/// - **`bad_response`** — something answered with a status, a body or a JSON shape that is not a
///   daemon's `health`.
///
/// The token travels as a bearer header and never appears in `detail`, which the frontend shows
/// and may paste into a bug report.
async fn probe_daemon(url: &str, token: &str) -> DaemonConnectionTest {
    let ws_url = derive_ws_url(url).unwrap_or_default();
    let Some(base) = http_base(url) else {
        return DaemonConnectionTest::failed(
            "invalid_url",
            ws_url,
            "the address must be a full http:// or https:// URL".to_string(),
        );
    };

    let client = match reqwest::Client::builder()
        .timeout(DAEMON_TEST_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return DaemonConnectionTest::failed(
                "unreachable",
                ws_url,
                format!("the test could not start: {error}"),
            )
        }
    };

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "health",
        "params": [],
    });
    let response = client
        .post(format!("{base}/rpc"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .bearer_auth(token)
        .json(&body)
        .send()
        .await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return DaemonConnectionTest::failed("unreachable", ws_url, transport_detail(&error))
        }
    };

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return DaemonConnectionTest::failed(
            "unauthorized",
            ws_url,
            format!(
                "the daemon answered HTTP {} and refused the token",
                status.as_u16()
            ),
        );
    }
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return DaemonConnectionTest::failed(
            "bad_response",
            ws_url,
            format!("the address answered HTTP {}", status.as_u16()),
        );
    }

    let Ok(body) = serde_json::from_str::<serde_json::Value>(&text) else {
        return DaemonConnectionTest::failed(
            "bad_response",
            ws_url,
            "the address answered with something that is not JSON".to_string(),
        );
    };
    if let Some(error) = body.get("error") {
        let detail = error
            .get("message")
            .and_then(|message| message.as_str())
            .unwrap_or("the daemon refused the call")
            .to_string();
        let reason = if is_auth_refusal(error) {
            "unauthorized"
        } else {
            "bad_response"
        };
        return DaemonConnectionTest::failed(reason, ws_url, detail);
    }
    let Some(result) = body.get("result") else {
        return DaemonConnectionTest::failed(
            "bad_response",
            ws_url,
            "the answer carried no result".to_string(),
        );
    };
    if let Some(reported) = result.get("status").and_then(|status| status.as_str()) {
        let normalized = reported.to_ascii_lowercase();
        if !matches!(normalized.as_str(), "healthy" | "ok" | "ready" | "degraded") {
            return DaemonConnectionTest::failed(
                "bad_response",
                ws_url,
                format!("the daemon reported status {reported:?}"),
            );
        }
    }
    let version = result
        .get("version")
        .and_then(|version| version.as_str())
        .map(str::to_string);
    DaemonConnectionTest::reachable(ws_url, version)
}

/// Whether a JSON-RPC error object is the daemon refusing the caller's token.
///
/// The daemon's own auth refusal is an HTTP 401 with `{"error":"unauthorized"}`, which never
/// reaches here; this covers a daemon that reports the same thing as a JSON-RPC error instead —
/// codes in the server-error range it uses for auth, or a message that names authentication. It
/// errs towards "unauthorized" because telling an owner their token was refused is more useful
/// than telling them the answer was odd when both may be true.
fn is_auth_refusal(error: &serde_json::Value) -> bool {
    let code = error
        .get("code")
        .and_then(|code| code.as_i64())
        .unwrap_or_default();
    if matches!(code, -32001 | -32002) {
        return true;
    }
    let message = error
        .get("message")
        .and_then(|message| message.as_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    [
        "unauthorized",
        "unauthenticated",
        "authentication",
        "forbidden",
        "token",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

/// The owner-facing sentence for a request that never produced a response.
///
/// A timeout and a refused connection are different things to fix — one is "is the machine up",
/// the other is "is anything listening on that port" — so they are said differently even though
/// both are `unreachable` to the frontend.
fn transport_detail(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        format!(
            "the address did not answer within {}s",
            DAEMON_TEST_TIMEOUT.as_secs()
        )
    } else if error.is_connect() {
        "nothing is listening at that address".to_string()
    } else {
        "the address could not be reached".to_string()
    }
}

/// `get_config` — the value of one shell config key.
#[tauri::command]
fn get_config(key: String, state: State<'_, ShellState>) -> Result<String, String> {
    Ok(state.get(&key))
}

/// `set_config` — store one webview preference, or one of the three server-connection values.
#[tauri::command]
fn set_config(key: String, value: String, state: State<'_, ShellState>) -> Result<(), String> {
    state.set(&key, &value)
}

/// `test_daemon_connection` — ask a candidate daemon address for `health` with `token`.
///
/// Unsaved by design: the settings screen tests what the owner has typed, before it writes
/// anything, so a wrong address costs a click rather than a stored setting to undo.
#[tauri::command]
async fn test_daemon_connection(
    url: String,
    token: String,
) -> Result<DaemonConnectionTest, String> {
    Ok(probe_daemon(&url, &token).await)
}

/// `pick_folder` — open a native OS directory picker dialog and return the chosen path, if any.
#[tauri::command]
async fn pick_folder(
    app: tauri::AppHandle,
    title: Option<String>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut builder = app.dialog().file();
        if let Some(title) = title.filter(|t| !t.trim().is_empty()) {
            builder = builder.set_title(title);
        }
        builder
            .blocking_pick_folder()
            .map(|fp| fp.into_path().map(|p| p.to_string_lossy().into_owned()))
    })
    .await
    .map_err(|err| err.to_string())?;

    match result {
        Some(Ok(path)) => Ok(Some(path)),
        Some(Err(err)) => Err(err.to_string()),
        None => Ok(None),
    }
}

/// Build and run the app: start the daemon, publish where it is, and reap it on exit.
///
/// The daemon starts *before* the window is created, and synchronously. The webview's first
/// `get_config` therefore cannot race the daemon's ports, which is the failure mode a background
/// start would introduce: the frontend would read an empty `daemon_url` and fall back to its
/// development defaults.
pub fn run() -> anyhow::Result<()> {
    let (data_dir, outcome) = start_shell();
    let settings_path = data_dir.join(SETTINGS_FILE);
    let log_path = data_dir.join(LOG_FILE);

    let (endpoint, process, startup_error) = match outcome {
        Ok((endpoint, process)) => (endpoint, process, None),
        Err(error) => (
            DaemonEndpoint {
                base_url: String::new(),
                ws_url: String::new(),
                token: String::new(),
                owned: false,
            },
            None,
            Some(error),
        ),
    };

    let shell = ShellState {
        endpoint,
        process: Mutex::new(process),
        settings: Mutex::new(read_settings(&settings_path)),
        settings_path,
        log_path,
        data_dir,
    };
    if let Some(error) = startup_error {
        shell.note(&error);
    } else if shell.endpoint.owned {
        shell.note(&format!("daemon started on {}", shell.endpoint.base_url));
    } else {
        shell.note("using a daemon another instance already started");
    }

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(shell)
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_config,
            test_daemon_connection,
            pick_folder
        ])
        .build(tauri::generate_context!())?;

    app.run(|handle, event| {
        // The daemon is this process's own, so it has to be stopped here: a daemon that is
        // dropped rather than shut down leaves its preview servers and MCP stdio processes
        // behind, and the next launch finds a data directory whose ports are taken.
        // `DaemonHandle::shutdown` stops the RPC server, aborts the HTTP task, kills and waits
        // for every child it spawned, and closes the lock handle.
        //
        // The paths that never reach this callback — a `build()` that fails, a `?` between the
        // start and the event loop, a panic unwinding through the shell — are covered by
        // `DaemonHandle`'s own `Drop`, which runs the same child sweep on a thread that may
        // block for it. And a process that is killed outright takes this daemon down with it
        // because it was never a separate `warshad`: the shell's own exit is the daemon's exit,
        // which is the reason there is no orphan `warshad` to find in the first place.
        if let RunEvent::Exit = event {
            let state = handle.state::<ShellState>();
            let owned = state
                .process
                .lock()
                .ok()
                .and_then(|mut process| process.take());
            if let Some(handle) = owned {
                tauri::async_runtime::block_on(handle.shutdown());
                state.note("daemon stopped");
            }
        }
    });

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keys the frontend writes today, and the ones a restart has to give back.
    const APP_MODE: &str = "app_mode";
    const THEME: &str = "theme";

    /// A scratch directory next to the test binary, not in `%TEMP%`.
    ///
    /// The operating system's temp directory is the conventional choice and the one this
    /// machine's sandbox denies to a freshly built test executable, which would make an
    /// environment tell the owner the settings code is broken. The directory the binary
    /// itself lives in is always writable by it, and each test gets its own name under it,
    /// cleared before use so a run never reads a previous run's file.
    fn scratch(tag: &str) -> PathBuf {
        let root = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."))
            .join("warsha-desktop-tests");
        let directory = root.join(tag);
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("the scratch directory is made");
        directory
    }

    /// A shell state over one scratch directory, with no daemon in it.
    fn shell(directory: &Path) -> ShellState {
        ShellState {
            endpoint: DaemonEndpoint {
                base_url: "http://127.0.0.1:1234".to_string(),
                ws_url: "ws://127.0.0.1:4321".to_string(),
                token: "a-token".to_string(),
                owned: false,
            },
            process: Mutex::new(None),
            settings: Mutex::new(read_settings(&directory.join(SETTINGS_FILE))),
            settings_path: directory.join(SETTINGS_FILE),
            log_path: directory.join(LOG_FILE),
            data_dir: directory.to_path_buf(),
        }
    }

    #[test]
    fn a_preference_the_owner_sets_is_where_the_next_launch_looks_for_it() {
        let directory = scratch("settings-round-trip");
        let first = shell(&directory);
        first.set(APP_MODE, "advanced").expect("app_mode is stored");
        first.set(THEME, "dark").expect("theme is stored");

        // The next launch: a new state, read from the same file, with nothing remembered
        // in memory. This is the whole of "mode and theme survive a restart".
        let second = shell(&directory);
        assert_eq!(second.get(APP_MODE), "advanced");
        assert_eq!(second.get(THEME), "dark");

        // And the file on disk is the settings file in the daemon's data directory, so one
        // directory is the whole install for a backup or a removal.
        let on_disk: BTreeMap<String, String> = serde_json::from_str(
            &std::fs::read_to_string(directory.join(SETTINGS_FILE)).expect("the file is readable"),
        )
        .expect("the file is the settings map");
        assert_eq!(on_disk[APP_MODE], "advanced");
        assert_eq!(on_disk[THEME], "dark");
    }

    #[test]
    fn the_shell_owned_keys_are_answered_by_the_shell_and_cannot_be_written() {
        let directory = scratch("shell-owned-keys");
        let state = shell(&directory);

        for key in SHELL_OWNED_KEYS {
            assert_eq!(
                state.set(key, "http://elsewhere"),
                Err(format!("{key} is reported by the shell and cannot be set")),
                "{key} is the shell's to answer"
            );
        }

        // A stale `desktop.json` that names those keys anyway cannot redirect the frontend,
        // because they are read from the endpoint, not the file.
        let stale =
            r#"{"daemon_url":"http://127.0.0.1:9","daemon_token":"not-it","data_dir":"/fake"}"#;
        std::fs::write(directory.join(SETTINGS_FILE), stale).expect("the stale file is placed");
        let reloaded = shell(&directory);
        assert_eq!(reloaded.get(CONFIG_DAEMON_URL), "http://127.0.0.1:1234");
        assert_eq!(reloaded.get(CONFIG_DAEMON_TOKEN), "a-token");
        assert_eq!(
            reloaded.get(CONFIG_DATA_DIR),
            directory.display().to_string()
        );
    }

    #[test]
    fn a_write_keeps_the_keys_another_writer_stored() {
        let directory = scratch("settings-merge");
        let first = shell(&directory);
        first
            .set("routine_enabled", "true")
            .expect("the first key is stored");

        // A second state over the same file, which is what a second window of the same
        // install is: it read the file at start, and its write must not be the whole file
        // as it remembers it.
        let second = shell(&directory);
        second
            .set(THEME, "light")
            .expect("the second key is stored");

        assert_eq!(
            second.get("routine_enabled"),
            "true",
            "the other window's key is still there"
        );
        assert_eq!(second.get(THEME), "light");

        // And the first writer sees the second's change on its next read of the file.
        let reloaded = shell(&directory);
        assert_eq!(reloaded.get(THEME), "light");
        assert_eq!(reloaded.get("routine_enabled"), "true");
    }

    #[test]
    fn a_write_leaves_no_scratch_file_and_an_unnamed_key_is_refused() {
        let directory = scratch("settings-atomic");
        let state = shell(&directory);
        for index in 0..8 {
            state.set("counter", &index.to_string()).expect("a rewrite");
        }
        assert_eq!(state.get("counter"), "7");

        let mut leftovers: Vec<String> = std::fs::read_dir(&directory)
            .expect("the directory reads")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with('.'))
            .collect();
        leftovers.sort();
        assert!(
            leftovers.is_empty(),
            "a completed write renames its scratch file away: {leftovers:?}"
        );

        assert!(state.set("   ", "value").is_err(), "a key has to be named");
    }

    #[test]
    fn a_file_that_does_not_parse_reads_as_empty_which_is_why_writes_are_renamed() {
        let directory = scratch("settings-corrupt");
        std::fs::write(directory.join(SETTINGS_FILE), b"{\"app_mode\": \"adv")
            .expect("the truncated file is placed");
        // The documented consequence: an unreadable settings file is an empty map, so a
        // half-written file would silently be an owner who never chose a mode. The atomic
        // rename is what makes this state unreachable from a crash mid-write.
        assert!(read_settings(&directory.join(SETTINGS_FILE)).is_empty());
    }

    #[test]
    fn the_webview_origins_are_added_once_and_the_endpoint_matches_the_configuration() {
        let config = warshad::DaemonConfig {
            http_port: 8765,
            ws_port: 8766,
            auth_token: "the-token".to_string(),
            ..warshad::DaemonConfig::default()
        };
        let configured = config.clone();

        let once = with_webview_origins(config.clone());
        let twice = with_webview_origins(once.clone());
        assert_eq!(
            once.allowed_origins, twice.allowed_origins,
            "adding the webview's origins is idempotent: a second pass must not grow the list"
        );
        for origin in webview_origins() {
            assert!(
                once.allowed_origins.contains(&origin),
                "{origin} is the origin the webview asks from"
            );
        }

        // A daemon the shell does not own is described by the configuration both processes
        // read: the RPC port is `ws_port` and the events socket is `http_port`, which is the
        // one mapping that lets a second window reach the daemon the first one started.
        let endpoint = configured_endpoint(&configured);
        assert_eq!(endpoint.base_url, "http://127.0.0.1:8766");
        assert_eq!(endpoint.ws_url, "ws://127.0.0.1:8765");
        assert_eq!(endpoint.token, "the-token");
        assert!(
            !endpoint.owned,
            "the shell does not own what it did not start"
        );
    }

    #[test]
    fn a_note_is_appended_to_the_log_the_installer_points_at() {
        let directory = scratch("settings-log");
        let state = shell(&directory);
        state.note("first");
        state.note("second");
        let log = std::fs::read_to_string(directory.join(LOG_FILE)).expect("the log is readable");
        assert!(log.contains("first") && log.contains("second"), "{log}");
        // One stamped line per note, so a report can be read in order.
        assert_eq!(log.lines().count(), 2, "{log}");
    }

    /// The bundle file this test reads, resolved from the crate directory rather than the
    /// current working directory: `cargo test` runs the test binary from inside `target/`,
    /// and the manifest is a sibling of `src/`, not of anything under `target/`.
    fn tauri_config_text() -> String {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tauri.conf.json");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} must be readable: {error}", path.display()))
    }

    #[test]
    fn the_shipped_config_builds_unsigned_and_ships_no_active_updater() {
        // The Windows NSIS installer ships unsigned: no code-signing certificate and no
        // updater minisign key exist for this release. A `true` here makes `tauri build`
        // demand `TAURI_SIGNING_PRIVATE_KEY` and fail the packaging run, and an active
        // `plugins.updater` would ship a window pointed at an endpoint that publishes no
        // signed artifacts (it 404s). Both are refused by this test, which reads the very
        // file `tauri build` consumes, so the packaged install and the checked-in config
        // can never disagree.
        let config: serde_json::Value =
            serde_json::from_str(&tauri_config_text()).expect("tauri.conf.json is valid JSON");

        assert_eq!(
            config["bundle"]["createUpdaterArtifacts"].as_bool(),
            Some(false),
            "bundle.createUpdaterArtifacts must be false so an unsigned build needs no signing key"
        );

        match config
            .get("plugins")
            .and_then(|plugins| plugins.get("updater"))
        {
            None => {}
            Some(updater) => assert_ne!(
                updater.get("active").and_then(serde_json::Value::as_bool),
                Some(true),
                "plugins.updater must not be active: the updater endpoint ships no signed artifact"
            ),
        }
    }

    /// One scratch HTTP server on loopback that answers every request with `status` and `body`
    /// and keeps the request head it saw.
    ///
    /// A real socket on `127.0.0.1`, not a mocked client: what these tests are about is what the
    /// shell does with an answer that came off a wire — a 401, a body that is not JSON, a port
    /// nothing is listening on — and a mock would answer those by agreeing with the code instead
    /// of by being a server. Only the loopback interface is bound, so no test can reach a real
    /// daemon or the network.
    struct MockServer {
        url: String,
        seen: std::sync::Arc<Mutex<Vec<String>>>,
    }

    impl MockServer {
        fn answering(status: u16, body: &'static str) -> Self {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port on loopback");
            let address = listener.local_addr().expect("the bound address");
            let seen: std::sync::Arc<Mutex<Vec<String>>> =
                std::sync::Arc::new(Mutex::new(Vec::new()));
            let requests = std::sync::Arc::clone(&seen);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { break };
                    let Some(head) = read_request_head(&mut stream) else {
                        continue;
                    };
                    if let Ok(mut requests) = requests.lock() {
                        requests.push(head);
                    }
                    let response = format!(
                        "HTTP/1.1 {status} stub\r\ncontent-type: application/json\r\n\
                         content-length: {}\r\nconnection: close\r\n\r\n{body}",
                        body.len()
                    );
                    let _ = stream.write_all(response.as_bytes());
                }
            });
            Self {
                url: format!("http://{address}"),
                seen,
            }
        }

        /// Every request head the server saw, lowercased, joined — what a header assertion needs.
        fn heads(&self) -> String {
            self.seen
                .lock()
                .expect("the request log is not poisoned")
                .join("\n")
                .to_ascii_lowercase()
        }
    }

    /// Read one request's head (up to the blank line) off `stream`.
    fn read_request_head(stream: &mut std::net::TcpStream) -> Option<String> {
        use std::io::Read;

        let mut buffer = Vec::new();
        let mut chunk = [0u8; 512];
        loop {
            let read = stream.read(&mut chunk).ok()?;
            if read == 0 {
                return Some(String::from_utf8_lossy(&buffer).into_owned());
            }
            buffer.extend_from_slice(&chunk[..read]);
            if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
                return Some(String::from_utf8_lossy(&buffer).into_owned());
            }
            if buffer.len() > 64 * 1024 {
                return None;
            }
        }
    }

    /// The JSON-RPC answer a healthy daemon gives, as `warshad` writes it
    /// (`crates/warshad/src/lib.rs` `health`: `status: "healthy"` when its
    /// capabilities probe passes, `"degraded"` otherwise).
    const HEALTHY_ANSWER: &str = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"healthy","version":"0.3.1","uptime_seconds":7,"capabilities":{}}}"#;

    #[test]
    fn a_healthy_daemon_answers_the_connection_test_with_its_version() {
        let server = MockServer::answering(200, HEALTHY_ANSWER);
        let report = tauri::async_runtime::block_on(probe_daemon(&server.url, "good-token"));

        assert!(report.ok, "{report:?}");
        assert_eq!(
            report.reason, None,
            "a reachable daemon has no reason to give"
        );
        assert_eq!(report.version.as_deref(), Some("0.3.1"));
        assert!(report.detail.contains("0.3.1"), "{report:?}");
        assert_eq!(
            report.ws_url,
            server.url.replacen("http://", "ws://", 1),
            "the ws address is derived from the http one the owner typed"
        );

        // The token went on the wire as a bearer header, and nowhere else: not into the detail
        // the settings screen shows, and not into the log.
        let heads = server.heads();
        assert!(
            heads.contains("authorization: bearer good-token"),
            "the health call carries the token: {heads}"
        );
        assert!(
            heads.contains("post /rpc"),
            "the probe is the same POST the client makes: {heads}"
        );
        assert!(!report.detail.contains("good-token"), "{report:?}");
    }

    #[test]
    fn the_other_healthy_statuses_still_pass_the_connection_test() {
        // `isDaemonHealthy` (`apps/desktop/src/v3/data/useHealth.ts:14-18`) accepts
        // `healthy | ok | ready | degraded`, case-insensitive; the probe must agree.
        for status in ["ok", "ready", "degraded", "Healthy", "DEGRADED"] {
            let body = format!(
                r#"{{"jsonrpc":"2.0","id":1,"result":{{"status":{status:?},"version":"0.3.1"}}}}"#
            );
            // `MockServer::answering` needs a `'static` body, so leak one per case;
            // five small test-only leaks are bounded and never shipped.
            let body: &'static str = Box::leak(body.into_boxed_str());
            let server = MockServer::answering(200, body);
            let report = tauri::async_runtime::block_on(probe_daemon(&server.url, "t"));
            assert!(report.ok, "status {status:?}: {report:?}");
            assert_eq!(report.version.as_deref(), Some("0.3.1"), "{report:?}");
        }
    }

    #[test]
    fn a_refused_token_is_unauthorized_and_not_a_bad_answer() {
        let server = MockServer::answering(401, r#"{"error":"unauthorized"}"#);
        let report = tauri::async_runtime::block_on(probe_daemon(&server.url, "wrong-token"));
        assert!(!report.ok);
        assert_eq!(report.reason.as_deref(), Some("unauthorized"), "{report:?}");
        assert_eq!(report.version, None, "a refused call learned no version");
        assert!(
            !report.detail.contains("wrong-token"),
            "an error an owner may paste must not carry the token: {report:?}"
        );

        // 403 is the same fact to the frontend: the daemon answered and would not have us.
        let forbidden = MockServer::answering(403, "{}");
        let report = tauri::async_runtime::block_on(probe_daemon(&forbidden.url, "t"));
        assert_eq!(report.reason.as_deref(), Some("unauthorized"), "{report:?}");
    }

    #[test]
    fn a_closed_port_is_unreachable() {
        // A port that was bound and then released: nothing is listening on it now, which is the
        // "the server is not running" case the wizard has to name rather than hang on.
        let address = {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a port on loopback");
            listener.local_addr().expect("the bound address")
        };
        let report =
            tauri::async_runtime::block_on(probe_daemon(&format!("http://{address}"), "t"));
        assert!(!report.ok);
        assert_eq!(report.reason.as_deref(), Some("unreachable"), "{report:?}");
        assert!(report.version.is_none());
        assert_eq!(report.ws_url, format!("ws://{address}"));
    }

    #[test]
    fn an_answer_that_is_not_a_health_report_is_a_bad_response() {
        let garbage = MockServer::answering(200, "<html>not a daemon</html>");
        let report = tauri::async_runtime::block_on(probe_daemon(&garbage.url, "t"));
        assert_eq!(report.reason.as_deref(), Some("bad_response"), "{report:?}");

        // A JSON body that is a JSON-RPC error which is not about auth is also not a daemon
        // that said yes.
        let refusal = MockServer::answering(
            200,
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32601,"message":"unknown method"}}"#,
        );
        let report = tauri::async_runtime::block_on(probe_daemon(&refusal.url, "t"));
        assert_eq!(report.reason.as_deref(), Some("bad_response"), "{report:?}");
        assert!(!report.ok);

        // An auth error delivered as JSON-RPC rather than as a status is unauthorized.
        let auth = MockServer::answering(
            200,
            r#"{"jsonrpc":"2.0","id":1,"error":{"code":-32001,"message":"authentication required"}}"#,
        );
        let report = tauri::async_runtime::block_on(probe_daemon(&auth.url, "t"));
        assert_eq!(report.reason.as_deref(), Some("unauthorized"), "{report:?}");

        // A status outside the healthy set the app accepts is still not a daemon's yes.
        let offline = MockServer::answering(
            200,
            r#"{"jsonrpc":"2.0","id":1,"result":{"status":"offline","version":"0.3.1"}}"#,
        );
        let report = tauri::async_runtime::block_on(probe_daemon(&offline.url, "t"));
        assert!(!report.ok, "{report:?}");
        assert_eq!(report.reason.as_deref(), Some("bad_response"), "{report:?}");
    }

    #[test]
    fn an_address_that_is_not_http_is_refused_before_anything_is_sent() {
        for address in [
            "",
            "daemon.example.com",
            "localhost:8787",
            "ftp://daemon.example.com",
            "ws://daemon.example.com",
        ] {
            let report = tauri::async_runtime::block_on(probe_daemon(address, "t"));
            assert!(!report.ok, "{address} is not an http address");
            assert_eq!(
                report.reason.as_deref(),
                Some("invalid_url"),
                "{address}: {report:?}"
            );
        }
    }

    #[test]
    fn a_server_mode_answers_with_the_stored_values_and_local_answers_with_the_endpoint() {
        let directory = scratch("settings-server-mode");
        let state = shell(&directory);

        // Unset is local, and local is the endpoint this shell started — the behaviour every
        // earlier build had.
        assert_eq!(state.get(CONFIG_CONNECTION_MODE), "local");
        assert_eq!(state.get(CONFIG_DAEMON_URL), "http://127.0.0.1:1234");
        assert_eq!(state.get(CONFIG_DAEMON_TOKEN), "a-token");

        state
            .set(CONFIG_DAEMON_URL, "https://daemon.example.com")
            .expect("a server address is stored");
        state
            .set(CONFIG_DAEMON_TOKEN, "server-token")
            .expect("a server token is stored");
        state
            .set(CONFIG_CONNECTION_MODE, "server")
            .expect("the mode is stored");

        assert_eq!(state.get(CONFIG_DAEMON_URL), "https://daemon.example.com");
        assert_eq!(
            state.get(CONFIG_DAEMON_WS_URL),
            "wss://daemon.example.com",
            "a ws address the owner never typed is derived from the one they did"
        );
        assert_eq!(state.get(CONFIG_DAEMON_TOKEN), "server-token");

        // A restart keeps the choice: the mode and the values are in `desktop.json`, not in
        // this process's memory.
        let restarted = shell(&directory);
        assert_eq!(restarted.get(CONFIG_CONNECTION_MODE), "server");
        assert_eq!(
            restarted.get(CONFIG_DAEMON_URL),
            "https://daemon.example.com"
        );
        assert_eq!(restarted.get(CONFIG_DAEMON_TOKEN), "server-token");

        // Switching back is one write, and it is the local endpoint again — the stored server
        // values stay on disk, so switching forward again needs no retyping.
        restarted
            .set(CONFIG_CONNECTION_MODE, "local")
            .expect("the mode is stored");
        assert_eq!(restarted.get(CONFIG_CONNECTION_MODE), "local");
        assert_eq!(restarted.get(CONFIG_DAEMON_URL), "http://127.0.0.1:1234");
        assert_eq!(restarted.get(CONFIG_DAEMON_WS_URL), "ws://127.0.0.1:4321");
        assert_eq!(restarted.get(CONFIG_DAEMON_TOKEN), "a-token");
        let on_disk = read_settings(&directory.join(SETTINGS_FILE));
        assert_eq!(on_disk[CONFIG_DAEMON_URL], "https://daemon.example.com");
        assert_eq!(on_disk[CONFIG_DAEMON_TOKEN], "server-token");

        // `server` with no address is not a server to talk to, so the local endpoint stays the
        // answer instead of an address the frontend cannot dial.
        restarted
            .set(CONFIG_DAEMON_URL, "")
            .expect("the server address is cleared");
        restarted
            .set(CONFIG_CONNECTION_MODE, "server")
            .expect("the mode is stored");
        assert_eq!(restarted.get(CONFIG_DAEMON_URL), "http://127.0.0.1:1234");

        // The token was never written to the log, and the log says it was stored.
        let log = std::fs::read_to_string(directory.join(LOG_FILE)).expect("the log is readable");
        assert!(!log.contains("server-token"), "{log}");
        assert!(log.contains("daemon_token stored"), "{log}");
    }

    #[test]
    fn the_server_values_are_validated_where_the_owner_typed_them() {
        let directory = scratch("settings-validation");
        let state = shell(&directory);

        for address in [
            "daemon.example.com",
            "localhost:8787",
            "ftp://daemon.example.com",
            "ws://daemon.example.com",
            "http://",
        ] {
            assert!(
                state.set(CONFIG_DAEMON_URL, address).is_err(),
                "{address} is not an http address to store"
            );
        }
        for address in ["http://daemon.example.com", "daemon.example.com", "wss://"] {
            assert!(
                state.set(CONFIG_DAEMON_WS_URL, address).is_err(),
                "{address} is not a ws address to store"
            );
        }
        assert!(
            state.set(CONFIG_CONNECTION_MODE, "cloud").is_err(),
            "the mode is one of two words"
        );
        assert_eq!(
            state.set(CONFIG_DATA_DIR, "/elsewhere"),
            Err("data_dir is reported by the shell and cannot be set".to_string()),
            "the data directory stays the shell's to answer"
        );
        assert!(
            read_settings(&directory.join(SETTINGS_FILE)).is_empty(),
            "a refused write stores nothing"
        );

        // A ws address the owner does type is kept as typed, and an empty one is re-derived
        // from the server address rather than stored blank.
        assert!(state
            .set(CONFIG_DAEMON_URL, "https://daemon.example.com/")
            .is_ok());
        assert_eq!(
            read_settings(&directory.join(SETTINGS_FILE))[CONFIG_DAEMON_URL],
            "https://daemon.example.com",
            "a trailing slash is dropped so the rpc path never doubles it"
        );
        state
            .set(CONFIG_DAEMON_WS_URL, "wss://daemon.example.com:8443/socket")
            .expect("a ws address is stored");
        assert_eq!(
            read_settings(&directory.join(SETTINGS_FILE))[CONFIG_DAEMON_WS_URL],
            "wss://daemon.example.com:8443/socket"
        );
        state
            .set(CONFIG_DAEMON_WS_URL, "  ")
            .expect("an empty ws address is derived");
        assert_eq!(
            read_settings(&directory.join(SETTINGS_FILE))[CONFIG_DAEMON_WS_URL],
            "wss://daemon.example.com"
        );

        // The token is stored trimmed, and the mode accepts either word in any case.
        state
            .set(CONFIG_DAEMON_TOKEN, "  spaced-token  ")
            .expect("a token");
        assert_eq!(
            read_settings(&directory.join(SETTINGS_FILE))[CONFIG_DAEMON_TOKEN],
            "spaced-token"
        );
        state.set(CONFIG_CONNECTION_MODE, "SERVER").expect("a mode");
        assert_eq!(state.get(CONFIG_CONNECTION_MODE), "server");
    }
}
