use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use systemd_zbus::zbus::blocking::Connection;
use systemd_zbus::zbus::zvariant::Value;
use systemd_zbus::{ActiveState, ManagerProxyBlocking, Mode, PathWatch, SubState, Unit};

use crate::leases;
use crate::manifest::{ConfigFile, Manifest, RuntimeConfigPart, Service};

type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy)]
enum ActivationKind {
    Socket,
    Path,
    Timer,
}

impl ActivationKind {
    fn suffix(self) -> &'static str {
        match self {
            Self::Socket => "socket",
            Self::Path => "path",
            Self::Timer => "timer",
        }
    }
}

impl std::fmt::Display for ActivationKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.suffix())
    }
}

pub fn up(manifest: &Manifest, targets: &[String]) -> Result<()> {
    with_manager(|manager| {
        let selected = activation_order(manifest, targets)?;

        for (name, service) in selected {
            start_service(manager, manifest, name, service)?;
        }

        Ok(())
    })
}

pub fn down(manifest: &Manifest, targets: &[String]) -> Result<()> {
    with_manager(|manager| {
        let selected = deactivation_order(manifest, targets)?;

        for (name, service) in selected {
            stop_service(manager, name, service)?;
            wait_until_stopped(manager, name, service)?;
            cleanup_runtime(manifest, name, service)?;
        }

        Ok(())
    })
}

pub fn restart(manifest: &Manifest, targets: &[String]) -> Result<()> {
    with_manager(|manager| {
        let restart_set = restart_set(manifest, targets)?;
        let mut stop_names = topo_sort(manifest, &restart_set)?;
        stop_names.reverse();

        for (name, service) in names_to_services(manifest, stop_names)? {
            stop_service(manager, name, service)?;
            wait_until_stopped(manager, name, service)?;
        }

        let mut start_set = restart_set;
        expand_activation_dependencies(manifest, &mut start_set)?;

        for (name, service) in names_to_services(manifest, topo_sort(manifest, &start_set)?)? {
            start_service(manager, manifest, name, service)?;
        }

        Ok(())
    })
}

pub fn status(manifest: &Manifest, targets: &[String], json: bool) -> Result<()> {
    if json {
        let status = status_snapshot(manifest, targets)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&status)
                .map_err(|err| format!("failed to serialize status: {err}"))?
        );
        return Ok(());
    }

    with_manager(|manager| {
        let selected = manifest.selected_services(targets)?;

        for (name, service) in selected {
            let status = service_status(manager, service)?;
            println!(
                "{name}\t{}\t{}\t{}",
                status.load, status.active, status.sub_state
            );
            println!("  unit: {}", status.name);
        }

        Ok(())
    })
}

pub fn status_snapshot(manifest: &Manifest, targets: &[String]) -> Result<serde_json::Value> {
    with_manager(|manager| {
        let selected = manifest.selected_services(targets)?;
        status_json(manager, manifest, selected)
    })
}

fn status_json(
    manager: &ManagerProxyBlocking<'_>,
    manifest: &Manifest,
    selected: Vec<(&str, &Service)>,
) -> Result<serde_json::Value> {
    let leases = leases::list(&manifest.set_id)?;
    let mut services = serde_json::Map::new();
    for (name, service) in selected {
        services.insert(
            name.to_string(),
            service_status_json(manager, manifest, name, service)?,
        );
    }

    Ok(serde_json::json!({
        "schemaVersion": 1,
        "name": &manifest.name,
        "setId": &manifest.set_id,
        "runtime": {
            "base": &manifest.runtime.base,
            "root": path_json(&runtime_root(manifest)?),
        },
        "leaseCount": leases.len(),
        "leases": leases,
        "services": services,
    }))
}

fn service_status_json(
    manager: &ManagerProxyBlocking<'_>,
    manifest: &Manifest,
    name: &str,
    service: &Service,
) -> Result<serde_json::Value> {
    let service_unit_name = service.systemd_unit_name();
    let start_unit_name = service_start_unit_name(service)?;
    let paths = runtime_paths(manifest, name, service)?;

    Ok(serde_json::json!({
        "serviceUnit": unit_status(manager, &service_unit_name)?.to_json(),
        "startUnit": unit_status(manager, &start_unit_name)?.to_json(),
        "activationUnits": activation_statuses(manager, service)?,
        "runtime": {
            "ephemeral": service.runtime.ephemeral,
            "paths": runtime_paths_json(&paths),
            "environment": runtime_environment_values(&paths, service),
        },
        "configs": configs_status_json(name, service, &paths)?,
    }))
}

fn activation_statuses(
    manager: &ManagerProxyBlocking<'_>,
    service: &Service,
) -> Result<Vec<serde_json::Value>> {
    activation_unit_specs(service)
        .into_iter()
        .map(|(kind, unit_name)| {
            Ok(serde_json::json!({
                "kind": kind.suffix(),
                "unit": unit_status(manager, &unit_name)?.to_json(),
            }))
        })
        .collect()
}

fn activation_order<'a>(
    manifest: &'a Manifest,
    targets: &[String],
) -> Result<Vec<(&'a str, &'a Service)>> {
    let mut selected = initial_set(manifest, targets)?;
    expand_activation_dependencies(manifest, &mut selected)?;
    names_to_services(manifest, topo_sort(manifest, &selected)?)
}

fn deactivation_order<'a>(
    manifest: &'a Manifest,
    targets: &[String],
) -> Result<Vec<(&'a str, &'a Service)>> {
    let mut selected = initial_set(manifest, targets)?;
    expand_part_of_dependents(manifest, &mut selected)?;

    let mut names = topo_sort(manifest, &selected)?;
    names.reverse();
    names_to_services(manifest, names)
}

fn restart_set(manifest: &Manifest, targets: &[String]) -> Result<BTreeSet<String>> {
    let mut selected = initial_set(manifest, targets)?;
    expand_part_of_dependents(manifest, &mut selected)?;
    Ok(selected)
}

fn initial_set(manifest: &Manifest, targets: &[String]) -> Result<BTreeSet<String>> {
    if targets.is_empty() {
        return Ok(manifest.services.keys().cloned().collect());
    }

    let mut selected = BTreeSet::new();
    for target in targets {
        ensure_service_exists(manifest, target)?;
        selected.insert(target.clone());
    }

    Ok(selected)
}

fn expand_activation_dependencies(
    manifest: &Manifest,
    selected: &mut BTreeSet<String>,
) -> Result<()> {
    let mut pending = selected.iter().cloned().collect::<Vec<_>>();

    while let Some(name) = pending.pop() {
        let service = get_service(manifest, &name)?;
        for dependency in service
            .dependencies
            .requires
            .iter()
            .chain(service.dependencies.wants.iter())
        {
            ensure_dependency_exists(manifest, &name, "requires/wants", dependency)?;
            if selected.insert(dependency.clone()) {
                pending.push(dependency.clone());
            }
        }
    }

    Ok(())
}

