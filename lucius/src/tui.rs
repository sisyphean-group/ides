use std::fmt::Write as _;
use std::io::{self, IsTerminal, Write};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::manifest::{Dependencies, Manifest};
use crate::systemd;

type Result<T> = std::result::Result<T, String>;

pub fn run(manifest: &Manifest, targets: &[String]) -> Result<()> {
    let interactive = io::stdout().is_terminal();

    loop {
        let screen = match systemd::status_snapshot(manifest, targets) {
            Ok(status) => render(manifest, &status),
            Err(err) => render_error(manifest, &err),
        };

        let mut stdout = io::stdout();
        if interactive {
            write!(stdout, "\x1b[2J\x1b[H").map_err(|err| format!("failed to draw tui: {err}"))?;
        }
        stdout
            .write_all(screen.as_bytes())
            .map_err(|err| format!("failed to draw tui: {err}"))?;
        stdout
            .flush()
            .map_err(|err| format!("failed to draw tui: {err}"))?;

        if !interactive {
            return Ok(());
        }

        thread::sleep(Duration::from_secs(1));
    }
}

fn render(manifest: &Manifest, status: &Value) -> String {
    let mut out = String::new();
    let name = value_str(status, "name").unwrap_or(&manifest.name);
    let set_id = value_str(status, "setId").unwrap_or(&manifest.set_id);
    let runtime = status.get("runtime").unwrap_or(&Value::Null);
    let root = value_str(runtime, "root").unwrap_or("-");
    let lease_count = status
        .get("leaseCount")
        .and_then(Value::as_u64)
        .unwrap_or_default();

    writeln!(out, "ides tui").ok();
    writeln!(
        out,
        "set: {name}  id: {}  refreshed: {}",
        short(set_id, 12),
        now_secs()
    )
    .ok();
    writeln!(out, "runtime: {root}").ok();
    writeln!(out, "leases: {lease_count}").ok();
    writeln!(out).ok();

    render_services(manifest, status, &mut out);
    render_leases(status, &mut out);

    out
}

fn render_services(manifest: &Manifest, status: &Value, out: &mut String) {
    writeln!(out, "services").ok();
    writeln!(
        out,
        "{:<24} {:<18} {:<18} {:<18} kept by",
        "name", "start", "service", "activation"
    )
    .ok();
    writeln!(out, "{}", "-".repeat(96)).ok();

    let Some(services) = status.get("services").and_then(Value::as_object) else {
        writeln!(out, "no services").ok();
        return;
    };

    for (name, service_status) in services {
        let start = unit_state(service_status.get("startUnit"));
        let service = unit_state(service_status.get("serviceUnit"));
        let activation = activation_state(service_status);
        let dependencies = manifest
            .services
            .get(name)
            .map(|service| dependency_summary(&service.dependencies))
            .unwrap_or_else(|| "-".to_string());

        writeln!(
            out,
            "{:<24} {:<18} {:<18} {:<18} {}",
            short(name, 24),
            short(&start, 18),
            short(&service, 18),
            short(&activation, 18),
            dependencies
        )
        .ok();

        render_service_details(name, service_status, out);
    }
}

fn render_service_details(name: &str, service_status: &Value, out: &mut String) {
    if let Some(runtime_paths) = service_status
        .get("runtime")
        .and_then(|runtime| runtime.get("paths"))
    {
        let base = value_str(runtime_paths, "base").unwrap_or("-");
        let run = value_str(runtime_paths, "run").unwrap_or("-");
        let data = value_str(runtime_paths, "data").unwrap_or("-");
        writeln!(out, "  {name}.runtime base={base}").ok();
        writeln!(out, "  {name}.runtime run={run}").ok();
        writeln!(out, "  {name}.runtime data={data}").ok();
    }

    let Some(configs) = service_status.get("configs").and_then(Value::as_object) else {
        return;
    };
    for (config_name, config) in configs {
        let kind = value_str(config, "kind").unwrap_or("unknown");
        let path = value_str(config, "path").unwrap_or("-");
        let exists = config
            .get("exists")
            .and_then(Value::as_bool)
            .map(|exists| if exists { "exists" } else { "missing" })
            .unwrap_or("unknown");
        writeln!(out, "  {name}.config.{config_name} {kind} {exists} {path}").ok();
    }
}

fn render_leases(status: &Value, out: &mut String) {
    writeln!(out).ok();
    writeln!(out, "leases").ok();
    writeln!(
        out,
        "{:<18} {:<10} {:<8} {:<12} root",
        "token", "kind", "pid", "updated"
    )
    .ok();
    writeln!(out, "{}", "-".repeat(96)).ok();

    let Some(leases) = status.get("leases").and_then(Value::as_array) else {
        writeln!(out, "no leases").ok();
        return;
    };

    if leases.is_empty() {
        writeln!(out, "no leases").ok();
        return;
    }

    for lease in leases {
        let token = value_str(lease, "token").unwrap_or("-");
        let kind = value_str(lease, "kind").unwrap_or("-");
        let root = value_str(lease, "root").unwrap_or("-");
        let pid = lease
            .get("pid")
            .and_then(Value::as_u64)
            .map(|pid| pid.to_string())
            .unwrap_or_else(|| "-".to_string());
        let updated = lease
            .get("updatedAt")
            .and_then(Value::as_u64)
            .map(|timestamp| timestamp.to_string())
            .unwrap_or_else(|| "-".to_string());

        writeln!(
            out,
            "{:<18} {:<10} {:<8} {:<12} {}",
            short(token, 18),
            short(kind, 10),
            short(&pid, 8),
            short(&updated, 12),
            root
        )
        .ok();
    }
}

fn render_error(manifest: &Manifest, err: &str) -> String {
    let mut out = String::new();
    writeln!(out, "ides tui").ok();
    writeln!(
        out,
        "set: {}  id: {}",
        manifest.name,
        short(&manifest.set_id, 12)
    )
    .ok();
    writeln!(out, "error: {err}").ok();
    out
}

fn unit_state(unit: Option<&Value>) -> String {
    let Some(unit) = unit else {
        return "-".to_string();
    };
    let active = value_str(unit, "active").unwrap_or("-");
    let sub_state = value_str(unit, "subState").unwrap_or("-");
    format!("{active}/{sub_state}")
}

fn activation_state(service_status: &Value) -> String {
    let Some(units) = service_status
        .get("activationUnits")
        .and_then(Value::as_array)
    else {
        return "-".to_string();
    };
    if units.is_empty() {
        return "-".to_string();
    }

    units
        .iter()
        .map(|activation| {
            let kind = value_str(activation, "kind").unwrap_or("activation");
            format!("{kind}:{}", unit_state(activation.get("unit")))
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn dependency_summary(dependencies: &Dependencies) -> String {
    let mut parts = Vec::new();
    push_dependency(&mut parts, "requires", &dependencies.requires);
    push_dependency(&mut parts, "wants", &dependencies.wants);
    push_dependency(&mut parts, "after", &dependencies.after);
    push_dependency(&mut parts, "before", &dependencies.before);
    push_dependency(&mut parts, "partOf", &dependencies.part_of);

    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join(" ")
    }
}

fn push_dependency(parts: &mut Vec<String>, label: &str, values: &[String]) {
    if !values.is_empty() {
        parts.push(format!("{label}={}", values.join(",")));
    }
}

fn value_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn short(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }

    let mut shortened = value
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    shortened.push('~');
    shortened
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}
