{
  pkgs,
  config,
  ...
}:
{
  config =
    let
      writers = {
        java = pkgs.formats.javaProperties { };
        json = pkgs.formats.json { };
        yaml = pkgs.formats.yaml { };
        ini = pkgs.formats.ini { };
        toml = pkgs.formats.toml { };
        xml = pkgs.formats.xml { };
        php = pkgs.formats.php { finalVariable = null; };
      };

      # control flow monstrosity
      branchOnConfig =
        cfg:
        {
          text,
          file,
          content,
          contentFmt,
          runtime,
          empty ? "",
        }:
        if (cfg.runtime != null) then
          runtime
        else if (cfg.text != "") then
          text
        else if (cfg.file != null) then
          file
        else if (cfg.content != null) then
          if (cfg.format != null) then
            content
          else if (cfg.formatter != null) then
            contentFmt
          else
            throw "`format` or `formatter` must be set for `content` value ${cfg.content}!"
        else
          empty;

      configExt =
        cfg:
        let
          rawExt =
            if cfg.ext != "" then
              cfg.ext
            else if cfg.format != null then
              cfg.format
            else
              "";
        in
        if rawExt == "" then "" else if pkgs.lib.hasPrefix "." rawExt then rawExt else "." + rawExt;

      configHash =
        cfg:
        let
          hashContent = builtins.hashString "sha256" (builtins.toJSON cfg.content);
        in
        branchOnConfig cfg {
          runtime = builtins.hashString "sha256" (builtins.toJSON cfg.runtime);
          text = builtins.hashString "sha256" cfg.text;
          file = builtins.hashFile "sha256" cfg.file;
          content = hashContent;
          contentFmt = hashContent;
          empty = "empty";
        };

      renderConfig =
        serviceName: configName: cfg:
        let
          cfgHash = configHash cfg;
          path = "config-${serviceName}-${configName}-${cfgHash}${configExt cfg}";
          hasStaticConfig = cfg.text != "" || cfg.file != null || cfg.content != null;
          normalizeRuntimePart =
            part:
            if builtins.isAttrs part then
              if part ? runtimePath then
                { runtimePath = part.runtimePath; }
              else if part ? env then
                { env = part.env; }
              else
                throw "serviceDefs.${serviceName}.configs.${configName}.runtime.parts contains an unsupported attribute set"
            else
              builtins.toString part;
          runtimeFileName =
            if cfg.runtime.fileName != null then
              cfg.runtime.fileName
            else
              "${configName}${configExt cfg}";
          runtimeManifest = {
            fileName = runtimeFileName;
            parts = map normalizeRuntimePart cfg.runtime.parts;
          };
        in
        if cfg.runtime != null && hasStaticConfig then
          throw "serviceDefs.${serviceName}.configs.${configName} cannot mix `runtime` with `text`, `file`, or `content`"
        else
        {
          hash = cfgHash;
          displayPath = branchOnConfig cfg {
            runtime = "<runtime:${runtimeFileName}>";
            text = "${pkgs.writeText path cfg.text}";
            file = "${cfg.file}";
            content = "${writers.${cfg.format}.generate path cfg.content}";
            contentFmt = "${cfg.formatter path cfg.content}";
            empty = throw "serviceDefs.${serviceName}.configs.${configName} must set `text`, `file`, `content`, or `runtime`";
          };
          manifest = branchOnConfig cfg {
            runtime = {
              path = null;
              runtime = runtimeManifest;
            };
            text = {
              path = "${pkgs.writeText path cfg.text}";
              runtime = null;
            };
            file = {
              path = "${cfg.file}";
              runtime = null;
            };
            content = {
              path = "${writers.${cfg.format}.generate path cfg.content}";
              runtime = null;
            };
            contentFmt = {
              path = "${cfg.formatter path cfg.content}";
              runtime = null;
            };
            empty = throw "serviceDefs.${serviceName}.configs.${configName} must set `text`, `file`, `content`, or `runtime`";
          };
        };
    in
    {
      # validate and complete the service configurations
      _buildIdes.finalServices = builtins.mapAttrs (
        name:
        {
          pkg,
          exec ? "",
          argv ? [ ],
          configs,
          path,
          socket,
          timer,
          systemd,
          shellCommand,
          dependencies,
          runtime,
        }:
        let
          renderedConfigs = pkgs.lib.mapAttrs (configName: cfg: renderConfig name configName cfg) configs;
          configManifests = pkgs.lib.mapAttrs (_: rendered: rendered.manifest) renderedConfigs;
          configHashes = pkgs.lib.mapAttrs (_: rendered: rendered.hash) renderedConfigs;
          resolveManifestArg =
            value:
            if builtins.isAttrs value then
              if value ? config then
                if builtins.hasAttr value.config renderedConfigs then
                  { config = value.config; }
                else
                  throw "serviceDefs.${name}.argv references missing config `${value.config}`"
              else
                throw "serviceDefs.${name}.argv contains an unsupported attribute set"
            else
              builtins.toString value;
          resolveDisplayArg =
            value:
            if builtins.isAttrs value then
              if value ? config then
                if builtins.hasAttr value.config renderedConfigs then
                  (builtins.getAttr value.config renderedConfigs).displayPath
                else
                  throw "serviceDefs.${name}.argv references missing config `${value.config}`"
              else
                throw "serviceDefs.${name}.argv contains an unsupported attribute set"
            else
              builtins.toString value;
          manifestArgv = map resolveManifestArg argv;
          displayArgv = map resolveDisplayArg argv;
          # make our best effort to use the correct binary
          bin =
            if shellCommand != null && pkg != null then
              throw "serviceDefs.${name} must set either `pkg` or `shellCommand`, not both"
            else if shellCommand != null && argv != [ ] then
              throw "serviceDefs.${name} must set either `argv` or `shellCommand`, not both"
            else if shellCommand != null && configs != { } then
              throw "serviceDefs.${name} cannot use generated `configs` with `shellCommand`; use structured `argv` instead"
            else if shellCommand != null then
              null
            else if pkg != null then
              if (exec == "") then pkgs.lib.getExe pkg else pkgs.lib.getExe' pkg exec
            else
              throw "serviceDefs.${name} must set `pkg` or `shellCommand`";
          command =
            if shellCommand != null then
              shellCommand
            else
              pkgs.lib.escapeShellArgs ([ bin ] ++ displayArgv);
          runtimePaths = {
            base = "services/${name}";
            run = "services/${name}/run";
            tmp = "services/${name}/tmp";
            cache = "services/${name}/cache";
            config = "services/${name}/config";
            data = "services/${name}/data";
            home = "services/${name}/home";
          };
          # flatten unit options into cli args
          sdArgs =
            let
              inherit (pkgs.lib) foldlAttrs;
              inherit (builtins) concatStringsSep;
              convertToArgList =
                prefix: name: values:
                (map (inner: "${prefix} ${name}=${inner}") values);
              writeArgListFor =
                attrs: prefix:
                if (attrs != { }) then
                  foldlAttrs (
                    acc: n: v:
                    acc + (concatStringsSep " " (convertToArgList prefix n v)) + " "
                  ) "" attrs
                else
                  "";
            in
            concatStringsSep " " (builtins.filter (arg: arg != "") [
              (writeArgListFor socket "--socket-property")
              (writeArgListFor path "--path-property")
              (writeArgListFor timer "--timer-property")
            ]);
        in
        # transform into attrs that mkWorks expects to receive
        {
          inherit
            command
            sdArgs
            shellCommand
            dependencies
            runtime
            systemd
            runtimePaths
            socket
            path
            timer
            configHashes
            configManifests
            ;
          program = bin;
          argv = manifestArgv;
          unitName = "shell-${name}-${builtins.hashString "sha256" (builtins.toJSON {
            inherit
              command
              configHashes
              manifestArgv
              dependencies
              runtime
              systemd
              path
              runtimePaths
              socket
              timer
              ;
          })}";
        }
      ) config.serviceDefs;

      # generate service scripts and create the shell
      _buildIdes.shell =
        let
          # create commands to run and clean up services
          mkWorks =
            name:
            {
              unitName,
              command,
              sdArgs,
              ...
            }:
            {
              runner = pkgs.writeShellScriptBin "run" ''
                echo "[ides]: starting ${name}.."
                systemd-run --user -q -G -u ${unitName} ${sdArgs} ${command}
              '';
              cleaner = pkgs.writeShellScriptBin "clean" ''
                echo "[ides]: stopping ${name}.."
                systemctl --user -q stop ${unitName}
              '';
              status = pkgs.writeShellScriptBin "status" ''
                systemctl --user -q status ${unitName}
              '';
            };

          works = pkgs.lib.mapAttrs (
            name: serviceConf: mkWorks name serviceConf
          ) config._buildIdes.finalServices;

          # create the ides cli
          cli = import ./cli.nix {
            inherit (pkgs) writeShellScriptBin;
            inherit (pkgs.lib) foldlAttrs;
            inherit works manifest;
            rustCli = lucius;
          };
          # shell id is based on the services config
          shellId = builtins.hashString "sha256" (builtins.toJSON config._buildIdes.finalServices);
          manifest = import ./manifest.nix {
            inherit pkgs shellId;
            inherit (config) auto;
            services = config._buildIdes.finalServices;
          };
          lucius = import ../lucius/package.nix { inherit pkgs; };
          # create the ides shell
          final =
            let
              inherit (config._buildIdes) shellArgs;
              monitorEnabled = config.monitor != false;
            in
            shellArgs
            // {
              nativeBuildInputs = (shellArgs.nativeBuildInputs or [ ]) ++ [
                cli
              ];
              IDES_MANIFEST = "${manifest}";
              shellHook =
                let
                  autoRun =
                    if config.auto then
                      ''
                        ides run
                      ''
                    else
                      "";
                  leaseRun =
                    if monitorEnabled then
                      ''
                        _IDES_ENTER_OUTPUT="$(ides enter --kind shell --root "$PWD" --pid $$)"
                        _IDES_ENTER_STATUS=$?
                        if [ "$_IDES_ENTER_STATUS" -ne 0 ]; then
                          unset _IDES_ENTER_OUTPUT
                          return "$_IDES_ENTER_STATUS" 2>/dev/null || exit "$_IDES_ENTER_STATUS"
                        fi
                        export IDES_LEASE_TOKEN="$_IDES_ENTER_OUTPUT"
                        unset _IDES_ENTER_OUTPUT _IDES_ENTER_STATUS
                        _ides_leave() {
                          if [ -n "''${IDES_LEASE_TOKEN:-}" ]; then
                            ides leave --token "$IDES_LEASE_TOKEN"
                          fi
                        }
                        trap _ides_leave EXIT
                      ''
                    else
                      "";
                in
                (shellArgs.shellHook or "")
                + ''
                  printf '[ides]: use "ides [action] [target]" to control services. type "ides help" to find out more.\n'
                  export IDES_CTL="/run/user/$(id -u)/ides-${shellId}.sock"
                  export IDES_MANIFEST="${manifest}"
                ''
                + leaseRun
                + autoRun
                ;
            };
        in
        # TODO make this optionally return the shell components to allow composability with other dev shell solutions
        config._buildIdes.shellFn final;
    };

}