fn expand_part_of_dependents(manifest: &Manifest, selected: &mut BTreeSet<String>) -> Result<()> {
    validate_dependencies(manifest)?;

    loop {
        let mut changed = false;

        for (name, service) in &manifest.services {
            if selected.contains(name) {
                continue;
            }

            if service
                .dependencies
                .part_of
                .iter()
                .any(|owner| selected.contains(owner))
            {
                selected.insert(name.clone());
                changed = true;
            }
        }

        if !changed {
            return Ok(());
        }
    }
}

fn topo_sort(manifest: &Manifest, selected: &BTreeSet<String>) -> Result<Vec<String>> {
    validate_dependencies(manifest)?;

    let mut outgoing = BTreeMap::<String, BTreeSet<String>>::new();
    let mut indegree = BTreeMap::<String, usize>::new();

    for name in selected {
        outgoing.insert(name.clone(), BTreeSet::new());
        indegree.insert(name.clone(), 0);
    }

    for name in selected {
        let service = get_service(manifest, name)?;

        for dependency in service
            .dependencies
            .requires
            .iter()
            .chain(service.dependencies.wants.iter())
            .chain(service.dependencies.after.iter())
        {
            if selected.contains(dependency) {
                add_edge(&mut outgoing, &mut indegree, dependency, name);
            }
        }

        for dependency in &service.dependencies.before {
            if selected.contains(dependency) {
                add_edge(&mut outgoing, &mut indegree, name, dependency);
            }
        }
    }

    let mut ready = indegree
        .iter()
        .filter_map(|(name, count)| (*count == 0).then_some(name.clone()))
        .collect::<BTreeSet<_>>();
    let mut ordered = Vec::with_capacity(selected.len());

    while let Some(name) = ready.pop_first() {
        ordered.push(name.clone());

        for dependent in outgoing.remove(&name).unwrap_or_default() {
            let count = indegree
                .get_mut(&dependent)
                .ok_or_else(|| format!("dependency graph lost node `{dependent}`"))?;
            *count -= 1;
            if *count == 0 {
                ready.insert(dependent);
            }
        }
    }

    if ordered.len() != selected.len() {
        let cycle = indegree
            .into_iter()
            .filter_map(|(name, count)| (count > 0).then_some(name))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!("service dependency cycle detected around: {cycle}"));
    }

    Ok(ordered)
}

fn add_edge(
    outgoing: &mut BTreeMap<String, BTreeSet<String>>,
    indegree: &mut BTreeMap<String, usize>,
    from: &str,
    to: &str,
) {
    let inserted = outgoing
        .entry(from.to_string())
        .or_default()
        .insert(to.to_string());
    if inserted {
        *indegree.entry(to.to_string()).or_default() += 1;
    }
}

fn names_to_services(manifest: &Manifest, names: Vec<String>) -> Result<Vec<(&str, &Service)>> {
    names
        .into_iter()
        .map(|name| {
            manifest
                .services
                .get_key_value(&name)
                .map(|(name, service)| (name.as_str(), service))
                .ok_or_else(|| format!("manifest has no service `{name}`"))
        })
        .collect()
}

fn validate_dependencies(manifest: &Manifest) -> Result<()> {
    for (name, service) in &manifest.services {
        for dependency in service
            .dependencies
            .requires
            .iter()
            .chain(service.dependencies.wants.iter())
        {
            ensure_dependency_exists(manifest, name, "requires/wants", dependency)?;
        }
        for dependency in &service.dependencies.after {
            ensure_dependency_exists(manifest, name, "after", dependency)?;
        }
        for dependency in &service.dependencies.before {
            ensure_dependency_exists(manifest, name, "before", dependency)?;
        }
        for dependency in &service.dependencies.part_of {
            ensure_dependency_exists(manifest, name, "partOf", dependency)?;
        }
    }

    Ok(())
}

fn ensure_service_exists(manifest: &Manifest, name: &str) -> Result<()> {
    manifest
        .services
        .contains_key(name)
        .then_some(())
        .ok_or_else(|| format!("manifest has no service `{name}`"))
}

fn ensure_dependency_exists(
    manifest: &Manifest,
    service: &str,
    field: &str,
    dependency: &str,
) -> Result<()> {
    manifest
        .services
        .contains_key(dependency)
        .then_some(())
        .ok_or_else(|| {
            format!("service `{service}` has unknown `{field}` dependency `{dependency}`")
        })
}

fn get_service<'a>(manifest: &'a Manifest, name: &str) -> Result<&'a Service> {
    manifest
        .services
        .get(name)
        .ok_or_else(|| format!("manifest has no service `{name}`"))
}

