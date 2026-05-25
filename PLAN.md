# ides Rust/Nix Migration Plan

This captures the current design direction for converting `ides` from generated
shell scripts into a Nix-generated manifest plus a small Rust runtime. The Rust
crate should be named `lucius`; the installed user-facing binary should remain
`ides`.

## Current Assessment

- `ides` is currently a thin wrapper around `pkgs.mkShell` that evaluates an
  `evalModules` configuration and passes non-ides arguments through to the
  wrapped shell function.
- Service definitions are lowered directly to generated shell scripts that call
  `systemd-run --user` and `systemctl --user`.
- Idempotency currently comes from hashing service config content into the
  systemd unit name. This is incomplete because command, package, environment,
  unit properties, dependencies, and lifecycle hooks also affect behavior.
- The current monitor tracks PIDs through a `socat` socket and `/proc` cwd
  checks. This is fragile and should be replaced by explicit leases from shell
  hooks and editor integrations.
- Transient units should still be used, but systemd should be treated as the
  process supervisor, not the owner of ides dependency semantics.
- The old OCaml `lucius` tree is useful as a conceptual guide: it sketches a
  daemon, DBus control surface, dependency graph, transient unit startup, and
  status monitoring. It should not be copied directly.
- The old Rust `lucius-rs` tree is currently only a stub.

## TheSast Fork Takeaways

The fork at `https://github.com/TheSast/ides` is small and should not be merged
as-is, but it contains a few useful ideas.

Strict improvements to keep:

- Add a raw command escape hatch. The fork's `serviceDefs.<name>.cmd` makes it
  possible to express commands directly instead of forcing `pkg + exec + args +
  %CFG%`. Keep this as an escape hatch, but do not make raw shell strings the
  only service API.
- Hash the actual executable command into service identity. The fork hashes
  `cmd`; the new implementation should hash the full normalized service
  manifest.
- Add a pre-start setup hook. The fork's `setup` is useful in spirit. Model this
  as structured lifecycle hooks, such as `preStart`, preferably inside the
  managed lifecycle rather than as anonymous shell outside the runtime.
- Add command introspection. The fork's `ides expose` is useful for debugging.
  Replace it with `ides inspect`, with `--json` support and enough information
  for editor integrations.

Do not take as-is:

- The fork breaks the bundled Redis module and Caddy example because it removes
  `pkg`, `args`, and `config` while leaving modules that still use them.
- Removing the config writer API is a regression. Typed/generated config is
  still valuable for reusable service modules.
- Raw shell command strings are fragile. Prefer structured `program` plus
  `args`, with `shellCommand` as an explicit escape hatch.
- The fork still has incomplete idempotency because `setup` and systemd unit
  properties are not included in the service hash.

## Design Goals

- Preserve the lightweight `mkShell` wrapper model.
- Provide a lower-level composable API so other shell builders can reuse the
  generated parts without calling `mkShell` directly.
- Keep Nix responsible for evaluation, option merging, config generation, and
  manifest creation.
- Keep Rust responsible for runtime state, systemd DBus calls, dependency
  transactions, leases, status reporting, and the TUI.
- Avoid persistent state by default. Runtime state should live under
  `$XDG_RUNTIME_DIR/ides/<set-id>/...`.
- Let users duplicate as much configuration as practical between NixOS machine
  modules and ides dev-service modules, but do this through a constrained
  adapter rather than pretending arbitrary NixOS modules map cleanly.
- Keep dependency count modest, but do not hand-roll DBus, argument parsing,
  serialization, terminal UI, or async primitives if mature crates solve them.

## Nix Architecture

Keep the public import shape:

```nix
let
  mkIdes = import ides {
    inherit pkgs;
    shell = pkgs.mkShell;
    modules = [ ];
  };
in
mkIdes {
  services.redis.enable = true;
  nativeBuildInputs = [ pkgs.hello ];
}
```

Add a composable parts API:

```nix
let
  idesParts = ides.lib.mkParts {
    inherit pkgs;
    modules = [ ];
  } {
    services.redis.enable = true;
  };
in
pkgs.mkShell (idesParts.shellArgs // {
  nativeBuildInputs = idesParts.shellArgs.nativeBuildInputs ++ [ pkgs.hello ];
})
```

Proposed generated parts:

- `manifest`: JSON store path consumed by the Rust runtime.
- `package`: package containing the `ides` binary.
- `shellArgs`: arguments ready to pass to `pkgs.mkShell`.
- `hooks`: shell-specific hook snippets for bash, zsh, fish, nushell, and a
  generic POSIX fallback.
