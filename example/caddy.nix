{ pkgs, ... }:
{
  # simplest possible concrete service definition
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
      http://*:8888 {
      	respond "hello"
      }
    '';
  };
}