fn with_manager<T>(action: impl FnOnce(&ManagerProxyBlocking<'_>) -> Result<T>) -> Result<T> {
    let connection = Connection::session()
        .map_err(|err| format!("failed to connect to the user DBus session: {err}"))?;
    let manager = ManagerProxyBlocking::new(&connection)
        .map_err(|err| format!("failed to connect to systemd user manager: {err}"))?;
    action(&manager)
}

fn start_service(
    manager: &ManagerProxyBlocking<'_>,
    manifest: &Manifest,
    name: &str,
    service: &Service,
) -> Result<()> {
    let unit_name = service_start_unit_name(service)?;

    if let Some(unit) = find_unit(manager, &unit_name)? {
        if is_started(&unit) {
            println!("[ides]: {name} already {}", active_state(&unit.active));
            return Ok(());
        }
    }

    let service_unit_name = service.systemd_unit_name();
    let service_description = format!("ides service {name}");
    let service_properties = service_properties(manifest, name, service, service_description)?;

    if let Some(kind) = activation_kind(service)? {
        let activation_unit_name = activation_unit_name(service, kind);
        let activation_description = format!("ides {kind} activation for {name}");
        let activation_properties = activation_properties(service, kind, activation_description)?;
        let aux = vec![(service_unit_name.as_str(), service_properties.as_slice())];
        manager
            .start_transient_unit(
                &activation_unit_name,
                Mode::Fail,
                &activation_properties,
                &aux,
            )
            .map_err(|err| {
                format!("failed to start {name} activation ({activation_unit_name}): {err}")
            })?;
        println!("[ides]: started {name} {kind}");
        return Ok(());
    }

    manager
        .start_transient_unit(&service_unit_name, Mode::Fail, &service_properties, &[])
        .map_err(|err| format!("failed to start {name} ({service_unit_name}): {err}"))?;

    println!("[ides]: started {name}");
    Ok(())
}

fn service_properties<'a>(
    manifest: &Manifest,
    name: &str,
    service: &'a Service,
    description: String,
) -> Result<Vec<(&'static str, Value<'a>)>> {
    let runtime = prepare_runtime(manifest, name, service)?;
    let configs = render_configs(name, service, &runtime)?;
    let (program, argv) = service
        .exec
        .systemd_argv(&manifest.runtime_shell, &configs)?;
    let exec_start = vec![(program, argv, service.systemd.ignore_start_failure)];
    let exec_start_pre = service
        .systemd
        .exec_start_pre
        .iter()
        .map(|command| {
            shell_exec_command(
                &manifest.runtime_shell,
                command.command(),
                command.ignore_failure(),
            )
        })
        .collect::<Vec<_>>();
    let exec_start_post = service
        .systemd
        .exec_start_post
        .iter()
        .map(|command| {
            shell_exec_command(
                &manifest.runtime_shell,
                command.command(),
                command.ignore_failure(),
            )
        })
        .collect::<Vec<_>>();
    let exec_reload = service
        .systemd
        .exec_reload
        .iter()
        .map(|command| {
            shell_exec_command(
                &manifest.runtime_shell,
                command.command(),
                command.ignore_failure(),
            )
        })
        .collect::<Vec<_>>();
    let exec_stop = service
        .systemd
        .exec_stop
        .iter()
        .map(|command| {
            shell_exec_command(
                &manifest.runtime_shell,
                command.command(),
                command.ignore_failure(),
            )
        })
        .collect::<Vec<_>>();
    let environment = runtime.environment;
    let working_directory = service
        .systemd
        .working_directory
        .clone()
        .unwrap_or_else(|| runtime.paths.data.to_string_lossy().to_string());
    let service_type = service.systemd.service_type.as_deref();

    let mut properties = vec![
        ("Description", Value::new(description)),
        ("CollectMode", Value::new("inactive-or-failed")),
        ("ExecStart", Value::new(exec_start)),
        ("Environment", Value::new(environment)),
        ("WorkingDirectory", Value::new(working_directory)),
    ];

    if !exec_start_pre.is_empty() {
        properties.push(("ExecStartPre", Value::new(exec_start_pre)));
    }
    if !exec_start_post.is_empty() {
        properties.push(("ExecStartPost", Value::new(exec_start_post)));
    }
    if !exec_reload.is_empty() {
        properties.push(("ExecReload", Value::new(exec_reload)));
    }
    if !exec_stop.is_empty() {
        properties.push(("ExecStop", Value::new(exec_stop)));
    }
    if let Some(service_type) = service_type {
        properties.push(("Type", Value::new(service_type.to_string())));
    }
    if let Some(notify_access) = service.systemd.notify_access.clone().or_else(|| {
        service_type
            .is_some_and(|service_type| service_type.starts_with("notify"))
            .then_some("all".to_string())
    }) {
        properties.push(("NotifyAccess", Value::new(notify_access)));
    }
    if let Some(restart) = &service.systemd.restart {
        properties.push(("Restart", Value::new(restart.clone())));
    }
    if let Some(restart_sec) = &service.systemd.restart_sec {
        properties.push((
            "RestartUSec",
            Value::new(parse_duration_usec("RestartSec", restart_sec)?),
        ));
    }
    if let Some(timeout_sec) = &service.systemd.timeout_sec {
        let timeout_usec = parse_duration_usec("TimeoutSec", timeout_sec)?;
        if service.systemd.timeout_start_sec.is_none() {
            properties.push(("TimeoutStartUSec", Value::new(timeout_usec)));
        }
        if service.systemd.timeout_stop_sec.is_none() {
            properties.push(("TimeoutStopUSec", Value::new(timeout_usec)));
        }
    }
    if let Some(timeout_start_sec) = &service.systemd.timeout_start_sec {
        properties.push((
            "TimeoutStartUSec",
            Value::new(parse_duration_usec("TimeoutStartSec", timeout_start_sec)?),
        ));
    }
    if let Some(timeout_stop_sec) = &service.systemd.timeout_stop_sec {
        properties.push((
            "TimeoutStopUSec",
            Value::new(parse_duration_usec("TimeoutStopSec", timeout_stop_sec)?),
        ));
    }
    if let Some(watchdog_sec) = &service.systemd.watchdog_sec {
        properties.push((
            "WatchdogUSec",
            Value::new(parse_duration_usec("WatchdogSec", watchdog_sec)?),
        ));
    }
    if let Some(kill_mode) = &service.systemd.kill_mode {
        properties.push(("KillMode", Value::new(kill_mode.clone())));
    }
    if let Some(remain_after_exit) = service.systemd.remain_after_exit {
        properties.push(("RemainAfterExit", Value::new(remain_after_exit)));
    }

    Ok(properties)
}

fn shell_exec_command(
    runtime_shell: &str,
    command: &str,
    ignore_failure: bool,
) -> (String, Vec<String>, bool) {
    (
        runtime_shell.to_string(),
        vec![
            runtime_shell.to_string(),
            "-lc".to_string(),
            command.to_string(),
        ],
        ignore_failure,
    )
}

fn activation_properties<'a>(
    service: &'a Service,
    kind: ActivationKind,
    description: String,
) -> Result<Vec<(&'static str, Value<'a>)>> {
    let mut properties = vec![
        ("Description", Value::new(description)),
        ("CollectMode", Value::new("inactive-or-failed")),
    ];

    match kind {
        ActivationKind::Socket => {
            socket_properties(&service.activation.socket, &mut properties)?;
        }
        ActivationKind::Path => {
            path_properties(&service.activation.path, &mut properties)?;
        }
        ActivationKind::Timer => {
            timer_properties(&service.activation.timer, &mut properties)?;
        }
    }

    Ok(properties)
}

fn activation_kind(service: &Service) -> Result<Option<ActivationKind>> {
    let mut kinds = Vec::new();
    if !service.activation.socket.is_empty() {
        kinds.push(ActivationKind::Socket);
    }
    if !service.activation.path.is_empty() {
        kinds.push(ActivationKind::Path);
    }
    if !service.activation.timer.is_empty() {
        kinds.push(ActivationKind::Timer);
    }

    match kinds.as_slice() {
        [] => Ok(None),
        [kind] => Ok(Some(*kind)),
        _ => Err("a service can use only one activation unit type at a time".to_string()),
    }
}