- `config`: evaluated ides module config for advanced users.

New or revised options should support both structured and escape-hatch forms.
Do not build the API around `%CFG%`; service modules should use typed config
references that Nix resolves before manifest emission.

```nix
serviceDefs.web = {
  package = pkgs.caddy;
  executable = "caddy";
  argv = [
    "run"
    "-c"
    { config = "main"; }
    "--adapter"
    "caddyfile"
  ];

  # Escape hatch. Mutually exclusive with package/executable/argv.
  shellCommand = null;

  configs.main.text = ''
    http://127.0.0.1:8888 {
      respond "hello"
    }
  '';

  dependencies.requires = [ "redis" ];
  dependencies.after = [ "redis" ];

  lifecycle.preStart = [ ];
  lifecycle.readiness = {
    type = "tcp";
    host = "127.0.0.1";
    port = 8888;
    timeoutSec = 10;
  };

  runtime.ephemeral = true;
  runtime.env = { };
  systemd.service = { };
};
```

Prefer structured argv in the manifest:

```json
{
  "program": "/nix/store/.../bin/caddy",
  "argv": ["run", "-c", "/nix/store/.../config", "--adapter", "caddyfile"]
}
```

Chosen Nix-facing shape for config references:

```nix
configs.main.text = "...";
argv = [ "run" "-c" { config = "main"; } "--adapter" "caddyfile" ];
```

For static configs, the manifest contains final paths and structured argv
references. Rust should not know about `%CFG%` or string placeholder expansion.

Runtime-dependent configs should be a separate, explicit feature rather than a
new magic token. The clean shape is a typed template that renders into the
service runtime `config/` directory immediately before `StartTransientUnit`:

```nix
configs.main.runtime = {
  fileName = "service.conf";
  parts = [
    "data_dir = "
    { runtimePath = "data"; }
    "\nsocket_dir = "
    { runtimePath = "run"; }
    "\n"
  ];
};

argv = [ "--config" { config = "main"; } ];
```

The manifest keeps structured parts, not an interpolated string with sentinels.
Supported dynamic references are deliberately small at first: `runtimePath` and
`env`. A later endpoint/socket layer can add service-specific socket refs once
there is a declarative endpoint model. Rust renders runtime files under
`$XDG_RUNTIME_DIR/ides/<set-id>/services/<name>/config/`, passes that runtime
path through the same `{ config = "name"; }` argv reference, and cleanup remains
covered by the ephemeral runtime directory removal. Static configs should stay
store-backed so pure Nix evaluation remains the default path.

Support raw shell commands only as explicit escape hatches:

```json
{
  "shellCommand": "/nix/store/.../bin/caddy run -c /nix/store/.../config"
}
```

## Manifest Sketch

The manifest is the contract between Nix and Rust. It should be deterministic,
validated, and hashable.

```json
{
  "schemaVersion": 1,
  "setId": "sha256-of-normalized-manifest",
  "name": "default",
  "rootHint": "$PWD",
  "autoStart": true,
  "services": {
    "redis": {
      "unitName": "ides-default-redis-<hash>.service",
      "description": "ides redis",
      "exec": {
        "program": "/nix/store/.../bin/redis-server",
        "argv": ["/nix/store/.../redis.conf"],
        "cwd": null,
        "env": {}
      },
      "configs": {
        "main": "/nix/store/.../redis.conf"
      },
      "dependencies": {
        "requires": [],
        "wants": [],
        "after": [],
        "before": [],
        "partOf": []
      },
      "lifecycle": {
        "preStart": [],
        "postStart": [],
        "preStop": [],
        "readiness": { "type": "none" }
      },
      "runtime": {
        "baseDir": null,
        "ephemeral": true,
        "privateTmp": true,
        "xdgDirs": true,
        "env": {}
      },
      "systemd": {
        "service": {},
        "socket": {},
        "path": {},
        "timer": {}
      }
    }
  }
}
```

The service hash should cover at least:

- normalized exec program, args, shell command, cwd, and environment;
- generated config store paths and config content hashes;
- runtime settings and ides-managed environment;
- lifecycle hooks and readiness configuration;
- dependency metadata;
- relevant systemd unit properties.

## Rust Runtime Sketch

Crate/package layout:

```text
lucius/
  Cargo.toml
  Cargo.lock
  package.nix
  src/
    main.rs
    cli.rs
    manifest.rs
    daemon.rs
    graph.rs
    leases.rs
    systemd.rs
    tui.rs
```

