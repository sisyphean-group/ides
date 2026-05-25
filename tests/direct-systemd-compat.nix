{ pkgs ? import <nixpkgs> { } }:

let
  mkIdes = import ../default.nix {
    inherit pkgs;
    shell = args: args;
    auto = false;
  };

  compat = mkIdes {
    systemd.services.db.script = "echo db";
    systemd.services.app = {
      path = [ pkgs.coreutils ];
      environment.FOO = "bar";
      serviceConfig = {
        ExecStart = [
          ""
          "${pkgs.coreutils}/bin/true"
        ];
        ExecStartPre = [
          ""
          "+${pkgs.coreutils}/bin/true"
        ];
        ExecStartPost = "-${pkgs.coreutils}/bin/true";
        ExecReload = "${pkgs.coreutils}/bin/true";
        ExecStop = [ "+${pkgs.coreutils}/bin/true" ];
        KillMode = "mixed";
        RemainAfterExit = "yes";
        Restart = "always";
        RestartSec = 3;
        TimeoutSec = "5s";
        TimeoutStartSec = "7s";
        TimeoutStopSec = "11s";
        Type = "notify";
        WatchdogSec = "13s";
        WorkingDirectory = "/tmp";
      };
      requires = [
        "db.service"
        "network-online.target"
      ];
      after = [ "db.service" ];
      wantedBy = [ "multi-user.target" ];
    };
  };

  monitorInt = mkIdes {
    monitor = 5;
    serviceDefs.noop.shellCommand = "${pkgs.coreutils}/bin/true";
  };

  monitorFalse = mkIdes {
    monitor = false;
    serviceDefs.noop.shellCommand = "${pkgs.coreutils}/bin/true";
  };
in
assert !(monitorInt ? monitor);
assert builtins.match ".*ides enter.*" monitorInt.shellHook != null;
assert builtins.match ".*ides enter.*" monitorFalse.shellHook == null;
pkgs.runCommand "ides-direct-systemd-compat-check" { } ''
  ${pkgs.jq}/bin/jq -e '.services.app.exec.shellCommand == "${pkgs.coreutils}/bin/true"' ${compat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.ignoreStartFailure == false' ${compat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.execStartPre == [{"command":"${pkgs.coreutils}/bin/true","ignoreFailure":false}]' ${compat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.execStartPost == [{"command":"${pkgs.coreutils}/bin/true","ignoreFailure":true}]' ${compat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.execReload == [{"command":"${pkgs.coreutils}/bin/true","ignoreFailure":false}]' ${compat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.execStop == [{"command":"${pkgs.coreutils}/bin/true","ignoreFailure":false}]' ${compat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.remainAfterExit == true' ${compat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.restartSec == "3"' ${compat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.timeoutStartSec == "7s"' ${compat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.dependencies.requires == ["db"]' ${compat.IDES_MANIFEST} >/dev/null
  touch $out
''