fn service_start_unit_name(service: &Service) -> Result<String> {
    Ok(match activation_kind(service)? {
        Some(kind) => activation_unit_name(service, kind),
        None => service.systemd_unit_name(),
    })
}

fn activation_unit_name(service: &Service, kind: ActivationKind) -> String {
    format!("{}.{}", service_unit_stem(service), kind.suffix())
}

fn service_unit_stem(service: &Service) -> String {
    service
        .systemd_unit_name()
        .strip_suffix(".service")
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| service.unit_name.clone())
}

fn path_condition(name: &str) -> Result<String> {
    match name {
        "PathExists"
        | "PathExistsGlob"
        | "PathChanged"
        | "PathModified"
        | "PathDirectoryNotEmpty" => Ok(name.to_string()),
        other => Err(format!(
            "`{other}` is not supported by ides path activation lowering yet"
        )),
    }
}

fn socket_properties<'a>(
    socket: &BTreeMap<String, Vec<String>>,
    properties: &mut Vec<(&'static str, Value<'a>)>,
) -> Result<()> {
    let mut listen = Vec::<(String, String)>::new();

    for (name, values) in socket {
        if let Some(listen_type) = socket_listen_type(name) {
            for value in values {
                listen.push((listen_type.to_string(), value.clone()));
            }
            continue;
        }

        match name.as_str() {
            "Accept" => push_bool(properties, "Accept", name, values)?,
            "Backlog" => push_u32(properties, "Backlog", name, values)?,
            "BindIPv6Only" => push_string(properties, "BindIPv6Only", name, values)?,
            "BindToDevice" => push_string(properties, "BindToDevice", name, values)?,
            "Broadcast" => push_bool(properties, "Broadcast", name, values)?,
            "DeferAcceptSec" => push_duration(properties, "DeferAcceptUSec", name, values)?,
            "DirectoryMode" => push_mode(properties, "DirectoryMode", name, values)?,
            "FileDescriptorName" => push_string(properties, "FileDescriptorName", name, values)?,
            "FlushPending" => push_bool(properties, "FlushPending", name, values)?,
            "FreeBind" => push_bool(properties, "FreeBind", name, values)?,
            "IPTOS" => push_i32(properties, "IPTOS", name, values)?,
            "IPTTL" => push_i32(properties, "IPTTL", name, values)?,
            "KeepAlive" => push_bool(properties, "KeepAlive", name, values)?,
            "KeepAliveIntervalSec" => {
                push_duration(properties, "KeepAliveIntervalUSec", name, values)?
            }
            "KeepAliveProbes" => push_u32(properties, "KeepAliveProbes", name, values)?,
            "KeepAliveTimeSec" => push_duration(properties, "KeepAliveTimeUSec", name, values)?,
            "Mark" => push_i32(properties, "Mark", name, values)?,
            "MaxConnections" => push_u32(properties, "MaxConnections", name, values)?,
            "MaxConnectionsPerSource" => {
                push_u32(properties, "MaxConnectionsPerSource", name, values)?
            }
            "NoDelay" => push_bool(properties, "NoDelay", name, values)?,
            "PassCredentials" => push_bool(properties, "PassCredentials", name, values)?,
            "PassEnvironment" => properties.push(("PassEnvironment", Value::new(values.clone()))),
            "PassPacketInfo" => push_bool(properties, "PassPacketInfo", name, values)?,
            "PassSecurity" => push_bool(properties, "PassSecurity", name, values)?,
            "PipeSize" => push_u64(properties, "PipeSize", name, values)?,
            "Priority" => push_i32(properties, "Priority", name, values)?,
            "ReceiveBuffer" => push_u64(properties, "ReceiveBuffer", name, values)?,
            "RemoveOnStop" => push_bool(properties, "RemoveOnStop", name, values)?,
            "SendBuffer" => push_u64(properties, "SendBuffer", name, values)?,
            "SocketGroup" => push_string(properties, "SocketGroup", name, values)?,
            "SocketMode" => push_mode(properties, "SocketMode", name, values)?,
            "SocketProtocol" => push_i32(properties, "SocketProtocol", name, values)?,
            "SocketUser" => push_string(properties, "SocketUser", name, values)?,
            "Symlinks" => properties.push(("Symlinks", Value::new(values.clone()))),
            "TCPCongestion" => push_string(properties, "TCPCongestion", name, values)?,
            "TimeoutSec" => push_duration(properties, "TimeoutUSec", name, values)?,
            "Timestamping" => push_string(properties, "Timestamping", name, values)?,
            "Transparent" => push_bool(properties, "Transparent", name, values)?,
            "TriggerLimitBurst" => push_u32(properties, "TriggerLimitBurst", name, values)?,
            "TriggerLimitIntervalSec" => {
                push_duration(properties, "TriggerLimitIntervalUSec", name, values)?
            }
            "Writable" => push_bool(properties, "Writable", name, values)?,
            other => return Err(unsupported_activation("socket", other)),
        }
    }

    if !listen.is_empty() {
        properties.push(("Listen", Value::new(listen)));
    }

    Ok(())
}

fn socket_listen_type(name: &str) -> Option<&'static str> {
    match name {
        "ListenDatagram" => Some("Datagram"),
        "ListenFIFO" => Some("FIFO"),
        "ListenMessageQueue" => Some("MessageQueue"),
        "ListenNetlink" => Some("Netlink"),
        "ListenSequentialPacket" => Some("SequentialPacket"),
        "ListenSpecial" => Some("Special"),
        "ListenStream" => Some("Stream"),
        "ListenUSBFunction" => Some("USBFunction"),
        _ => None,
    }
}

fn path_properties<'a>(
    path: &BTreeMap<String, Vec<String>>,
    properties: &mut Vec<(&'static str, Value<'a>)>,
) -> Result<()> {
    let mut paths = Vec::new();

    for (name, values) in path {
        match name.as_str() {
            "PathExists"
            | "PathExistsGlob"
            | "PathChanged"
            | "PathModified"
            | "PathDirectoryNotEmpty" => {
                for value in values {
                    paths.push(PathWatch {
                        condition: path_condition(name)?,
                        path: value.clone(),
                    });
                }
            }
            "MakeDirectory" => push_bool(properties, "MakeDirectory", name, values)?,
            "DirectoryMode" => push_mode(properties, "DirectoryMode", name, values)?,
            "TriggerLimitBurst" => push_u32(properties, "TriggerLimitBurst", name, values)?,
            "TriggerLimitIntervalSec" => {
                push_duration(properties, "TriggerLimitIntervalUSec", name, values)?
            }
            other => return Err(unsupported_activation("path", other)),
        }
    }

    if !paths.is_empty() {
        properties.push(("Paths", Value::new(paths)));
    }

    Ok(())
}