Packaging:

```toml
[package]
name = "lucius"

[[bin]]
name = "ides"
path = "src/main.rs"
```

Likely crates:

- `zbus` for DBus/systemd user-manager calls.
- `serde` and `serde_json` for the manifest.
- `clap` or a smaller parser for CLI arguments.
- `petgraph` or a small local graph implementation for dependency
  transactions. Use a crate if lifecycle behavior becomes non-trivial.
- `ratatui` plus `crossterm` only when implementing the TUI. Keep this feature
  optional if dependency weight matters.
- Async runtime choice is still open. Prefer the smallest setup that works well
  with `zbus`; `smol`/`async-io` is attractive if it fits cleanly, otherwise
  use the runtime that avoids integration friction.

Primary commands:

```text
ides up [SERVICE...]
ides down [SERVICE...]
ides restart [SERVICE...]
ides status [SERVICE...]
ides inspect [SERVICE...] [--json]
ides tui
ides daemon --manifest MANIFEST
ides enter --manifest MANIFEST --kind shell --root DIR
ides leave --token TOKEN
ides heartbeat --token TOKEN
ides hook bash|zsh|fish|nu
```

The CLI can either:

- talk to a per-manifest daemon over a Unix socket or DBus name; or
- act directly for simple commands and start a daemon only for leases/TUI.

Prefer a daemon once leases are implemented, because it gives one owner for
graph state, active transactions, service status, and "what keeps this alive".

## Dependency Semantics

Do not rely on systemd to enforce ides service graph semantics.

Systemd transient units may accept dependency-like properties, but ides needs
additional behavior:

- lease-driven teardown;
- readiness-gated start order;
- set-level status;
- precise failure propagation;
- restart propagation;
- explanation of why a service or service set is alive.

Proposed semantics:

- `requires`: hard dependency. If a required dependency fails to start, the
  requesting service fails to start.
- `wants`: soft dependency. Start it when requested, but do not fail the parent
  if it cannot start unless configured otherwise.
- `after` / `before`: ordering constraints only.
- `partOf`: stop/restart propagation relationship.

Transactions:

- Normalize all dependency aliases into a directed graph.
- Detect cycles at manifest load time.
- Start dependencies in topological order.
- Stop dependents before dependencies.
- Restart as stop affected subgraph, then start in dependency order.
- Track desired state separately from observed systemd state.
- Readiness checks should gate dependents when configured.

## Systemd Integration

Use the user manager over DBus and `StartTransientUnit`.

Useful transient properties:

- `Description`
- `ExecStart`
- `WorkingDirectory`
- `Environment`
- `AddRef`
- `CollectMode=inactive-or-failed`
- `Restart` and related restart controls, when explicitly configured
- sandbox/runtime settings where supported by the user manager

Avoid shelling out to `systemd-run` for normal operation. Shelling out can
remain a debugging fallback while the DBus implementation matures.

Unit names must be sanitized and deterministic. User service names should never
be interpolated directly into unit names without normalization.

Status monitoring should use systemd DBus properties/signals where practical,
falling back to periodic `ListUnitsByNames` style polling if needed.

## Lease And Hook Model

Replace `/proc` cwd scanning with explicit leases.

Lease data should include:

- manifest path and set id;
- lease token;
- root/workspace path;
- kind: `shell`, `direnv`, `vscode`, `emacs`, `jetbrains`, etc.;
- pid if available;
- parent/owner metadata if available;
- last heartbeat timestamp;
- optional human-readable label.

Shell hooks:

- On shell entry: call `ides enter` and export `IDES_LEASE_TOKEN`.
- During shell lifetime: heartbeat periodically or rely on TTL plus exit trap.
- On shell exit: call `ides leave`.
- For direnv: account for reloads and parent shell behavior without walking
  arbitrary process trees.

IDE integrations should use the same lease API. A plugin does not need special
control privileges; it just creates and renews a lease for a workspace.

## Ephemerality Model

Default all writable service paths to runtime storage:

```text
$XDG_RUNTIME_DIR/ides/<set-id>/
  services/<service>/
    run/
    tmp/
    cache/
    config/
    data/
    home/
```

The runtime should provide environment defaults:

- `TMPDIR`
- `XDG_RUNTIME_DIR`
- `XDG_CACHE_HOME`
- `XDG_CONFIG_HOME`
- `XDG_DATA_HOME`
- optionally `HOME`, when a service is known to write into home by default

