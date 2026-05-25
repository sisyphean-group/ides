{ pkgs ? import <nixpkgs> { } }:

let
  mkIdes = import ../default.nix {
    inherit pkgs;
    shell = args: args;
    auto = false;
  };

  configless = mkIdes {
    serviceDefs.hello.pkg = pkgs.hello;
  };

  textConfig = mkIdes {
    serviceDefs.caddy = {
      pkg = pkgs.caddy;
      argv = [
        "run"
        "-c"
        { config = "main"; }
        "--adapter"
        "caddyfile"
      ];
      configs.main.text = ''
        http://127.0.0.1:8888 {
          respond "hello"
        }
      '';
    };
  };

  redis = mkIdes {
    services.redis.enable = true;
  };

  shellCommand = mkIdes {
    serviceDefs.raw.shellCommand = "${pkgs.lib.getExe pkgs.hello} --greeting hi";
  };

  dependencyShell = mkIdes {
    serviceDefs.db.shellCommand = "${pkgs.coreutils}/bin/true";
    serviceDefs.app = {
      shellCommand = "${pkgs.coreutils}/bin/true";
      dependencies.requires = [ "db" ];
      dependencies.after = [ "db" ];
    };
  };

  activationShell = mkIdes {
    serviceDefs.sock = {
      pkg = pkgs.coreutils;
      exec = "cat";
      socket.ListenStream = [ "/tmp/ides-test.sock" ];
    };
    serviceDefs.pathsvc = {
      pkg = pkgs.coreutils;
      exec = "true";
      path.PathExists = [ "/tmp/ides-trigger" ];
    };
    serviceDefs.timersvc = {
      pkg = pkgs.coreutils;
      exec = "true";
      timer.OnActiveSec = [ "1s" ];
    };
  };

  runtimeConfig = mkIdes {
    serviceDefs.rt = {
      pkg = pkgs.coreutils;
      exec = "cat";
      argv = [ { config = "main"; } ];
      runtime.env.IDES_TEST_NAME = "runtime";
      configs.main.runtime = {
        fileName = "runtime.conf";
        parts = [
          "dir="
          { runtimePath = "data"; }
          "\nname="
          { env = "IDES_TEST_NAME"; }
          "\n"
        ];
      };
    };
  };

  nixosCompat = mkIdes {
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

  hasManifest = shell:
    shell ? IDES_MANIFEST && builtins.match ".*IDES_MANIFEST=.*" shell.shellHook != null;
in
assert hasManifest configless;
assert hasManifest textConfig;
assert hasManifest redis;
assert hasManifest shellCommand;
assert hasManifest dependencyShell;
assert hasManifest activationShell;
assert hasManifest runtimeConfig;
assert hasManifest nixosCompat;
pkgs.runCommand "ides-eval-check" { } ''
  ${pkgs.jq}/bin/jq -e '.services.app.dependencies.requires == ["db"]' ${dependencyShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.dependencies.after == ["db"]' ${dependencyShell.IDES_MANIFEST} >/dev/null

  ${pkgs.jq}/bin/jq -e '.services.db.exec.shellCommand == "echo db"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.exec.shellCommand == "${pkgs.coreutils}/bin/true"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.runtime.env.FOO == "bar"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.runtime.env.PATH | contains("coreutils")' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.serviceType == "notify"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.restart == "always"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.restartSec == "3"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.timeoutSec == "5s"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.timeoutStartSec == "7s"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.timeoutStopSec == "11s"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.watchdogSec == "13s"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.workingDirectory == "/tmp"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.killMode == "mixed"' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.remainAfterExit == true' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.ignoreStartFailure == false' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.execStartPre == [{"command":"${pkgs.coreutils}/bin/true","ignoreFailure":false}]' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.execStartPost == [{"command":"${pkgs.coreutils}/bin/true","ignoreFailure":true}]' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.execReload == [{"command":"${pkgs.coreutils}/bin/true","ignoreFailure":false}]' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.systemd.execStop == [{"command":"${pkgs.coreutils}/bin/true","ignoreFailure":false}]' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.dependencies.requires == ["db"]' ${nixosCompat.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.app.dependencies.after == ["db"]' ${nixosCompat.IDES_MANIFEST} >/dev/null

  ${pkgs.jq}/bin/jq -e '.services.sock.activation.socket.ListenStream == ["/tmp/ides-test.sock"]' ${activationShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.pathsvc.activation.path.PathExists == ["/tmp/ides-trigger"]' ${activationShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.timersvc.activation.timer.OnActiveSec == ["1s"]' ${activationShell.IDES_MANIFEST} >/dev/null

  ${pkgs.jq}/bin/jq -e '.services.caddy.exec.argv == ["run","-c",{"config":"main"},"--adapter","caddyfile"]' ${textConfig.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.caddy.configs.main.runtime == null' ${textConfig.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.caddy.configs.main.path | type == "string"' ${textConfig.IDES_MANIFEST} >/dev/null

  ${pkgs.jq}/bin/jq -e '.services.rt.exec.argv == [{"config":"main"}]' ${runtimeConfig.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.rt.configs.main.path == null' ${runtimeConfig.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.rt.configs.main.runtime.fileName == "runtime.conf"' ${runtimeConfig.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.rt.configs.main.runtime.parts == ["dir=",{"runtimePath":"data"},"\nname=",{"env":"IDES_TEST_NAME"},"\n"]' ${runtimeConfig.IDES_MANIFEST} >/dev/null

  ${pkgs.jq}/bin/jq -e '.services.redis.configs.main.path == null' ${redis.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.redis.configs.main.runtime.fileName == "redis.conf"' ${redis.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.redis.configs.main.runtime.parts == ["bind 127.0.0.1 ::1\n","port 6379\n","databases 16\n","loglevel notice\n","dir ",{"runtimePath":"data"},"\n","pidfile ",{"runtimePath":"run"},"/redis.pid\n"]' ${redis.IDES_MANIFEST} >/dev/null

  touch $out
''