fn timer_properties<'a>(
    timer: &BTreeMap<String, Vec<String>>,
    properties: &mut Vec<(&'static str, Value<'a>)>,
) -> Result<()> {
    let mut monotonic = Vec::new();
    let mut calendar = Vec::new();

    for (name, values) in timer {
        if let Some(base) = timer_monotonic_base(name) {
            for value in values {
                monotonic.push((base.to_string(), parse_duration_usec(name, value)?));
            }
            continue;
        }

        match name.as_str() {
            "OnCalendar" => {
                for value in values {
                    calendar.push(("OnCalendar".to_string(), value.clone()));
                }
            }
            "AccuracySec" => push_duration(properties, "AccuracyUSec", name, values)?,
            "OnClockChange" => push_bool(properties, "OnClockChange", name, values)?,
            "OnTimezoneChange" => push_bool(properties, "OnTimezoneChange", name, values)?,
            "Persistent" => push_bool(properties, "Persistent", name, values)?,
            "RandomizedDelaySec" => push_duration(properties, "RandomizedDelayUSec", name, values)?,
            "RemainAfterElapse" => push_bool(properties, "RemainAfterElapse", name, values)?,
            "WakeSystem" => push_bool(properties, "WakeSystem", name, values)?,
            other => return Err(unsupported_activation("timer", other)),
        }
    }

    if !monotonic.is_empty() {
        properties.push(("TimersMonotonic", Value::new(monotonic)));
    }
    if !calendar.is_empty() {
        properties.push(("TimersCalendar", Value::new(calendar)));
    }

    Ok(())
}

fn timer_monotonic_base(name: &str) -> Option<&'static str> {
    match name {
        "OnActiveSec" => Some("OnActiveSec"),
        "OnBootSec" => Some("OnBootSec"),
        "OnStartupSec" => Some("OnStartupSec"),
        "OnUnitActiveSec" => Some("OnUnitActiveSec"),
        "OnUnitInactiveSec" => Some("OnUnitInactiveSec"),
        _ => None,
    }
}

fn push_bool<'a>(
    properties: &mut Vec<(&'static str, Value<'a>)>,
    dbus_name: &'static str,
    option_name: &str,
    values: &[String],
) -> Result<()> {
    properties.push((
        dbus_name,
        Value::new(parse_bool(option_name, expect_one(option_name, values)?)?),
    ));
    Ok(())
}

fn push_string<'a>(
    properties: &mut Vec<(&'static str, Value<'a>)>,
    dbus_name: &'static str,
    option_name: &str,
    values: &[String],
) -> Result<()> {
    properties.push((
        dbus_name,
        Value::new(expect_one(option_name, values)?.to_string()),
    ));
    Ok(())
}

fn push_u32<'a>(
    properties: &mut Vec<(&'static str, Value<'a>)>,
    dbus_name: &'static str,
    option_name: &str,
    values: &[String],
) -> Result<()> {
    let value = expect_one(option_name, values)?
        .parse::<u32>()
        .map_err(|err| format!("invalid {option_name} value: {err}"))?;
    properties.push((dbus_name, Value::new(value)));
    Ok(())
}

fn push_i32<'a>(
    properties: &mut Vec<(&'static str, Value<'a>)>,
    dbus_name: &'static str,
    option_name: &str,
    values: &[String],
) -> Result<()> {
    let value = expect_one(option_name, values)?
        .parse::<i32>()
        .map_err(|err| format!("invalid {option_name} value: {err}"))?;
    properties.push((dbus_name, Value::new(value)));
    Ok(())
}

fn push_u64<'a>(
    properties: &mut Vec<(&'static str, Value<'a>)>,
    dbus_name: &'static str,
    option_name: &str,
    values: &[String],
) -> Result<()> {
    let value = expect_one(option_name, values)?
        .parse::<u64>()
        .map_err(|err| format!("invalid {option_name} value: {err}"))?;
    properties.push((dbus_name, Value::new(value)));
    Ok(())
}

fn push_mode<'a>(
    properties: &mut Vec<(&'static str, Value<'a>)>,
    dbus_name: &'static str,
    option_name: &str,
    values: &[String],
) -> Result<()> {
    properties.push((
        dbus_name,
        Value::new(parse_mode(option_name, expect_one(option_name, values)?)?),
    ));
    Ok(())
}

fn push_duration<'a>(
    properties: &mut Vec<(&'static str, Value<'a>)>,
    dbus_name: &'static str,
    option_name: &str,
    values: &[String],
) -> Result<()> {
    properties.push((
        dbus_name,
        Value::new(parse_duration_usec(
            option_name,
            expect_one(option_name, values)?,
        )?),
    ));
    Ok(())
}

fn expect_one<'a>(name: &str, values: &'a [String]) -> Result<&'a str> {
    match values {
        [value] => Ok(value),
        [] => Err(format!("{name} requires one value")),
        _ => Err(format!("{name} accepts only one value")),
    }
}

fn parse_bool(name: &str, value: &str) -> Result<bool> {
    match value.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(format!("invalid boolean for {name}: `{value}`")),
    }
}

fn parse_mode(name: &str, value: &str) -> Result<u32> {
    let trimmed = value.trim();
    let (radix, digits) = trimmed
        .strip_prefix("0o")
        .or_else(|| trimmed.strip_prefix("0O"))
        .map(|digits| (8, digits))
        .unwrap_or_else(|| {
            if trimmed.starts_with('0') && trimmed.len() > 1 {
                (8, trimmed)
            } else {
                (10, trimmed)
            }
        });

    u32::from_str_radix(digits, radix).map_err(|err| format!("invalid mode for {name}: {err}"))
}

fn parse_duration_usec(name: &str, value: &str) -> Result<u64> {
    let mut input = value.trim();
    if input.is_empty() {
        return Err(format!("{name} requires a non-empty duration"));
    }
    if input.eq_ignore_ascii_case("infinity") {
        return Ok(u64::MAX);
    }

    let mut total = 0.0_f64;
    while !input.is_empty() {
        input = input.trim_start();
        let number_len = input
            .char_indices()
            .take_while(|(_, character)| character.is_ascii_digit() || *character == '.')
            .last()
            .map(|(index, character)| index + character.len_utf8())
            .unwrap_or(0);
        if number_len == 0 {
            return Err(format!("invalid duration for {name}: `{value}`"));
        }

        let number = input[..number_len]
            .parse::<f64>()
            .map_err(|err| format!("invalid duration for {name}: {err}"))?;
        input = input[number_len..].trim_start();

        let unit_len = input
            .char_indices()
            .take_while(|(_, character)| character.is_ascii_alphabetic())
            .last()
            .map(|(index, character)| index + character.len_utf8())
            .unwrap_or(0);
        let unit = &input[..unit_len];
        input = &input[unit_len..];

        total += number * duration_unit_multiplier(name, unit)?;
    }

    if !total.is_finite() || total < 0.0 || total > u64::MAX as f64 {
        return Err(format!("duration for {name} is out of range: `{value}`"));
    }

    Ok(total.round() as u64)
}