Avoid default use of persistent systemd directories such as `StateDirectory` and
`CacheDirectory`. Those are useful systemd features, but they intentionally
create persistent state and do not match ides defaults.

Modules must configure service-specific state paths. Examples:

- Redis should set data/log/socket paths into the service runtime directory.
- Postgres will need its cluster directory under runtime storage and clear
  initialization semantics.
- Caddy should write config/runtime paths under the service runtime directory
  unless the user explicitly opts into persistence.

## NixOS Module Conversion

Support this as an experimental adapter, not as a promise that arbitrary NixOS
modules can run unchanged.

Possible API:

```nix
ides.lib.fromNixos {
  inherit pkgs;
  modules = [
    ./some-service-module.nix
  ];
  mapPersistentPaths = false;
}
```

Initial lowering target:

- `config.systemd.services.<name>.serviceConfig.ExecStart`
- selected `Environment`, `WorkingDirectory`, restart settings, and sandbox
  settings;
- selected socket/timer/path units;
- package paths already present in generated commands.

Reject or require explicit mapping for:

- persistent `StateDirectory`, `CacheDirectory`, `LogsDirectory`;
- system users/groups;
- privileged system services;
- machine-level networking/firewall assumptions;
- activation scripts;
- global `environment.*` mutation.

Longer term, prefer shared service modules that can emit both NixOS services
and ides service definitions from common options.

## Adios Module System

The mergeable fork at `github:llakala/adios` is a possible future replacement
or helper for the module layer. Do not adopt it before the manifest and runtime
contract are stable.

Evaluation order:

1. Keep `lib.evalModules` for the first Rust-backed implementation.
2. Stabilize the ides manifest schema.
3. Compare Adios against the actual module needs: merge semantics, option docs,
   small dependency footprint, and compatibility with existing ides modules.
4. Switch only if it clearly reduces complexity or enables cleaner composition.

## Implementation Phases

Current progress:

- Phase 1 is implemented: `lucius` builds a user-facing `ides` binary, Nix
  emits an `IDES_MANIFEST`, and `ides inspect` can print text or JSON summaries.
- Phase 2 is implemented for plain services: `ides up/down/restart/status`
  talks directly to the user systemd manager over DBus and the generated shell
  wrapper delegates lifecycle commands to Rust.
- Phase 3 is partially implemented: dependency fields are emitted into the
  manifest and Rust applies `requires`, `wants`, `after`, `before`, and
  `partOf` ordering/propagation with cycle detection.
- `%CFG%` is no longer part of the planned public API. Service definitions use
  structured `argv`, named `configs`, and typed config references of the form
  `{ config = "name"; }`; the manifest preserves those refs so Rust can resolve
  either static store configs or runtime-rendered configs at start time.
- Runtime-rendered configs are implemented for `runtimePath` and `env` template
  parts. Static configs remain store-backed; runtime configs are written under
  the service runtime `config/` directory immediately before the transient unit
  is started.
- Phase 5 has started: manifests now include per-service runtime path metadata,
  the Rust runtime creates those paths under `$XDG_RUNTIME_DIR/ides/<set-id>/`,
  injects XDG/TMP/HOME defaults into transient units, uses runtime `data/` as
  the working directory, and cleans ephemeral runtime trees on `ides down`.
- Phase 4 has a per-manifest daemon lease path: `ides enter` starts an ides
  daemon as a transient user service, records explicit leases under the runtime
  root, and `leave` tears the service set down when the final lease is gone.
  The daemon prunes direct-hook leases whose recorded PID has exited, so a
  crashed shell does not keep services alive forever. `ides status --json`
  reports the active leases, giving the TUI and editor integrations an initial
  "what keeps this alive" substrate.
- Socket, path, and timer activation units now lower through DBus transient
  units. ides maps unit-file-style socket `Listen*` directives to DBus
  `Listen`, path watches to `Paths`, and timer directives to the transient
  setter forms `TimersMonotonic`/`TimersCalendar`.
- `ides status --json` now reports the manifest identity, runtime root, lease
  count/list, per-service service unit state, start unit state, activation unit
  states, expected runtime paths, effective runtime environment defaults, and
  static/runtime config paths without rendering new runtime files.
- Phase 6 has an initial no-dependency `ides tui`/`ides top` command. It
  reuses the status snapshot, renders service state, activation state,
  dependency hints, lease owners, runtime paths, and config paths, and refreshes
  in terminals while rendering once for piped output.
