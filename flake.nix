{
  description = "fsm";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-22.05";

    # glibc 2.31
    glibcNixpkgs.url = "github:nixos/nixpkgs/nixos-20.09";

    fenix = {
      url = "github:nix-community/fenix";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    naersk = {
      url = "github:nix-community/naersk";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    { self
    , nixpkgs
    , glibcNixpkgs
    , fenix
    , naersk
    , ...
    } @ inputs:
    let
      nameValuePair = name: value: { inherit name value; };
      genAttrs = names: f: builtins.listToAttrs (map (n: nameValuePair n (f n)) names);
      allSystems = [ "x86_64-linux" "aarch64-linux" "i686-linux" "x86_64-darwin" ];

      forAllSystems = f: genAttrs allSystems (system: f {
        inherit system;
        pkgs = import nixpkgs { inherit system; };
      });

      forAllSystemsOldGlibc = f: genAttrs allSystems (system: f {
        inherit system;
        pkgs = import glibcNixpkgs { inherit system; };
      });

      fenixToolchain = system: with fenix.packages.${system};
        combine [
          stable.clippy
          stable.rustc
          stable.cargo
          stable.rustfmt
          stable.rust-src
        ];
    in
    {
      devShell = forAllSystems ({ system, pkgs, ... }:
        let
          toolchain = fenixToolchain system;
        in
        pkgs.mkShell {
          name = "fsm-shell";

          RUST_SRC_PATH = "${toolchain}/lib/rustlib/src/rust/library";

          buildInputs = with pkgs; [
            toolchain

            codespell
            nixpkgs-fmt
            findutils # for xargs
            patchelf
          ];
        });

      packages = forAllSystemsOldGlibc
        ({ system, pkgs, ... }:
          let
            naerskLib = pkgs.callPackage naersk {
              cargo = fenixToolchain system;
              rustc = fenixToolchain system;
            };
          in
          {
            package = naerskLib.buildPackage rec {
              pname = "fsm";
              version = "unreleased";
              src = self;

              nativeBuildInputs = with pkgs; [
                pkg-config
                perl # necessary to build the vendored openssl
              ];

              override = { preBuild ? "", ... }: {
                preBuild = preBuild + ''
                  logRun "cargo clippy --all-targets --all-features -- -D warnings"
                '';
              };
            };
          });

      defaultPackage = forAllSystems ({ system, ... }: self.packages.${system}.package);
    };
}
