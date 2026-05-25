{
  pkgs,
  ...
}:
{
  options =
    let
      inherit (pkgs) lib;
      inherit (lib) types mkOption;
      configRef =
        with types;
        submodule {
          options.config = mkOption {
            type = str;
            description = "Name of a generated service config file to splice into this argv position.";
            example = "main";
          };
        };
      runtimeConfig =
        with types;
        submodule {
          options = {
            fileName = mkOption {
              type = nullOr str;
              description = "Relative file name to use under the service runtime config directory. Defaults to the config name plus any configured extension.";
              example = "redis.conf";
              default = null;
            };
            parts = mkOption {
              type = listOf (oneOf [
                str
                path
                attrs
              ]);
              description = "Typed template parts rendered by ides immediately before starting the service.";
              example = [
                "dir "
                { runtimePath = "data"; }
              ];
              default = [ ];
            };
          };
        };
      systemdExecCommand =
        with types;
        submodule {
          options = {
            command = mkOption {
              type = str;
              description = "Command line for a systemd lifecycle hook.";
            };
            ignoreFailure = mkOption {
              type = bool;
              description = "Whether systemd should ignore this hook command's failure.";
              default = false;
            };
          };
        };
      configFile =
        with types;
        submodule {
          options = {
            text = mkOption {
              type = str;
              default = "";
              description = "Plaintext configuration to use.";
              example = ''
                http://*:8080 {
                  respond "hello"
                }
              '';
            };
            ext = mkOption {
              type = str;
              default = "";
              description = "If your service config requires a file extension, set it here. This overrides `format`'s output path'.";
              example = "json";
            };
            file = mkOption {
              type = nullOr path;
              description = "Path to config file. This overrides all other values.";
              example = "./configs/my-config.ini";
              default = null;
            };
            content = mkOption {
              type = nullOr attrs;
              description = "Attributes that define your config values.";
              default = null;
              example = {
                this = "that";
              };
            };
            format = mkOption {
              type = nullOr (enum [
                "java"
                "json"
                "yaml"
                "toml"
                "ini"
                "xml"
                "php"
              ]);
              description = "Config output format.\nOne of:\n`java json yaml toml ini xml php`.";
              example = "json";
              default = null;
            };
            formatter = mkOption {
              type = types.anything;
              description = "Serialisation/writer function to apply to `content`.\n`format` will auto-apply the correct format if the option value is valid.\nShould take `path: attrs:` and return a storepath.";
              example = "pkgs.formats.yaml {}.generate";
              default = null;
            };
            runtime = mkOption {
              type = nullOr runtimeConfig;
              description = "Runtime-rendered config template. Use this when config content must refer to ephemeral ides runtime paths.";
              default = null;
            };
          };
        };
      serviceConfig =
        with types;
        submodule {
          options = {
            pkg = mkOption {
              type = nullOr package;
              description = "Package to use for service.";
              example = "pkgs.caddy";
              default = null;
            };
            exec = mkOption {
              type = str;
              description = "Alternative executable name to use from `pkg`.";
              example = "caddy";
              default = "";
            };
            argv = mkOption {
              type = listOf (oneOf [
                str
                path
                configRef
              ]);
              description = "Structured argv to supply to the service binary. Use `{ config = \"name\"; }` to splice in a generated static or runtime config path.";
              example = [
                "run"
                "-c"
                { config = "main"; }
                "--adapter"
                "caddyfile"
              ];
              default = [ ];
            };
            shellCommand = mkOption {
              type = nullOr str;
              description = "Raw shell command to run for the service. This is an escape hatch for services that do not fit package/executable/argv.";
              example = "\${pkgs.lib.getExe pkgs.caddy} run -c \${pkgs.writeText \"Caddyfile\" cfg.extraConfig} --adapter caddyfile";
              default = null;
            };
            configs = mkOption {
              type = attrsOf configFile;
              description = "Named generated static or runtime config files that can be referenced from `argv` with `{ config = \"name\"; }`.";
              default = { };
            };
            runtime = mkOption {
              description = "Ephemeral runtime defaults for the service.";
              default = { };
              type = submodule {
                options = {
                  ephemeral = mkOption {
                    type = bool;
                    description = "Place default writable paths under the ides runtime directory and clean them on explicit stop.";
                    default = true;
                  };
                  env = mkOption {
                    type = attrsOf str;
                    description = "Additional environment variables to inject into the transient service.";
                    default = { };
                  };
                };
              };
            };
            dependencies = mkOption {
              description = "Service dependency and ordering metadata handled by the ides runtime.";
              default = { };
              type = submodule {
                options = {
                  requires = mkOption {
                    type = listOf str;
                    description = "Services that must be started before this service. Missing services are treated as configuration errors.";
                    example = [ "postgres" ];
                    default = [ ];
                  };
                  wants = mkOption {
                    type = listOf str;
                    description = "Services that should be started before this service. In ides this is currently strict like `requires`, because all referenced services must exist in the manifest.";
                    example = [ "redis" ];
                    default = [ ];
                  };
                  after = mkOption {
                    type = listOf str;
                    description = "Services that must be ordered before this service when both are started or stopped together.";
                    example = [ "postgres" ];
                    default = [ ];
                  };
                  before = mkOption {
                    type = listOf str;
                    description = "Services that must be ordered after this service when both are started or stopped together.";
                    example = [ "worker" ];
                    default = [ ];
                  };
                  partOf = mkOption {
                    type = listOf str;
                    description = "Services whose stop or restart operations should also stop or restart this service.";
                    example = [ "app" ];
                    default = [ ];
                  };
                };
              };
            };
            socket = mkOption {
              type = attrsOf (listOf str);
              description = "List of socket options for the unit (see `man systemd.socket`) - supplied as a list due to some options allowing duplicates.";
              example = {
                ListenStream = [ "/run/user/1000/myapp.sock" ];
              };
              default = { };
            };
            path = mkOption {
              type = attrsOf (listOf str);
              description = "List of path options for the unit (see `man systemd.path`) - supplied as a list due to some options allowing duplicates.";
              example = {
                PathModified = [ "/some/path" ];
              };
              default = { };
            };
            timer = mkOption {
              type = attrsOf (listOf str);
              description = "List of timer options for the unit (see `man systemd.timer`) - supplied as a list due to some options allowing duplicates.";
              example = {
                OnActiveSec = [ "50s" ];
              };
              default = { };
            };
            systemd = mkOption {
              description = "Small set of systemd service properties that ides can preserve when starting transient units.";
              default = { };
              type = submodule {
                options = {
                  serviceType = mkOption {
                    type = nullOr str;
                    description = "systemd service Type to pass to the transient unit, such as `simple`, `exec`, `oneshot`, or `notify`.";
                    default = null;
                  };
                  notifyAccess = mkOption {
                    type = nullOr str;
                    description = "systemd NotifyAccess value for notify-style services. Defaults to `all` for notify services when omitted.";
                    default = null;
                  };
                  restart = mkOption {
                    type = nullOr str;
                    description = "systemd Restart policy to pass to the transient unit.";
                    default = null;
                  };
                  restartSec = mkOption {
                    type = nullOr str;
                    description = "systemd RestartSec duration to pass to the transient unit.";
                    default = null;
                  };
                  timeoutSec = mkOption {
                    type = nullOr str;
                    description = "systemd TimeoutSec duration to apply to transient service start and stop timeouts.";
                    default = null;
                  };
                  timeoutStartSec = mkOption {
                    type = nullOr str;
                    description = "systemd TimeoutStartSec duration to pass to the transient unit.";
                    default = null;
                  };
                  timeoutStopSec = mkOption {
                    type = nullOr str;
                    description = "systemd TimeoutStopSec duration to pass to the transient unit.";
                    default = null;
                  };
                  watchdogSec = mkOption {
                    type = nullOr str;
                    description = "systemd WatchdogSec duration to pass to the transient unit.";
                    default = null;
                  };
                  workingDirectory = mkOption {
                    type = nullOr str;
                    description = "Working directory to pass to the transient unit. Defaults to the ides runtime data directory.";
                    default = null;
                  };
                  killMode = mkOption {
                    type = nullOr str;
                    description = "systemd KillMode value to pass to the transient unit.";
                    default = null;
                  };
                  remainAfterExit = mkOption {
                    type = nullOr bool;
                    description = "systemd RemainAfterExit value to pass to the transient unit.";
                    default = null;
                  };
                  execStartPre = mkOption {
                    type = listOf (oneOf [
                      str
                      systemdExecCommand
                    ]);
                    description = "Commands to run as systemd ExecStartPre entries before the main service command.";
                    default = [ ];
                  };
                  execStartPost = mkOption {
                    type = listOf (oneOf [
                      str
                      systemdExecCommand
                    ]);
                    description = "Commands to run as systemd ExecStartPost entries after the main service command starts.";
                    default = [ ];
                  };
                  execReload = mkOption {
                    type = listOf (oneOf [
                      str
                      systemdExecCommand
                    ]);
                    description = "Commands to run as systemd ExecReload entries.";
                    default = [ ];
                  };
                  execStop = mkOption {
                    type = listOf (oneOf [
                      str
                      systemdExecCommand
                    ]);
                    description = "Commands to run as systemd ExecStop entries when the transient unit stops.";
                    default = [ ];
                  };
                  ignoreStartFailure = mkOption {
                    type = bool;
                    description = "Whether systemd should ignore the main ExecStart command's failure.";
                    default = false;
                  };
                };
              };
            };
          };
        };
    in
    {
      serviceDefs = mkOption {
        type = types.attrsOf serviceConfig;
        description = "Concrete service definitions, as per submodule options.\nPlease put service-related options into `options.services` instead, and use this to implement those options.";
      };

      auto = mkOption {
        type = types.bool;
        description = "Whether to autostart ides services at devshell instantiation.";
        default = true;
      };

      monitor = mkOption {
        type = types.either types.bool types.int;
        description = "Enable shell lease hooks that stop and clean services when the last shell lease exits. Integer values are accepted for the old monitor API but currently behave like `true`.";
        default = true;
      };

      # to prevent generating docs for this option; see https://github.com/NixOS/nixpkgs/issues/293510
      _module.args = mkOption {
        internal = true;
      };

      # for internal use
      _buildIdes = mkOption {
        type = types.attrs;
        internal = true;
      };
    };

}