- Redis now uses a runtime-rendered config for writable paths. Its `dir` and
  `pidfile` point under the service runtime tree, and relative Redis sockets are
  placed under the runtime `run/` directory.
- A portable Nitro backend is now the preferred non-systemd portability path.
  The first version should run one unprivileged Nitro instance per ides set,
  generate Nitro service directories under the ides runtime tree, set
  `NITRO_SOCK` to a per-set runtime socket, and keep lucius responsible for
  manifest semantics, dependency transactions, runtime config rendering,
  leases, and status output. Nitro is the target for Linux non-systemd and
  POSIX-like systems; Darwin/macOS remains experimental until Nitro builds and
  runs there cleanly.
- Readiness gates, lifecycle hooks, declared endpoint/port metadata, the Nitro
  backend implementation, and a richer interactive TUI remain future phases.

Phase 1: Manifest and package skeleton

- Add Rust crate `lucius` with binary name `ides`.
- Add Nix package output for the binary.
- Add manifest generation from current evaluated service definitions.
- Add `ides inspect --manifest <path> --json`.
- Add evaluation checks for examples and Redis.

Phase 2: Direct systemd control

- Implement `ides up/down/status/restart --manifest <path>`.
- Use systemd DBus `StartTransientUnit` instead of generated `systemd-run`
  scripts.
- Preserve current single-service behavior before adding graph semantics.
- Add unit name sanitization and full-manifest hashing.

Phase 3: ides graph controller

- Add dependency options and manifest fields.
- Implement graph validation, topological start, reverse stop, restart
  transactions, and failure propagation.
- Add readiness gates.

Phase 4: Daemon and leases

- Add per-manifest daemon ownership. Done for shell leases.
- Add `enter`, `leave`, and `heartbeat`. Initial implementation done.
- Replace monitor shell script with shell-specific hooks. Initial POSIX-style
  shell hook done; zsh/fish/nushell and IDE integrations remain.
- Track "what keeps this alive" in daemon state.

Phase 5: Ephemeral runtime defaults

- Generate runtime directory layout.
- Inject XDG/TMP/HOME environment as appropriate.
- Convert Redis module to use runtime paths by default. Done for generated
  config `dir`, `pidfile`, and relative socket paths.
- Add Caddy and at least one database-style module as proof points.

Phase 6: TUI

- Add `ides tui` showing service sets, service state, dependency graph, leases,
  runtime paths, and recent lifecycle events. Initial service/lease/runtime
  dashboard is implemented; lifecycle events need daemon event recording first.
- Keep TUI dependencies optional if practical.

Phase 6.5: Portable supervisor backend

- Add `runtime.backend = "auto" | "systemd-user" | "nitro"`.
- Keep `systemd-user` as the default backend on Linux when a user manager is
  available.
- Implement `nitro` as an optional package/runtime component:
  - generate service directories under
    `$XDG_RUNTIME_DIR/ides/<set-id>/nitro/services`;
  - create one Nitro daemon per ides set;
  - set `NITRO_SOCK` to `$XDG_RUNTIME_DIR/ides/<set-id>/nitro/nitro.sock`;
  - emit foreground `run` scripts that exec the prepared service command with
    ides-rendered runtime configs and environment;
  - drive start/stop/status through `nitroctl`, while lucius keeps ownership of
    graph transactions and lease-driven shutdown;
  - use Nitro readiness notification where an ides readiness gate can map
    cleanly, otherwise keep readiness in lucius.
- Validate on Linux non-systemd first.
- Fork Nitro for an ides packaging/prototyping branch before depending on it
  as a backend. If the portability patch is small and works cleanly, send it
  upstream.
- Darwin/macOS support appears realistic but should be treated as unproven
  until the fork builds and passes a supervisor smoke test. Apple documents
  `AF_UNIX`/`SOCK_DGRAM`, `poll`, `pipe`, `dup2`, and `fcntl`, but current
  Nitro uses Linux/BSD convenience calls and flags that Darwin does not expose
  in the same shape:
  - replace `pipe2(..., O_CLOEXEC | O_NONBLOCK)` with `pipe` plus `fcntl`;
  - replace `dup3(..., O_CLOEXEC)` with `dup2` plus `fcntl`;
  - create sockets without `SOCK_CLOEXEC | SOCK_NONBLOCK`, then apply
    `FD_CLOEXEC`/`O_NONBLOCK` with `fcntl`;
  - always set `NITRO_SOCK` to an ides runtime path so non-Linux defaults do
    not try to bind under `/var/run`.

