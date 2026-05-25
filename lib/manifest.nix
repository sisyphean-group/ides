{
  pkgs,
  services,
  shellId,
  auto,
  name ? "default",
}:

let
  manifest = {
    schemaVersion = 1;
    setId = shellId;
    inherit name;
    autoStart = auto;
    runtimeShell = pkgs.runtimeShell;
    runtime = {
      base = "ides/${shellId}";
    };
    services = pkgs.lib.mapAttrs (_: service: {
      inherit (service) unitName;
      exec = {
        program = service.program or null;
        argv = service.argv or [ ];
        shellCommand = service.shellCommand or null;
        inherit (service) command;
      };
      configs = service.configManifests or { };
      activation = {
        socket = service.socket or { };
        path = service.path or { };
        timer = service.timer or { };
      };
      runtime = (service.runtime or { }) // {
        paths = service.runtimePaths or {
          base = "";
          run = "";
          tmp = "";
          cache = "";
          config = "";
          data = "";
          home = "";
        };
      };
      systemd = service.systemd or { };
      systemdArgs = service.sdArgs or "";
      dependencies = service.dependencies or {
        requires = [ ];
        wants = [ ];
        after = [ ];
        before = [ ];
        partOf = [ ];
      };
    }) services;
  };
in
pkgs.writeText "ides-manifest-${builtins.substring 0 12 shellId}.json" (
  builtins.toJSON manifest
)
