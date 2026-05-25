{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    {
      self,
      nixpkgs,
    }:
    let
      linuxSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forLinuxSystems = nixpkgs.lib.genAttrs linuxSystems;
    in
    {
      lib = {
        use = import ./default.nix;
        nixosServices = import ./lib/nixos-services.nix {
          inherit (nixpkgs) lib;
        };
      };
      packages = forLinuxSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          lucius = import ./lucius/package.nix { inherit pkgs; };
          default = self.packages.${system}.lucius;
        }
      );
      templates.default = {
        path = ./example;
        description = "the ides template";
      };
    };
}
