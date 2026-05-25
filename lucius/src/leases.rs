use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::manifest::Manifest;

type Result<T> = std::result::Result<T, String>;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Lease {
    pub schema_version: u64,
    pub set_id: String,
    pub token: String,
    pub manifest_path: String,
    pub kind: String,
    pub root: String,
    pub pid: Option<u32>,
    pub created_at: u64,
    pub updated_at: u64,
}

pub struct LeaveOutcome {
    pub manifest_path: PathBuf,
    pub last: bool,
}

pub fn enter(
    manifest: &Manifest,
    manifest_path: &Path,
    kind: &str,
    root: &Path,
    pid: Option<u32>,
) -> Result<String> {
    let id = new_lease_id();
    let token = format!("{}:{id}", manifest.set_id);
    let now = now_secs()?;
    let lease = Lease {
        schema_version: 1,
        set_id: manifest.set_id.clone(),
        token: token.clone(),
        manifest_path: manifest_path.display().to_string(),
        kind: kind.to_string(),
        root: root.display().to_string(),
        pid,
        created_at: now,
        updated_at: now,
    };

    let dir = lease_dir(&manifest.set_id)?;
    fs::create_dir_all(&dir).map_err(|err| format!("failed to create {}: {err}", dir.display()))?;

    let path = lease_path(&token)?;
    let raw = serde_json::to_string_pretty(&lease)
        .map_err(|err| format!("failed to serialize lease: {err}"))?;
    fs::write(&path, raw).map_err(|err| format!("failed to write {}: {err}", path.display()))?;

    Ok(token)
}

pub fn leave(token: &str) -> Result<LeaveOutcome> {
    let path = lease_path(token)?;
    let lease = read_lease(&path)?;

    match fs::remove_file(&path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("failed to remove {}: {err}", path.display())),
    }

    prune_dead(&lease.set_id)?;
    let last = active_count(&lease.set_id)? == 0;

    Ok(LeaveOutcome {
        manifest_path: PathBuf::from(lease.manifest_path),
        last,
    })
}

pub fn heartbeat(token: &str) -> Result<()> {
    let path = lease_path(token)?;
    let mut lease = read_lease(&path)?;
    lease.updated_at = now_secs()?;
    let raw = serde_json::to_string_pretty(&lease)
        .map_err(|err| format!("failed to serialize lease: {err}"))?;
    fs::write(&path, raw).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

pub fn active_count(set_id: &str) -> Result<usize> {
    prune_dead(set_id)?;
    let dir = lease_dir(set_id)?;
    match dir.read_dir() {
        Ok(entries) => Ok(entries
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .count()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(err) => Err(format!("failed to read {}: {err}", dir.display())),
    }
}

pub fn list(set_id: &str) -> Result<Vec<Lease>> {
    prune_dead(set_id)?;
    let dir = lease_dir(set_id)?;
    let entries = match dir.read_dir() {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(format!("failed to read {}: {err}", dir.display())),
    };

    let mut leases = Vec::new();
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") {
            leases.push(read_lease(&path)?);
        }
    }
    leases.sort_by(|left, right| left.token.cmp(&right.token));
    Ok(leases)
}

pub fn prune_dead(set_id: &str) -> Result<usize> {
    let dir = lease_dir(set_id)?;
    let entries = match dir.read_dir() {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(err) => return Err(format!("failed to read {}: {err}", dir.display())),
    };

    let mut removed = 0;
    for entry in entries.filter_map(|entry| entry.ok()) {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }

        let remove = match read_lease(&path) {
            Ok(lease) => lease.pid.is_some_and(pid_is_gone),
            Err(_) => true,
        };

        if remove {
            match fs::remove_file(&path) {
                Ok(()) => removed += 1,
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(err) => return Err(format!("failed to remove {}: {err}", path.display())),
            }
        }
    }

    Ok(removed)
}

fn read_lease(path: &Path) -> Result<Lease> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    serde_json::from_str(&raw).map_err(|err| format!("failed to parse {}: {err}", path.display()))
}

fn lease_path(token: &str) -> Result<PathBuf> {
    let (set_id, id) = token
        .split_once(':')
        .ok_or_else(|| "lease token must have the form <set-id>:<id>".to_string())?;
    validate_token_component("lease set id", set_id)?;
    validate_token_component("lease id", id)?;
    Ok(lease_dir(set_id)?.join(format!("{id}.json")))
}

fn lease_dir(set_id: &str) -> Result<PathBuf> {
    validate_token_component("lease set id", set_id)?;
    Ok(runtime_home()?.join(set_id).join("leases"))
}

fn runtime_home() -> Result<PathBuf> {
    let xdg_runtime = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| "XDG_RUNTIME_DIR must be set to use ides leases".to_string())?;
    Ok(PathBuf::from(xdg_runtime).join("ides"))
}

fn new_lease_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{}-{nanos}", process::id())
}

fn now_secs() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|err| format!("system clock is before UNIX epoch: {err}"))
}

fn pid_is_gone(pid: u32) -> bool {
    !Path::new("/proc").join(pid.to_string()).exists()
}

fn validate_token_component(name: &str, value: &str) -> Result<()> {
    if value.contains(':') {
        return Err(format!("{name} must be a single safe path component"));
    }

    let mut components = Path::new(value).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(format!("{name} must be a single safe path component")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lease_tokens_cannot_escape_runtime_home() {
        assert!(lease_path("../set:id").unwrap_err().contains("safe path"));
        assert!(lease_path("set:../id").unwrap_err().contains("safe path"));
        assert!(lease_path("set:id:extra")
            .unwrap_err()
            .contains("safe path"));
    }
}
