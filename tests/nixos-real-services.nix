{ pkgs ? import <nixpkgs> { } }:

let
  mkIdes = import ../default.nix {
    inherit pkgs;
    shell = args: args;
    auto = false;
  };
  nixosServices = import ../lib/nixos-services.nix {
    inherit (pkgs) lib;
  };

  mkNixos =
    configuration:
    import <nixpkgs/nixos> {
      configuration = {
        system.stateVersion = "26.05";
      } // configuration;
    };

  redisNixos = mkNixos {
    services.redis.servers.main.enable = true;
  };

  nginxNixos = mkNixos {
    services.nginx.enable = true;
  };

  nodeExporterNixos = mkNixos {
    services.prometheus.exporters.node.enable = true;
  };

  sshNixos = mkNixos {
    services.openssh.enable = true;
  };

  postgresqlNixos = mkNixos {
    services.postgresql.enable = true;
  };

  mysqlNixos = mkNixos {
    services.mysql.enable = true;
    services.mysql.package = pkgs.mariadb;
  };

  memcachedNixos = mkNixos {
    services.memcached.enable = true;
  };

  caddyNixos = mkNixos {
    services.caddy.enable = true;
  };

  prometheusNixos = mkNixos {
    services.prometheus.enable = true;
  };

  grafanaNixos = mkNixos {
    services.grafana.enable = true;
  };

  rabbitmqNixos = mkNixos {
    services.rabbitmq.enable = true;
  };

  mosquittoNixos = mkNixos {
    services.mosquitto.enable = true;
    services.mosquitto.listeners = [
      {
        users.test = {
          acl = [ "readwrite #" ];
          hashedPassword = "$7$101$dummysalt$dummyhash";
        };
      }
    ];
  };

  unboundNixos = mkNixos {
    services.unbound.enable = true;
  };

  haproxyNixos = mkNixos {
    services.haproxy.enable = true;
    services.haproxy.config = ''
      global
        daemon
      defaults
        mode http
    '';
  };

  traefikNixos = mkNixos {
    services.traefik.enable = true;
  };

  transmissionNixos = mkNixos {
    services.transmission.enable = true;
  };

  giteaNixos = mkNixos {
    services.gitea.enable = true;
    services.gitea.settings.server.DOMAIN = "localhost";
    services.gitea.settings.server.ROOT_URL = "http://localhost/";
  };

  staticWebServerNixos = mkNixos {
    services.static-web-server.enable = true;
    services.static-web-server.root = "/tmp";
  };

  syncthingNixos = mkNixos {
    services.syncthing.enable = true;
    services.syncthing.user = "alice";
  };

  zigbee2mqttNixos = mkNixos {
    services.zigbee2mqtt.enable = true;
    services.zigbee2mqtt.settings.serial.port = "/dev/null";
  };

  dnsmasqNixos = mkNixos {
    services.dnsmasq.enable = true;
  };

  bindNixos = mkNixos {
    services.bind.enable = true;
  };

  dnsmasqLowering = builtins.tryEval (
    builtins.deepSeq (nixosServices.fromNixosConfig [ "dnsmasq" ] dnsmasqNixos.config) true
  );

  bindLowering = builtins.tryEval (
    builtins.deepSeq (nixosServices.fromNixosConfig [ "bind" ] bindNixos.config) true
  );

  idesShell = mkIdes {
    systemd.services =
      (nixosServices.fromNixosConfig [ "redis-main" ] redisNixos.config)
      // (nixosServices.fromNixosConfig [ "nginx" ] nginxNixos.config)
      // (nixosServices.fromNixosConfig [ "prometheus-node-exporter" ] nodeExporterNixos.config)
      // (nixosServices.fromNixosConfig [
        "sshd-keygen"
        "sshd"
      ] sshNixos.config)
      // (nixosServices.fromNixosConfig [
        "postgresql-setup"
        "postgresql"
      ] postgresqlNixos.config)
      // (nixosServices.fromNixosConfig [ "mysql" ] mysqlNixos.config)
      // (nixosServices.fromNixosConfig [ "memcached" ] memcachedNixos.config)
      // (nixosServices.fromNixosConfig [ "caddy" ] caddyNixos.config)
      // (nixosServices.fromNixosConfig [ "prometheus" ] prometheusNixos.config)
      // (nixosServices.fromNixosConfig [ "grafana" ] grafanaNixos.config)
      // (nixosServices.fromNixosConfig [
        "epmd"
        "rabbitmq"
      ] rabbitmqNixos.config)
      // (nixosServices.fromNixosConfig [ "mosquitto" ] mosquittoNixos.config)
      // (nixosServices.fromNixosConfig [ "unbound" ] unboundNixos.config)
      // (nixosServices.fromNixosConfig [ "haproxy" ] haproxyNixos.config)
      // (nixosServices.fromNixosConfig [ "traefik" ] traefikNixos.config)
      // (nixosServices.fromNixosConfig [
        "transmission-setup"
        "transmission"
      ] transmissionNixos.config)
      // (nixosServices.fromNixosConfig [ "gitea" ] giteaNixos.config)
      // (nixosServices.fromNixosConfig [ "static-web-server" ] staticWebServerNixos.config)
      // (nixosServices.fromNixosConfig [ "syncthing" ] syncthingNixos.config)
      // (nixosServices.fromNixosConfig [ "zigbee2mqtt" ] zigbee2mqttNixos.config);
  };
in
assert dnsmasqLowering.success == false;
assert bindLowering.success == false;
pkgs.runCommand "ides-real-nixos-services-check" { } ''
  ${pkgs.jq}/bin/jq -e '.services["redis-main"].exec.shellCommand | contains("redis-server")' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services["redis-main"].systemd.serviceType == "notify"' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services["redis-main"].systemd.execStartPre[0].command | contains("redis-main-prep-conf")' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.nginx.exec.shellCommand | contains("nginx")' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.nginx.systemd.restart == "always"' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.nginx.systemd.restartSec == "10s"' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.nginx.systemd.execReload[0].command | contains("nginx")' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services["prometheus-node-exporter"].exec.shellCommand | contains("node_exporter")' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services["prometheus-node-exporter"].systemd.workingDirectory == "/tmp"' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.sshd.dependencies.wants == ["sshd-keygen"]' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.sshd.dependencies.after == ["sshd-keygen"]' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services["postgresql-setup"].systemd.remainAfterExit == true' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.postgresql.systemd.timeoutSec != null' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.mysql.systemd.execStartPost[0].command | contains("mysql")' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.caddy.exec.shellCommand | contains("caddy")' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services["static-web-server"].exec.shellCommand | contains("static-web-server")' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.rabbitmq.systemd.execStop[0].command | contains("rabbitmqctl")' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.rabbitmq.systemd.timeoutStartSec != null' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.mosquitto.systemd.notifyAccess == "main"' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.unbound.systemd.serviceType == "notify"' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.haproxy.systemd.serviceType == "notify"' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.traefik.systemd.execStartPre == []' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.transmission.systemd.serviceType == "notify-reload"' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.gitea.systemd.watchdogSec != null' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.syncthing.exec.shellCommand | contains("syncthing")' ${idesShell.IDES_MANIFEST} >/dev/null
  ${pkgs.jq}/bin/jq -e '.services.zigbee2mqtt.systemd.workingDirectory != null' ${idesShell.IDES_MANIFEST} >/dev/null
  touch $out
''
