{
  config,
  pkgs,
  lib,
  ...
}:
{
  # create some options
  options.services.redis =
    let
      inherit (lib) mkOption types;
    in
    {
      enable = lib.mkEnableOption "Enable Redis.";

      bind = mkOption {
        type = with types; listOf str;
        description = "List of IPs to bind to.";
        default = [
          "127.0.0.1"
          "::1"
        ];
      };

      port = mkOption {
        type = types.ints.between 1024 65535;
        description = "Port to bind to.";
        default = 6379;
      };

      socket = mkOption {
        type = with types; nullOr str;
        description = "Unix socket to bind to. Relative paths are placed under the service runtime run directory; absolute paths are used as-is.";
        default = null;
      };

      socketPerms = mkOption {
        type = with types; nullOr int;
        description = "Permissions for the unix socket.";
        default = null;
      };

      logLevel = mkOption {
        type = types.enum [
          "debug"
          "verbose"
          "notice"
          "warning"
          "nothing"
        ];
        description = "Logging verbosity level.";
        default = "notice";
      };

      databases = mkOption {
        type = types.int;
        description = "Number of databases.";
        default = 16;
      };

      # escape hatch due to redis config being massive
      extraConfig = mkOption {
        type = types.str;
        description = "Additional config directives.";
        default = "";
      };

      name = mkOption {
        type = types.str;
        description = "The name ides uses for this service.";
        default = "redis";
      };
    };

  config.serviceDefs =
    let
      cfg = config.services.redis;
    in
    lib.mkIf cfg.enable {
      # use a customisable name in case the user needs several instances
      "${cfg.name}" = {
        pkg = pkgs.redis;
        # make sure we get the server binary, not cli
        exec = "redis-server";
        argv = [ { config = "main"; } ];
        configs.main.runtime = {
          fileName = "redis.conf";
          parts =
            [
              "bind ${lib.concatStringsSep " " cfg.bind}\n"
              "port ${toString cfg.port}\n"
              "databases ${toString cfg.databases}\n"
              "loglevel ${cfg.logLevel}\n"
              "dir "
              { runtimePath = "data"; }
              "\n"
              "pidfile "
              { runtimePath = "run"; }
              "/redis.pid\n"
            ]
            ++ lib.optionals (cfg.socket != null) (
              if lib.hasPrefix "/" cfg.socket then
                [
                  "unixsocket ${cfg.socket}\n"
                ]
              else
                [
                  "unixsocket "
                  { runtimePath = "run"; }
                  "/${cfg.socket}\n"
                ]
            )
            ++ lib.optionals (cfg.socket != null && cfg.socketPerms != null) [
              "unixsocketperm ${toString cfg.socketPerms}\n"
            ]
            ++ lib.optionals (cfg.extraConfig != "") [
              cfg.extraConfig
            ];
        };
      };
    };
}
