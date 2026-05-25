{
  config,
  lib,
  pkgs,
  ...
}:
let
  inherit (lib)
    attrNames
    filter
    filterAttrs
    hasSuffix
    head
    length
    mapAttrs
    mkIf
    mkOption
    optionalAttrs
    removeSuffix
    types
    ;

  envValue = types.oneOf [
    types.str
    types.path
    types.int
    types.bool
  ];

  compatService =
    { ... }:
    {
      options = {
        enable = mkOption {
          type = types.bool;
          default = true;
          description = "Whether to lower this NixOS-style systemd service into an ides service definition.";
        };

        description = mkOption {
          type = types.str;
          default = "";
          description = "Service description. Kept for NixOS compatibility; currently not emitted into the ides manifest.";
        };

        script = mkOption {
          type = types.lines;
          default = "";
          description = "Shell script to run as the service command.";
        };

        preStart = mkOption {
          type = types.lines;
          default = "";
          description = "Shell script to run before the service command.";
        };

        path = mkOption {
          type = types.listOf (types.oneOf [
            types.package
            types.path
            types.str
          ]);
          default = [ ];
          description = "Packages or paths added to PATH for script-style services.";
        };

        environment = mkOption {
          type = types.attrsOf envValue;
          default = { };
          description = "Environment variables injected into the lowered ides service.";
        };

        serviceConfig = mkOption {
          type = types.attrsOf types.anything;
          default = { };
          description = "Small supported subset of NixOS serviceConfig. ides currently accepts command lifecycle hooks, Type, NotifyAccess, Restart, common timeout fields, RemainAfterExit, KillMode, and WorkingDirectory.";
        };

        wantedBy = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "Accepted for NixOS compatibility but ignored by ides.";
        };

        requiredBy = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "Accepted for NixOS compatibility but ignored by ides.";
        };

        wants = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "NixOS-style service wants. Local service references are lowered into ides dependencies.";
        };

        requires = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "NixOS-style service requirements. Local service references are lowered into ides dependencies.";
        };

        after = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "NixOS-style ordering constraints. Local service references are lowered into ides dependencies.";
        };

        before = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "NixOS-style reverse ordering constraints. Local service references are lowered into ides dependencies.";
        };

        partOf = mkOption {
          type = types.listOf types.str;
          default = [ ];
          description = "NixOS-style PartOf constraints. Local service references are lowered into ides dependencies.";
        };
      };
    };

  enabledSystemdServices = filterAttrs (_: service: service.enable) config.systemd.services;
  localSystemdServiceNames = attrNames enabledSystemdServices;

  normalizeLocalServiceRef =
    ref:
    let
      serviceName = removeSuffix ".service" ref;
      unsupportedUnit =
        hasSuffix ".target" ref
        || hasSuffix ".socket" ref
        || hasSuffix ".path" ref
        || hasSuffix ".timer" ref
        || hasSuffix ".mount" ref
        || hasSuffix ".slice" ref;
    in
    if unsupportedUnit then null else if builtins.elem serviceName localSystemdServiceNames then serviceName else null;

  lowerDeps =
    refs:
    filter (value: value != null) (map normalizeLocalServiceRef refs);

  envToString =
    value:
    if builtins.isBool value then
      if value then "true" else "false"
    else
      builtins.toString value;

  pathEnv =
    service:
    let
      joined = lib.makeBinPath service.path;
      envPath = service.environment.PATH or "";
    in
    if joined == "" then envPath else if envPath == "" then joined else "${joined}:${envPath}";

  lowerEnvironment =
    service:
    (mapAttrs (_: envToString) service.environment)
    // optionalAttrs (pathEnv service != "") {
      PATH = pathEnv service;
    };

  singletonExecStart =
    name: value:
    let
      commands = execCommands value;
    in
    if length commands == 1 then
      head commands
    else if length commands == 0 then
      throw "systemd.services.${name}.serviceConfig.ExecStart must contain a command to lower into ides"
    else
      throw "systemd.services.${name}.serviceConfig.ExecStart with multiple commands is not supported by ides yet";

  listExec =
    value:
    if value == null then [ ] else if builtins.isList value then value else [ value ];

  parseSystemdExec =
    value:
    let
      raw = builtins.toString value;
      strip =
        ignoreFailure: text:
        if lib.hasPrefix "!!" text then
          strip ignoreFailure (lib.removePrefix "!!" text)
        else if lib.hasPrefix "+" text then
          strip ignoreFailure (lib.removePrefix "+" text)
        else if lib.hasPrefix "!" text then
          strip ignoreFailure (lib.removePrefix "!" text)
        else if lib.hasPrefix "-" text then
          strip true (lib.removePrefix "-" text)
        else if lib.hasPrefix "@" text then
          strip ignoreFailure (lib.removePrefix "@" text)
        else if lib.hasPrefix ":" text then
          strip ignoreFailure (lib.removePrefix ":" text)
        else
          {
            command = text;
            inherit ignoreFailure;
          };
    in
    strip false raw;

  execCommands =
    value:
    filter (command: command.command != "") (map parseSystemdExec (listExec value));

  boolValue =
    optionName: value:
    if builtins.isBool value then
      value
    else
      let
        normalized = lib.toLower (builtins.toString value);
      in
      if builtins.elem normalized [
        "1"
        "true"
        "yes"
        "on"
      ] then
        true
      else if builtins.elem normalized [
        "0"
        "false"
        "no"
        "off"
      ] then
        false
      else
        throw "systemd.services.*.serviceConfig.${optionName} must be a boolean-compatible value";

  stringValue = value: if value == null then null else builtins.toString value;

  supportedServiceConfigKeys = [
    "ExecReload"
    "ExecStart"
    "ExecStartPost"
    "ExecStartPre"
    "ExecStop"
    "KillMode"
    "NotifyAccess"
    "RemainAfterExit"
    "Restart"
    "RestartSec"
    "TimeoutSec"
    "TimeoutStartSec"
    "TimeoutStopSec"
    "Type"
    "WatchdogSec"
    "WorkingDirectory"
  ];

  supportedServiceTypes = [
    "simple"
    "exec"
    "oneshot"
    "notify"
    "notify-reload"
  ];

  unsupportedServiceConfigKeys =
    service:
    filter (key: !(builtins.elem key supportedServiceConfigKeys)) (attrNames service.serviceConfig);

  commandFor =
    name: service:
    let
      hasScript = service.script != "";
      hasExecStart = service.serviceConfig ? ExecStart;
      unsupported = unsupportedServiceConfigKeys service;
      serviceType = service.serviceConfig.Type or "simple";
    in
    if unsupported != [ ] then
      throw "systemd.services.${name}.serviceConfig uses unsupported ides compatibility keys: ${builtins.concatStringsSep ", " unsupported}"
    else if !(builtins.elem serviceType supportedServiceTypes) then
      throw "systemd.services.${name}.serviceConfig.Type=${serviceType} is not supported by ides compatibility yet"
    else if hasScript && hasExecStart then
      throw "systemd.services.${name} must set either `script` or `serviceConfig.ExecStart`, not both"
    else if hasExecStart then
      (singletonExecStart name service.serviceConfig.ExecStart).command
    else if hasScript then
      service.script
    else
      throw "systemd.services.${name} must set `script` or `serviceConfig.ExecStart` to lower into ides";

  preStartCommands =
    service:
    let
      explicitPreStart =
        if service.preStart != "" then
          [
            {
              command = service.preStart;
              ignoreFailure = false;
            }
          ]
        else
          [ ];
      execStartPre =
        if service.serviceConfig ? ExecStartPre then
          execCommands service.serviceConfig.ExecStartPre
        else
          [ ];
    in
    explicitPreStart ++ execStartPre;

  lowerService =
    name: service:
    let
      execStart =
        if service.serviceConfig ? ExecStart then
          singletonExecStart name service.serviceConfig.ExecStart
        else
          null;
      remainAfterExit =
        if service.serviceConfig ? RemainAfterExit then
          boolValue "RemainAfterExit" service.serviceConfig.RemainAfterExit
        else
          null;
    in
    {
      shellCommand = commandFor name service;
      runtime.env = lowerEnvironment service;
      systemd = {
        serviceType = service.serviceConfig.Type or null;
        notifyAccess = service.serviceConfig.NotifyAccess or null;
        restart = service.serviceConfig.Restart or null;
        restartSec = stringValue (service.serviceConfig.RestartSec or null);
        timeoutSec = stringValue (service.serviceConfig.TimeoutSec or null);
        timeoutStartSec = stringValue (service.serviceConfig.TimeoutStartSec or null);
        timeoutStopSec = stringValue (service.serviceConfig.TimeoutStopSec or null);
        watchdogSec = stringValue (service.serviceConfig.WatchdogSec or null);
        workingDirectory = service.serviceConfig.WorkingDirectory or null;
        killMode = service.serviceConfig.KillMode or null;
        inherit remainAfterExit;
        ignoreStartFailure = if execStart == null then false else execStart.ignoreFailure;
        execStartPre = preStartCommands service;
        execStartPost = execCommands (service.serviceConfig.ExecStartPost or null);
        execReload = execCommands (service.serviceConfig.ExecReload or null);
        execStop = execCommands (service.serviceConfig.ExecStop or null);
      };
      dependencies = {
        requires = lowerDeps service.requires;
        wants = lowerDeps service.wants;
        after = lowerDeps service.after;
        before = lowerDeps service.before;
        partOf = lowerDeps service.partOf;
      };
    };
in
{
  options.systemd.services = mkOption {
    type = types.attrsOf (types.submodule compatService);
    default = { };
    description = ''
      Compatibility subset of NixOS `systemd.services`.

      ides lowers simple foreground services into `serviceDefs`. Supported
      fields are `script`, single-command `serviceConfig.ExecStart`, `path`,
      `environment`, a small transient-safe `serviceConfig` subset, and local
      service dependency lists. Target/unit installation fields such as
      `wantedBy` are accepted but ignored.
    '';
  };

  config.serviceDefs = mkIf (enabledSystemdServices != { }) (
    mapAttrs lowerService enabledSystemdServices
  );
}
