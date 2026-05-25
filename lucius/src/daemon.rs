use std::env;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Component, Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use systemd_zbus::zbus::blocking::Connection;
use systemd_zbus::zbus::zvariant::Value;
use systemd_zbus::{ManagerProxyBlocking, Mode};

use crate::leases;
use crate::manifest::Manifest;
use crate::systemd;

type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "op", rename_all = "camelCase")]
enum Request {
    Ping,
    Enter {
        manifest_path: String,
        kind: String,
        root: String,
        pid: Option<u32>,
    },
    Leave {
        token: String,
    },
    Heartbeat {
        token: String,
    },
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    ok: bool,
    token: Option<String>,
    last: bool,
    error: Option<String>,
}

impl Response {
    fn ok() -> Self {
        Self {
            ok: true,
            token: None,
            last: false,
            error: None,
        }
    }

    fn token(token: String) -> Self {
        Self {
            ok: true,
            token: Some(token),
            last: false,
            error: None,
        }
    }

    fn last(last: bool) -> Self {
        Self {
            ok: true,
            token: None,
            last,
            error: None,
        }
    }

    fn error(error: String) -> Self {
        Self {
            ok: false,
            token: None,
            last: false,
            error: Some(error),
        }
    }
}

pub fn enter(
    manifest: &Manifest,
    manifest_path: &Path,
    kind: &str,
    root: &Path,
    pid: Option<u32>,
) -> Result<String> {
    ensure_started(manifest, manifest_path)?;
    let response = send(
        manifest,
        &Request::Enter {
            manifest_path: manifest_path.display().to_string(),
            kind: kind.to_string(),
            root: root.display().to_string(),
            pid,
        },
    )?;
    response
        .token
        .ok_or_else(|| "daemon did not return a lease token".to_string())
}

pub fn leave(token: &str) -> Result<()> {
    if let Some(set_id) = token.split_once(':').map(|(set_id, _)| set_id) {
        let manifest = Manifest {
            schema_version: 1,
            set_id: set_id.to_string(),
            name: String::new(),
            auto_start: false,
            runtime_shell: String::new(),
            runtime: Default::default(),
            services: Default::default(),
        };
        if send(
            &manifest,
            &Request::Leave {
                token: token.to_string(),
            },
        )
        .is_ok()
        {
            return Ok(());
        }
    }

    leave_without_daemon(token)
}

pub fn heartbeat(token: &str) -> Result<()> {
    if let Some(set_id) = token.split_once(':').map(|(set_id, _)| set_id) {
        let manifest = Manifest {
            schema_version: 1,
            set_id: set_id.to_string(),
            name: String::new(),
            auto_start: false,
            runtime_shell: String::new(),
            runtime: Default::default(),
            services: Default::default(),
        };
        if send(
            &manifest,
            &Request::Heartbeat {
                token: token.to_string(),
            },
        )
        .is_ok()
        {
            return Ok(());
        }
    }

    leases::heartbeat(token)
}

pub fn serve(manifest: &Manifest, manifest_path: &Path) -> Result<()> {
    let socket = socket_path(manifest)?;
    if let Some(parent) = socket.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    match fs::remove_file(&socket) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("failed to remove {}: {err}", socket.display())),
    }

    let listener = UnixListener::bind(&socket)
        .map_err(|err| format!("failed to bind {}: {err}", socket.display()))?;
    listener
        .set_nonblocking(true)
        .map_err(|err| format!("failed to set daemon socket nonblocking: {err}"))?;

    let mut idle_since = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                idle_since = Instant::now();
                if handle_client(stream, manifest, manifest_path)? {
                    break;
                }
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                let pruned = leases::prune_dead(&manifest.set_id)?;
                let active = leases::active_count(&manifest.set_id)?;
                if active == 0 && pruned > 0 {
                    systemd::down(manifest, &[])?;
                    break;
                }
                if active == 0 && idle_since.elapsed() > Duration::from_secs(10) {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(err) => return Err(format!("failed to accept daemon client: {err}")),
        }
    }

    match fs::remove_file(&socket) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("failed to remove {}: {err}", socket.display())),
    }

    Ok(())
}

fn handle_client(
    mut stream: UnixStream,
    manifest: &Manifest,
    _manifest_path: &Path,
) -> Result<bool> {
    let mut raw = String::new();
    {
        let mut reader = BufReader::new(&mut stream);
        reader
            .read_line(&mut raw)
            .map_err(|err| format!("failed to read daemon request: {err}"))?;
    }

    let request = serde_json::from_str::<Request>(&raw)
        .map_err(|err| format!("failed to parse daemon request: {err}"));
    let mut should_exit = false;
    let response = match request {
        Ok(Request::Ping) => Response::ok(),
        Ok(Request::Enter {
            manifest_path,
            kind,
            root,
            pid,
        }) => match leases::enter(
            manifest,
            Path::new(&manifest_path),
            &kind,
            Path::new(&root),
            pid,
        ) {
            Ok(token) => Response::token(token),
            Err(err) => Response::error(err),
        },
        Ok(Request::Leave { token }) => match leases::leave(&token) {
            Ok(outcome) => {
                if outcome.last {
                    if let Err(err) = systemd::down(manifest, &[]) {
                        Response::error(err)
                    } else {
                        should_exit = true;
                        Response::last(true)
                    }
                } else {
                    Response::last(false)
                }
            }
            Err(err) => Response::error(err),
        },
        Ok(Request::Heartbeat { token }) => match leases::heartbeat(&token) {
            Ok(()) => Response::ok(),
            Err(err) => Response::error(err),
        },
        Err(err) => Response::error(err),
    };

    let raw = serde_json::to_string(&response)
        .map_err(|err| format!("failed to serialize daemon response: {err}"))?;
    writeln!(stream, "{raw}").map_err(|err| format!("failed to write daemon response: {err}"))?;

    Ok(should_exit)
}