fn duration_unit_multiplier(name: &str, unit: &str) -> Result<f64> {
    let multiplier = match unit {
        "" | "s" | "sec" | "secs" | "second" | "seconds" => 1_000_000.0,
        "us" | "usec" | "usecs" | "microsecond" | "microseconds" => 1.0,
        "ms" | "msec" | "msecs" | "millisecond" | "milliseconds" => 1_000.0,
        "m" | "min" | "mins" | "minute" | "minutes" => 60_000_000.0,
        "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000_000.0,
        "d" | "day" | "days" => 86_400_000_000.0,
        "w" | "week" | "weeks" => 604_800_000_000.0,
        _ => return Err(format!("unsupported duration unit for {name}: `{unit}`")),
    };
    Ok(multiplier)
}

fn unsupported_activation(kind: &str, option: &str) -> String {
    format!("`{option}` is not supported by ides transient {kind} activation lowering yet")
}

fn stop_service(manager: &ManagerProxyBlocking<'_>, name: &str, service: &Service) -> Result<()> {
    let unit_name = service.systemd_unit_name();
    for activation_unit in activation_unit_names(service)? {
        stop_unit_if_loaded(manager, name, &activation_unit)?;
    }

    stop_unit_if_loaded(manager, name, &unit_name)?;

    Ok(())
}

fn stop_unit_if_loaded(
    manager: &ManagerProxyBlocking<'_>,
    name: &str,
    unit_name: &str,
) -> Result<()> {
    match find_unit(manager, unit_name)? {
        Some(unit) if is_stoppable(&unit) => {
            manager
                .stop_unit(unit_name, Mode::Replace)
                .map_err(|err| format!("failed to stop {name} ({unit_name}): {err}"))?;
            println!("[ides]: stopped {name}");
        }
        Some(_) => println!("[ides]: {name} already stopped"),
        None => println!("[ides]: {name} is not loaded"),
    }

    Ok(())
}

fn activation_unit_names(service: &Service) -> Result<Vec<String>> {
    Ok(activation_unit_specs(service)
        .into_iter()
        .map(|(_, unit)| unit)
        .collect())
}

fn activation_unit_specs(service: &Service) -> Vec<(ActivationKind, String)> {
    let mut units = Vec::new();
    if !service.activation.socket.is_empty() {
        units.push((
            ActivationKind::Socket,
            activation_unit_name(service, ActivationKind::Socket),
        ));
    }
    if !service.activation.path.is_empty() {
        units.push((
            ActivationKind::Path,
            activation_unit_name(service, ActivationKind::Path),
        ));
    }
    if !service.activation.timer.is_empty() {
        units.push((
            ActivationKind::Timer,
            activation_unit_name(service, ActivationKind::Timer),
        ));
    }
    units
}

fn wait_until_stopped(
    manager: &ManagerProxyBlocking<'_>,
    name: &str,
    service: &Service,
) -> Result<()> {
    let unit_name = service.systemd_unit_name();
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        match find_unit(manager, &unit_name)? {
            Some(unit) if is_stoppable(&unit) => {
                if Instant::now() >= deadline {
                    return Err(format!(
                        "timed out waiting for {name} ({unit_name}) to stop"
                    ));
                }
                thread::sleep(Duration::from_millis(50));
            }
            _ => return Ok(()),
        }
    }
}

fn prepare_runtime(manifest: &Manifest, name: &str, service: &Service) -> Result<PreparedRuntime> {
    let paths = runtime_paths(manifest, name, service)?;

    if service.runtime.ephemeral {
        for path in paths.all() {
            fs::create_dir_all(path)
                .map_err(|err| format!("failed to create {}: {err}", path.display()))?;
        }
    }

    let environment_values = runtime_environment_values(&paths, service);
    let environment = environment_values
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();

    Ok(PreparedRuntime {
        paths,
        environment,
        environment_values,
    })
}

fn runtime_environment_values(
    paths: &RuntimePathSet,
    service: &Service,
) -> BTreeMap<String, String> {
    let mut environment_values = BTreeMap::from([
        ("TMPDIR".to_string(), paths.tmp.display().to_string()),
        (
            "XDG_RUNTIME_DIR".to_string(),
            paths.run.display().to_string(),
        ),
        (
            "XDG_CACHE_HOME".to_string(),
            paths.cache.display().to_string(),
        ),
        (
            "XDG_CONFIG_HOME".to_string(),
            paths.config.display().to_string(),
        ),
        (
            "XDG_DATA_HOME".to_string(),
            paths.data.display().to_string(),
        ),
        ("HOME".to_string(), paths.home.display().to_string()),
        (
            "IDES_RUNTIME_DIR".to_string(),
            paths.base.display().to_string(),
        ),
    ]);

    environment_values.extend(service.runtime.env.clone());
    environment_values
}

fn render_configs(
    service_name: &str,
    service: &Service,
    runtime: &PreparedRuntime,
) -> Result<BTreeMap<String, String>> {
    service
        .configs
        .iter()
        .map(|(name, config)| {
            let path = render_config(service_name, name, config, runtime)?;
            Ok((name.clone(), path))
        })
        .collect()
}

fn render_config(
    service_name: &str,
    config_name: &str,
    config: &ConfigFile,
    runtime: &PreparedRuntime,
) -> Result<String> {
    if let Some(path) = &config.path {
        return Ok(path.clone());
    }

    let Some(template) = &config.runtime else {
        return Err(format!(
            "service `{service_name}` config `{config_name}` has neither a static path nor a runtime template"
        ));
    };

    let path = runtime_config_path(
        &runtime.paths.config,
        service_name,
        config_name,
        &template.file_name,
    )?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }

    let mut rendered = String::new();
    for part in &template.parts {
        match part {
            RuntimeConfigPart::Text(text) => rendered.push_str(text),
            RuntimeConfigPart::RuntimePath { runtime_path } => {
                rendered.push_str(
                    &runtime
                        .paths
                        .by_name(service_name, runtime_path)?
                        .display()
                        .to_string(),
                );
            }
            RuntimeConfigPart::Env { env } => {
                let value = runtime.environment_values.get(env).ok_or_else(|| {
                    format!(
                        "service `{service_name}` config `{config_name}` references missing env `{env}`"
                    )
                })?;
                rendered.push_str(value);
            }
        }
    }

    fs::write(&path, rendered)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(path.to_string_lossy().to_string())
}

