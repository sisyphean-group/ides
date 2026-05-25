# import stage args
{
  pkgs ? import <nixpkgs>,
  shell ? pkgs.mkShell,
  modules ? [ ],
  auto ? true,
  ...
}:
let
  defaultAuto = auto;
in
# shell creation args
{
  services ? { },
  systemd ? { },
  imports ? [ ],
  serviceDefs ? { },
  auto ? defaultAuto,
  monitor ? true,
  ...
}@args:
let
  # filter ides args out
  # for passthrough to mkShell
  shellArgs = builtins.removeAttrs args [
    "services"
    "systemd"
    "serviceDefs"
    "imports"
    "auto"
    "monitor"
  ];
  # include some premade services
  baseModules = [ ./modules/redis.nix ];
  # eval the config
  eval = pkgs.lib.evalModules {
    modules =
      [
        # ides
        ./lib/ides.nix
        # service config and build params
        (_: {
          inherit
            services
            systemd
            serviceDefs
            auto
            monitor
            ;
          _buildIdes.shellFn = shell;
          _buildIdes.shellArgs = shellArgs;
        })
      ]
      ++ baseModules
      ++ modules
      ++ imports;

    specialArgs = {
      inherit pkgs;
    };

    class = "ides";
  };
in
eval.config._buildIdes.shell
