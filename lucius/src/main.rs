mod daemon;
mod leases;
mod manifest;
mod systemd;
mod tui;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

use manifest::{Manifest, Service};

#[derive(Debug)]
struct CommandArgs {
    manifest: Option<PathBuf>,
    json: bool,
    services: Vec<String>,
    kind: Option<String>,
    root: Option<PathBuf>,
    pid: Option<u32>,
    token: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Command {
    Inspect,
    Up,
    Down,
    Restart,
    Status,
    Enter,
    Leave,
    Heartbeat,
    Daemon,
    Tui,
    Help,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("ides: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let (command, args) = parse_args(env::args().skip(1))?;

    match command {
        Command::Help => {
            print_help();
            Ok(())
        }
        Command::Inspect => {
            let (manifest, path) = load_manifest(args.manifest.clone())?;
            inspect(&manifest, &path, &args)
        }
        Command::Up => {
            let (manifest, _) = load_manifest(args.manifest)?;
            systemd::up(&manifest, &args.services)
        }
        Command::Down => {
            let (manifest, _) = load_manifest(args.manifest)?;
            systemd::down(&manifest, &args.services)
        }
        Command::Restart => {
            let (manifest, _) = load_manifest(args.manifest)?;
            systemd::restart(&manifest, &args.services)
        }
        Command::Status => {
            let (manifest, _) = load_manifest(args.manifest)?;
            systemd::status(&manifest, &args.services, args.json)
        }
        Command::Enter => {
            let (manifest, path) = load_manifest(args.manifest)?;
            let root = match args.root {
                Some(root) => root,
                None => env::current_dir().map_err(|err| format!("failed to read cwd: {err}"))?,
            };
            let token = daemon::enter(
                &manifest,
                &path,
                args.kind.as_deref().unwrap_or("shell"),
                &root,
                Some(args.pid.unwrap_or_else(process::id)),
            )?;
            println!("{token}");
            Ok(())
        }
        Command::Leave => {
            let token = args
                .token
                .ok_or_else(|| "pass --token <token> or set IDES_LEASE_TOKEN".to_string())?;
            daemon::leave(&token)
        }
        Command::Heartbeat => {
            let token = args
                .token
                .ok_or_else(|| "pass --token <token> or set IDES_LEASE_TOKEN".to_string())?;
            daemon::heartbeat(&token)
        }
        Command::Daemon => {
            let (manifest, path) = load_manifest(args.manifest)?;
            daemon::serve(&manifest, &path)
        }
        Command::Tui => {
            let (manifest, _) = load_manifest(args.manifest)?;
            tui::run(&manifest, &args.services)
        }
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<(Command, CommandArgs), String> {
    let mut command = None;
    let mut manifest = None;
    let mut json = false;
    let mut services = Vec::new();
    let mut kind = None;
    let mut root = None;
    let mut pid = None;
    let mut token = None;
    let mut iter = args;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => command = Some(Command::Help),
            "--json" => json = true,
            "--kind" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("missing value for {arg}"))?;
                kind = Some(value);
            }
            "--root" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("missing value for {arg}"))?;
                root = Some(PathBuf::from(value));
            }
            "--pid" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("missing value for {arg}"))?;
                pid = Some(
                    value
                        .parse::<u32>()
                        .map_err(|err| format!("invalid --pid value `{value}`: {err}"))?,
                );
            }
            "--token" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("missing value for {arg}"))?;
                token = Some(value);
            }
            "--manifest" | "-m" => {
                let value = iter
                    .next()
                    .ok_or_else(|| format!("missing value for {arg}"))?;
                manifest = Some(PathBuf::from(value));
            }
            _ if arg.starts_with("--manifest=") => {
                let (_, value) = arg
                    .split_once('=')
                    .ok_or_else(|| "invalid --manifest option".to_string())?;
                manifest = Some(PathBuf::from(value));
            }
            _ if arg.starts_with("--token=") => {
                let (_, value) = arg
                    .split_once('=')
                    .ok_or_else(|| "invalid --token option".to_string())?;
                token = Some(value.to_string());
            }
            _ if arg.starts_with('-') => return Err(format!("unknown option `{arg}`")),
            _ if command.is_none() => command = Some(Command::parse(&arg)?),
            _ => services.push(arg),
        }
    }

    Ok((
        command.unwrap_or(Command::Help),
        CommandArgs {
            manifest,
            json,
            services,
            kind,
            root,
            pid,
            token: token.or_else(|| env::var("IDES_LEASE_TOKEN").ok()),
        },
    ))
}