fn runtime_config_path(
    config_dir: &Path,
    service_name: &str,
    config_name: &str,
    file_name: &str,
) -> Result<PathBuf> {
    if file_name.is_empty() {
        return Err(format!(
            "service `{service_name}` config `{config_name}` runtime fileName must not be empty"
        ));
    }

    let relative = Path::new(file_name);
    if !is_safe_relative_path(relative) {
        return Err(format!(
            "service `{service_name}` config `{config_name}` runtime fileName must be relative and stay inside the runtime config directory"
        ));
    }

    Ok(config_dir.join(relative))
}

fn configs_status_json(
    service_name: &str,
    service: &Service,
    paths: &RuntimePathSet,
) -> Result<serde_json::Value> {
    let mut configs = serde_json::Map::new();
    for (name, config) in &service.configs {
        configs.insert(
            name.clone(),
            config_status_json(service_name, name, config, paths)?,
        );
    }
    Ok(serde_json::Value::Object(configs))
}

fn config_status_json(
    service_name: &str,
    config_name: &str,
    config: &ConfigFile,
    paths: &RuntimePathSet,
) -> Result<serde_json::Value> {
    if let Some(path) = &config.path {
        return Ok(serde_json::json!({
            "kind": "static",
            "path": path,
            "exists": Path::new(path).exists(),
        }));
    }

    let Some(template) = &config.runtime else {
        return Err(format!(
            "service `{service_name}` config `{config_name}` has neither a static path nor a runtime template"
        ));
    };
    let path = runtime_config_path(
        &paths.config,
        service_name,
        config_name,
        &template.file_name,
    )?;

    Ok(serde_json::json!({
        "kind": "runtime",
        "path": path_json(&path),
        "fileName": &template.file_name,
        "exists": path.exists(),
    }))
}

fn cleanup_runtime(manifest: &Manifest, name: &str, service: &Service) -> Result<()> {
    if !service.runtime.ephemeral {
        return Ok(());
    }

    let paths = runtime_paths(manifest, name, service)?;
    match fs::remove_dir_all(&paths.base) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("failed to remove {}: {err}", paths.base.display())),
    }
}

fn runtime_paths(manifest: &Manifest, name: &str, service: &Service) -> Result<RuntimePathSet> {
    let root = runtime_root(manifest)?;
    let paths = &service.runtime.paths;
    let base = runtime_path(&root, name, "base", &paths.base)?;
    let run = runtime_path_under_base(&root, &base, name, "run", &paths.run)?;
    let tmp = runtime_path_under_base(&root, &base, name, "tmp", &paths.tmp)?;
    let cache = runtime_path_under_base(&root, &base, name, "cache", &paths.cache)?;
    let config = runtime_path_under_base(&root, &base, name, "config", &paths.config)?;
    let data = runtime_path_under_base(&root, &base, name, "data", &paths.data)?;
    let home = runtime_path_under_base(&root, &base, name, "home", &paths.home)?;

    Ok(RuntimePathSet {
        base,
        run,
        tmp,
        cache,
        config,
        data,
        home,
    })
}

fn runtime_root(manifest: &Manifest) -> Result<PathBuf> {
    let xdg_runtime = env::var_os("XDG_RUNTIME_DIR")
        .ok_or_else(|| "XDG_RUNTIME_DIR must be set to use ides runtime paths".to_string())?;
    let base = Path::new(&manifest.runtime.base);
    if !is_safe_relative_path(base) {
        return Err(
            "manifest runtime base must be relative and contain only normal path components"
                .to_string(),
        );
    }
    Ok(PathBuf::from(xdg_runtime).join(base))
}

fn runtime_path(root: &Path, service: &str, name: &str, relative: &str) -> Result<PathBuf> {
    if relative.is_empty() {
        return Err(format!(
            "service `{service}` has no runtime path configured for `{name}`"
        ));
    }

    let path = Path::new(relative);
    if !is_safe_relative_path(path) {
        return Err(format!(
            "service `{service}` runtime path `{name}` must be relative and contain only normal path components"
        ));
    }

    Ok(root.join(path))
}

fn runtime_path_under_base(
    root: &Path,
    base: &Path,
    service: &str,
    name: &str,
    relative: &str,
) -> Result<PathBuf> {
    let path = runtime_path(root, service, name, relative)?;
    if !path.starts_with(base) {
        return Err(format!(
            "service `{service}` runtime path `{name}` must stay under its base runtime path"
        ));
    }
    Ok(path)
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path.components().next().is_some()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn runtime_paths_json(paths: &RuntimePathSet) -> serde_json::Value {
    serde_json::json!({
        "base": path_json(&paths.base),
        "run": path_json(&paths.run),
        "tmp": path_json(&paths.tmp),
        "cache": path_json(&paths.cache),
        "config": path_json(&paths.config),
        "data": path_json(&paths.data),
        "home": path_json(&paths.home),
    })
}

fn path_json(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn service_status(manager: &ManagerProxyBlocking<'_>, service: &Service) -> Result<UnitStatus> {
    let unit_name = service_start_unit_name(service)?;
    unit_status(manager, &unit_name)
}

fn unit_status(manager: &ManagerProxyBlocking<'_>, unit_name: &str) -> Result<UnitStatus> {
    let status = match find_unit(manager, unit_name)? {
        Some(unit) => UnitStatus {
            name: unit_name.to_string(),
            load: load_state(&unit.load).to_string(),
            active: active_state(&unit.active).to_string(),
            sub_state: sub_state(&unit.sub_state).to_string(),
        },
        None => UnitStatus {
            name: unit_name.to_string(),
            load: "not-found".to_string(),
            active: "inactive".to_string(),
            sub_state: "dead".to_string(),
        },
    };

    Ok(status)
}

fn find_unit(manager: &ManagerProxyBlocking<'_>, unit_name: &str) -> Result<Option<Unit>> {
    let units = manager
        .list_units_by_names(&[unit_name])
        .map_err(|err| format!("failed to query {unit_name}: {err}"))?;

    Ok(units
        .into_iter()
        .find(|unit| unit.name == unit_name && load_state(&unit.load) != "not-found"))
}

fn is_started(unit: &Unit) -> bool {
    matches!(
        unit.active,
        ActiveState::Active | ActiveState::Activating | ActiveState::Reloading
    )
}

fn is_stoppable(unit: &Unit) -> bool {
    !matches!(unit.active, ActiveState::Inactive | ActiveState::Failed)
}

fn load_state(state: &systemd_zbus::LoadState) -> &'static str {
    state.into()
}

fn active_state(state: &ActiveState) -> &'static str {
    state.into()
}

