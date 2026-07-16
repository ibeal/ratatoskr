{
  description = "ratatoskr / rata CLI";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-26.05";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "ratatoskr";
          version = "0.1.0";
          src = ./.;
          cargoLock.lockFile = ./Cargo.lock;

          meta = with pkgs.lib; {
            description = "Filesystem-first CLI for portable AI-agent context";
            mainProgram = "rata";
            license = licenses.mit;
            platforms = platforms.unix ++ platforms.windows;
          };
        };

        apps.default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/rata";
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            clippy
            rustc
            rustfmt
          ];
        };
      }
    );
}
