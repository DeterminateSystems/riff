{{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = {{ self, nixpkgs }}: {{
    devShells.x86_64-linux.default =
      with import nixpkgs {{ system = "x86_64-linux"; }};
      stdenv.mkDerivation {{
        name = "fsm-shell";
        buildInputs = [ {build_inputs} ];
        {environment_variables}

        shellHook = ''
          {ld_library_path}
        '';
      }};
  }};
}}