fn sub_state(state: &SubState) -> &'static str {
    state.into()
}

struct UnitStatus {
    name: String,
    load: String,
    active: String,
    sub_state: String,
}

struct PreparedRuntime {
    paths: RuntimePathSet,
    environment: Vec<String>,
    environment_values: BTreeMap<String, String>,
}

#[derive(Debug)]
struct RuntimePathSet {
    base: PathBuf,
    run: PathBuf,
    tmp: PathBuf,
    cache: PathBuf,
    config: PathBuf,
    data: PathBuf,
    home: PathBuf,
}

impl RuntimePathSet {
    fn all(&self) -> [&Path; 7] {
        [
            &self.base,
            &self.run,
            &self.tmp,
            &self.cache,
            &self.config,
            &self.data,
            &self.home,
        ]
    }

    fn by_name(&self, service: &str, name: &str) -> Result<&Path> {
        match name {
            "base" => Ok(&self.base),
            "run" => Ok(&self.run),
            "tmp" => Ok(&self.tmp),
            "cache" => Ok(&self.cache),
            "config" => Ok(&self.config),
            "data" => Ok(&self.data),
            "home" => Ok(&self.home),
            other => Err(format!(
                "service `{service}` references unknown runtime path `{other}`"
            )),
        }
        .map(PathBuf::as_path)
    }
}

impl UnitStatus {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "name": &self.name,
            "load": &self.load,
            "active": &self.active,
            "subState": &self.sub_state,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::manifest::{
        Activation, Dependencies, Exec, ManifestRuntime, RuntimePaths, Service, ServiceRuntime,
    };

    use super::*;

    #[test]
    fn activation_pulls_required_services_first() {
        let manifest = test_manifest(vec![
            ("db", Dependencies::default()),
            (
                "app",
                Dependencies {
                    requires: vec!["db".to_string()],
                    after: vec!["db".to_string()],
                    ..Dependencies::default()
                },
            ),
        ]);

        let targets = vec!["app".to_string()];
        assert_eq!(
            ordered_names(activation_order(&manifest, &targets).unwrap()),
            ["db", "app"]
        );
    }

    #[test]
    fn deactivation_stops_part_of_dependents_first() {
        let manifest = test_manifest(vec![
            ("app", Dependencies::default()),
            (
                "worker",
                Dependencies {
                    part_of: vec!["app".to_string()],
                    after: vec!["app".to_string()],
                    ..Dependencies::default()
                },
            ),
        ]);

        let targets = vec!["app".to_string()];
        assert_eq!(
            ordered_names(deactivation_order(&manifest, &targets).unwrap()),
            ["worker", "app"]
        );
    }

    #[test]
    fn cycles_are_rejected() {
        let manifest = test_manifest(vec![
            (
                "a",
                Dependencies {
                    after: vec!["b".to_string()],
                    ..Dependencies::default()
                },
            ),
            (
                "b",
                Dependencies {
                    after: vec!["a".to_string()],
                    ..Dependencies::default()
                },
            ),
        ]);

        let err = activation_order(&manifest, &[]).unwrap_err();
        assert!(err.contains("cycle"));
    }

    #[test]
    fn systemd_durations_accept_unit_file_spellings() {
        assert_eq!(parse_duration_usec("RestartSec", "3").unwrap(), 3_000_000);
        assert_eq!(
            parse_duration_usec("TimeoutStartSec", "1min 30s").unwrap(),
            90_000_000
        );
        assert_eq!(
            parse_duration_usec("TimeoutSec", "infinity").unwrap(),
            u64::MAX
        );
    }

    #[test]
    fn runtime_paths_cannot_escape_the_service_base() {
        std::env::set_var("XDG_RUNTIME_DIR", "/tmp/ides-test-runtime");
        let mut manifest = test_manifest(vec![("app", Dependencies::default())]);

        {
            let service = manifest.services.get_mut("app").unwrap();
            service.runtime.paths.data = "services/other/data".to_string();
        }
        let service = manifest.services.get("app").unwrap();
        let err = runtime_paths(&manifest, "app", service).unwrap_err();
        assert!(err.contains("must stay under its base"));

        {
            let service = manifest.services.get_mut("app").unwrap();
            service.runtime.paths.data = "../escape".to_string();
        }
        let service = manifest.services.get("app").unwrap();
        let err = runtime_paths(&manifest, "app", service).unwrap_err();
        assert!(err.contains("only normal path components"));
    }

    #[test]
    fn manifest_runtime_base_cannot_escape_xdg_runtime_dir() {
        std::env::set_var("XDG_RUNTIME_DIR", "/tmp/ides-test-runtime");
        let mut manifest = test_manifest(vec![("app", Dependencies::default())]);
        manifest.runtime.base = "../escape".to_string();

        let service = manifest.services.get("app").unwrap();
        let err = runtime_paths(&manifest, "app", service).unwrap_err();
        assert!(err.contains("manifest runtime base"));
    }

    fn test_manifest(services: Vec<(&str, Dependencies)>) -> Manifest {
        Manifest {
            schema_version: 1,
            set_id: "test".to_string(),
            name: "test".to_string(),
            auto_start: false,
            runtime_shell: "/bin/sh".to_string(),
            runtime: ManifestRuntime {
                base: "ides/test".to_string(),
            },
            services: services
                .into_iter()
                .map(|(name, dependencies)| {
                    (
                        name.to_string(),
                        Service {
                            unit_name: format!("ides-test-{name}.service"),
                            exec: Exec {
                                program: None,
                                argv: Vec::new(),
                                shell_command: Some("/bin/true".to_string()),
                                command: "/bin/true".to_string(),
                            },
                            configs: BTreeMap::new(),
                            activation: Activation::default(),
                            runtime: ServiceRuntime {
                                ephemeral: true,
                                env: BTreeMap::new(),
                                paths: RuntimePaths {
                                    base: format!("services/{name}"),
                                    run: format!("services/{name}/run"),
                                    tmp: format!("services/{name}/tmp"),
                                    cache: format!("services/{name}/cache"),
                                    config: format!("services/{name}/config"),
                                    data: format!("services/{name}/data"),
                                    home: format!("services/{name}/home"),
                                },
                            },
                            systemd: Default::default(),
                            systemd_args: String::new(),
                            dependencies,
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn ordered_names(ordered: Vec<(&str, &Service)>) -> Vec<String> {
        ordered
            .into_iter()
            .map(|(name, _)| name.to_string())
            .collect()
    }
}
