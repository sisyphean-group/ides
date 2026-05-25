{ lib }:
let
  inherit (lib)
    filterAttrs
    genAttrs
    hasPrefix
    mapAttrs
    optionalAttrs
    removePrefix
    ;

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

  supportedTypes = [
    "simple"
    "exec"
    "oneshot"
    "notify"
    "notify-reload"
  ];

  toList =
    value:
    if value == null then
      [ ]
    else if builtins.isList value then
      value
    else
      [ value ];

  stripSystemdExecPrefix =
    value:
    let
      raw = builtins.toString value;
      strip =
        text:
        if hasPrefix "!!" text then
          strip (removePrefix "!!" text)
        else if hasPrefix "+" text then
          strip (removePrefix "+" text)
        else if hasPrefix "!" text then
          strip (removePrefix "!" text)
        else if hasPrefix "-" text then
          strip (removePrefix "-" text)
        else if hasPrefix "@" text then
          strip (removePrefix "@" text)
        else if hasPrefix ":" text then
          strip (removePrefix ":" text)
        else
          text;
    in
    strip raw;

  commandValue =
    value:
    {
      raw = builtins.toString value;
      normalized = stripSystemdExecPrefix value;
    };

  execCommands =
    value:
    builtins.filter (command: command.normalized != "") (map commandValue (toList value));

  rawExecCommands = value: map (command: command.raw) (execCommands value);

  singletonExec =
    field: value:
    let
      commands = execCommands value;
    in
    if builtins.length commands == 1 then
      (builtins.head commands).raw
    else if builtins.length commands == 0 then
      null
    else
      throw "ides nixosServices.sanitizeService cannot lower multiple ${field} commands yet";

  boolValue =
    field: value:
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
        throw "ides nixosServices.sanitizeService cannot lower ${field}=${builtins.toString value} as a boolean";

  stringValue = value: if value == null then null else builtins.toString value;

  maybeAttr =
    name: value:
    optionalAttrs (value != null) {
      ${name} = value;
    };

  maybeListAttr =
    name: values:
    optionalAttrs (values != [ ]) {
      ${name} = values;
    };

  sanitizeService =
    service:
    let
      serviceConfig = service.serviceConfig or { };
      serviceType = serviceConfig.Type or null;
      keepType = serviceType != null && builtins.elem serviceType supportedTypes;
      useScript = (service.script or "") != "";
      execStart =
        if useScript || !(serviceConfig ? ExecStart) then
          null
        else
          singletonExec "ExecStart" serviceConfig.ExecStart;
      execStartPre = rawExecCommands (serviceConfig.ExecStartPre or null);
      execStartPost = rawExecCommands (serviceConfig.ExecStartPost or null);
      execReload = rawExecCommands (serviceConfig.ExecReload or null);
      execStop = rawExecCommands (serviceConfig.ExecStop or null);
      serviceConfigSubset =
        maybeAttr "ExecStart" execStart
        // maybeListAttr "ExecReload" execReload
        // maybeListAttr "ExecStartPost" execStartPost
        // maybeListAttr "ExecStartPre" execStartPre
        // maybeListAttr "ExecStop" execStop
        // maybeAttr "KillMode" (serviceConfig.KillMode or null)
        // maybeAttr "NotifyAccess" (serviceConfig.NotifyAccess or null)
        // maybeAttr "RemainAfterExit" (
          if serviceConfig ? RemainAfterExit then boolValue "RemainAfterExit" serviceConfig.RemainAfterExit else null
        )
        // maybeAttr "Restart" (serviceConfig.Restart or null)
        // maybeAttr "RestartSec" (stringValue (serviceConfig.RestartSec or null))
        // maybeAttr "TimeoutSec" (stringValue (serviceConfig.TimeoutSec or null))
        // maybeAttr "TimeoutStartSec" (stringValue (serviceConfig.TimeoutStartSec or null))
        // maybeAttr "TimeoutStopSec" (stringValue (serviceConfig.TimeoutStopSec or null))
        // maybeAttr "WatchdogSec" (stringValue (serviceConfig.WatchdogSec or null))
        // maybeAttr "WorkingDirectory" (serviceConfig.WorkingDirectory or null)
        // optionalAttrs keepType {
          Type = serviceType;
        };
    in
    if serviceType != null && !keepType then
      throw "ides nixosServices.sanitizeService cannot lower serviceConfig.Type=${serviceType}"
    else
    {
      enable = service.enable or true;
      description = service.description or "";
      script = service.script or "";
      preStart = if serviceConfig ? ExecStartPre then "" else service.preStart or "";
      path = service.path or [ ];
      environment = filterAttrs (_: value: value != null) (service.environment or { });
      serviceConfig = serviceConfigSubset;
      wantedBy = service.wantedBy or [ ];
      requiredBy = service.requiredBy or [ ];
      wants = service.wants or [ ];
      requires = service.requires or [ ];
      after = service.after or [ ];
      before = service.before or [ ];
      partOf = service.partOf or [ ];
    };

  sanitizeServices = services: mapAttrs (_: sanitizeService) services;
  selectServices = names: services: genAttrs names (name: sanitizeService services.${name});
  fromNixosConfig = names: config: selectServices names config.systemd.services;
in
{
  inherit
    sanitizeService
    sanitizeServices
    selectServices
    supportedServiceConfigKeys
    supportedTypes
    fromNixosConfig
    ;
}