fn ensure_started(manifest: &Manifest, manifest_path: &Path) -> Result<()> {
    if send(manifest, &Request::Ping).is_ok() {
        return Ok(());
    }

    start_daemon_unit(manifest, manifest_path)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if send(manifest, &Request::Ping).is_ok() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for ides daemon socket".to_string());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn start_daemon_unit(manifest: &Manifest, manifest_path: &Path) -> Result<()> {
    let exe =
        env::current_exe().map_err(|err| format!("failed to find current executable: {err}"))?;
    let exe = exe.to_string_lossy().to_string();
    let manifest_path = manifest_path.to_string_lossy().to_string();
    let argv = [exe.as_str(), "daemon", "--manifest", manifest_path.as_str()];
    let exec_start = vec![(exe.as_str(), argv.to_vec(), false)];
    let unit_name = daemon_unit_name(manifest);
    let description = format!("ides daemon {}", manifest.name);
    let environment = env::var("XDG_RUNTIME_DIR")
        .ok()
        .map(|value| vec![format!("XDG_RUNTIME_DIR={value}")])
        .unwrap_or_default();
    let environment = environment.iter().map(String::as_str).collect::<Vec<_>>();
    let properties = vec![
        ("Description", Value::new(description)),
        ("CollectMode", Value::new("inactive-or-failed")),
        ("ExecStart", Value::new(exec_start)),
        ("Environment", Value::new(environment)),
    ];

    let connection = Connection::session()
        .map_err(|err| format!("failed to connect to the user DBus session: {err}"))?;
    let manager = ManagerProxyBlocking::new(&connection)
        .map_err(|err| format!("failed to connect to systemd user manager: {err}"))?;
    manager
        .start_transient_unit(&unit_name, Mode::Replace, &properties, &[])
        .map_err(|err| format!("failed to start ides daemon ({unit_name}): {err}"))?;

    Ok(())
}

fn leave_without_daemon(token: &str) -> Result<()> {
    let outcome = leases::leave(token)?;
    if outcome.last {
        let raw = fs::read_to_string(&outcome.manifest_path)
            .map_err(|err| format!("failed to read {}: {err}", outcome.manifest_path.display()))?;
        let manifest = Manifest::from_json(&raw)
            .map_err(|err| format!("failed to parse {}: {err}", outcome.manifest_path.display()))?;
        systemd::down(&manifest, &[])?;
    }
    Ok(())
}

fn send(manifest: &Manifest, request: &Request) -> Result<Response> {
    let socket = socket_path(manifest)?;
    let mut stream = UnixStream::connect(&socket)
        .map_err(|err| format!("failed to connect to {}: {err}", socket.display()))?;
    let raw = serde_json::to_string(request)
        .map_err(|err| format!("failed to serialize request: {err}"))?;
    writeln!(stream, "{raw}").map_err(|err| format!("failed to write daemon request: {err}"))?;

    let mut response = String::new();
    let mut reader = BufReader::new(stream);
    reader
        .read_line(&mut response)
        .map_err(|err| format!("failed to read daemon response: {err}"))?;

    let response = serde_json::from_str::<Response>(&response)
        .map_err(|err| format!("failed to parse daemon response: {err}"))?;
    if response.ok {
        Ok(response)
    } else {
        Err(response
            .error
            .unwrap_or_else(|| "daemon request failed".to_string()))
    }
}

fn socket_path(manifest: &Manifest) -> Result<PathBuf> {
    let xdg_runtime = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| "XDG_RUNTIME_DIR must be set to use the ides daemon".to_string())?;
    let base = Path::new(&manifest.runtime.base);
    if !is_safe_relative_path(base) {
        return Err(
            "manifest runtime base must be relative and contain only normal path components"
                .to_string(),
        );
    }
    Ok(PathBuf::from(xdg_runtime).join(base).join("daemon.sock"))
}

fn daemon_unit_name(manifest: &Manifest) -> String {
    let short = manifest.set_id.chars().take(16).collect::<String>();
    format!("ides-daemon-{short}.service")
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path.components().next().is_some()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::manifest::ManifestRuntime;

    use super::*;

    #[test]
    fn daemon_socket_cannot_escape_xdg_runtime_dir() {
        std::env::set_var("XDG_RUNTIME_DIR", "/tmp/ides-test-runtime");
        let manifest = Manifest {
            schema_version: 1,
            set_id: "test".to_string(),
            name: "test".to_string(),
            auto_start: false,
            runtime_shell: "/bin/sh".to_string(),
            runtime: ManifestRuntime {
                base: "../escape".to_string(),
            },
            services: BTreeMap::new(),
        };

        let err = socket_path(&manifest).unwrap_err();
        assert!(err.contains("manifest runtime base"));
    }
}
