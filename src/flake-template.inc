{{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = {{ self, nixpkgs }}: {{
    devShells.x86_64-linux.default =
      with import nixpkgs {{ system = "x86_64-linux"; }};
      stdenv.mkDerivation {{
        name = "fsm-shell";
        buildInputs = [ {} ];
        {}

        # Done to ensure "__FORIFTY_SOURCE requires compiling with optimization" errors don't fail builds
        # This was noted in `spirv-tools-sys` and others.
        hardeningDisable = [ "fortify" ];
      }};
  }};
}}