Phase 7: NixOS adapter and module-system revisit

- Prototype lowering a constrained subset of NixOS `systemd.services`.
  Initial subset is implemented inside ides as `systemd.services.<name>`:
  `script`, single-command `serviceConfig.ExecStart`, `path`, `environment`,
  and local service dependency lists lower into `serviceDefs`. Target
  installation fields such as `wantedBy` are accepted and ignored. External
  unit references such as `network-online.target` are ignored for the ides
  graph; local refs like `db.service` become `db`.
- Keep the adapter strict about `serviceConfig`: unsupported keys fail during
  evaluation instead of silently promising NixOS behavior that lucius cannot
  reproduce yet. Next useful additions are `WorkingDirectory`, `Restart`,
  lifecycle hooks, readiness, and service/socket/timer pairing once those
  manifest/runtime surfaces exist.
- Added a real NixOS-service smoke test for evaluated Redis, Nginx, Prometheus
  node exporter, and OpenSSH units. Full evaluated NixOS service attrsets are
  not directly accepted yet because they contain many unsupported NixOS/systemd
  options; the test extracts the currently supported subset and verifies the
  lowered ides manifest, including `sshd -> sshd-keygen` local dependencies.
- Broadened the NixOS-service compatibility sweep across Redis, Nginx,
  PostgreSQL, MariaDB/MySQL, Memcached, Caddy, Prometheus, Grafana, RabbitMQ,
  Mosquitto, Unbound, HAProxy, Traefik, Transmission, Gitea,
  static-web-server, Syncthing, and Zigbee2MQTT. The main strict improvements
  from that sweep are:
  - NixOS' common `ExecStart = [ "" "cmd" ]` reset form should lower to the
    remaining single command instead of failing;
  - empty reset entries in lifecycle hook lists should be filtered;
  - `Type = "dbus"` and `Type = "forking"` should fail explicitly for now
    rather than silently running as `simple`;
  - transient-safe lifecycle fields are worth preserving now:
    `ExecStartPre`, `ExecStartPost`, `ExecReload`, `ExecStop`,
    `RemainAfterExit`, `KillMode`, `RestartSec`, `TimeoutSec`,
    `TimeoutStartSec`, `TimeoutStopSec`, and `WatchdogSec`.
- Current sharp edges from the suite:
  - `dnsmasq` uses `Type=dbus`/`BusName`; supporting it would require deciding
    how user-session D-Bus activation maps into ides.
  - `bind` uses `Type=forking`; supporting that likely means either refusing
    daemonizing services or rewriting them to foreground mode where the NixOS
    module exposes one.
  - Many services depend on NixOS sandboxing, credentials, users/groups,
    `StateDirectory`, `RuntimeDirectory`, and read/write path policies. These
    are intentionally not promised by the first adapter; the dev-shell version
    should instead prefer ides runtime paths and generated configs over
    recreating system-level state.
  - `EnvironmentFile` appears in Caddy, Traefik, static-web-server, and other
    modules. Treat it as a later config-generation/secret-loading problem
    rather than blindly parsing and importing host files.
  - systemd exec prefixes are partially normalized for compatibility. The
    important `-` failure-ignore bit is modeled explicitly, and privilege
    prefixes such as `+`, `!`, and `!!` are intentionally discarded for the
    user-manager context. `@` argv0 override semantics are still not modeled.
- Decide whether Adios improves the service module layer.

## First Implementation Sketch

Initial file changes should be small and reversible:

- Add `lucius/Cargo.toml` and `lucius/src/main.rs` with `lucius` package and
  `ides` binary.
- Add `lucius/src/manifest.rs` with strict serde types for the manifest.
- Add `lib/manifest.nix` to lower current `serviceDefs` into JSON.
- Change `lib/build.nix` to add the Rust `ides` binary and set
  `IDES_MANIFEST`, but keep old generated scripts until Rust reaches parity.
- Add `checks` or simple Nix eval tests for:
  - config-less service;
  - text config service;
  - Redis module;
  - raw `shellCommand` escape hatch;
  - dependency cycle rejection once dependencies exist.

The first Rust command should be:

```text
ides inspect --manifest "$IDES_MANIFEST" --json
```

Then implement:

```text
ides up --manifest "$IDES_MANIFEST" redis
ides down --manifest "$IDES_MANIFEST" redis
ides status --manifest "$IDES_MANIFEST"
```

Only after that should shell hooks and leases replace the existing monitor.