impl Command {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "inspect" | "show" | "expose" => Ok(Self::Inspect),
            "run" | "r" | "up" | "start" => Ok(Self::Up),
            "stop" | "s" | "clean" | "et-tu" | "down" => Ok(Self::Down),
            "restart" | "qq" | "re" => Ok(Self::Restart),
            "status" | "stat" | "check" | "ch" => Ok(Self::Status),
            "enter" => Ok(Self::Enter),
            "leave" => Ok(Self::Leave),
            "heartbeat" => Ok(Self::Heartbeat),
            "daemon" => Ok(Self::Daemon),
            "tui" | "top" => Ok(Self::Tui),
            "help" | "h" => Ok(Self::Help),
            other => Err(format!("unknown command `{other}`")),
        }
    }
}

fn load_manifest(path: Option<PathBuf>) -> Result<(Manifest, PathBuf), String> {
    let manifest_path = match path {
        Some(path) => path,
        None => env::var_os("IDES_MANIFEST")
            .map(PathBuf::from)
            .ok_or_else(|| "set IDES_MANIFEST or pass --manifest <path>".to_string())?,
    };

    let raw = fs::read_to_string(&manifest_path)
        .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;
    let manifest = Manifest::from_json(&raw)
        .map_err(|err| format!("failed to parse {}: {err}", manifest_path.display()))?;

    Ok((manifest, manifest_path))
}

fn inspect(manifest: &Manifest, manifest_path: &Path, args: &CommandArgs) -> Result<(), String> {
    if args.json {
        print_json(manifest, &args.services)?;
    } else {
        print_text(manifest, manifest_path, &args.services)?;
    }

    Ok(())
}

fn print_json(manifest: &Manifest, services: &[String]) -> Result<(), String> {
    if services.is_empty() {
        println!(
            "{}",
            serde_json::to_string_pretty(manifest)
                .map_err(|err| format!("failed to serialize manifest: {err}"))?
        );
        return Ok(());
    }

    let mut selected = BTreeMap::<&str, &Service>::new();
    for (name, service) in manifest.selected_services(services)? {
        selected.insert(name, service);
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&selected)
            .map_err(|err| format!("failed to serialize services: {err}"))?
    );
    Ok(())
}

fn print_text(
    manifest: &Manifest,
    manifest_path: &Path,
    services: &[String],
) -> Result<(), String> {
    println!("manifest: {}", manifest_path.display());
    println!("set: {} ({})", manifest.name, manifest.set_id);
    println!("auto start: {}", manifest.auto_start);

    let names = if services.is_empty() {
        manifest.services.keys().cloned().collect::<Vec<_>>()
    } else {
        services.to_vec()
    };

    println!("services:");
    for name in names {
        let service = manifest
            .services
            .get(&name)
            .ok_or_else(|| format!("manifest has no service `{name}`"))?;
        println!("- {name}");
        println!("  unit: {}", service.unit_name);
        println!("  command: {}", service.exec.display_command());
        if !service.configs.is_empty() {
            for (config_name, config) in &service.configs {
                println!("  config {config_name}: {}", config.display_path());
            }
        }
        println!("  runtime: {}", service.runtime.paths.base);
        if !service.systemd_args.is_empty() {
            println!("  systemd args: {}", service.systemd_args);
        }
    }

    Ok(())
}

fn print_help() {
    println!(
        "\
ides

Usage:
  ides inspect [--manifest <path>] [--json] [SERVICE...]
  ides run     [--manifest <path>] SERVICE...
  ides stop    [--manifest <path>] SERVICE...
  ides restart [--manifest <path>] SERVICE...
  ides status  [--manifest <path>] [--json] [SERVICE...]
  ides enter   [--manifest <path>] [--kind shell] [--root <path>] [--pid <pid>]
  ides leave   --token <token>
  ides heartbeat --token <token>
  ides daemon  --manifest <path>
  ides tui     [--manifest <path>] [SERVICE...]

Options:
  -m, --manifest <path>  Manifest path. Defaults to IDES_MANIFEST.
      --json             Print JSON instead of a text summary.
      --token <token>    Lease token. Defaults to IDES_LEASE_TOKEN.
  -h, --help             Print help.
"
    );
}
