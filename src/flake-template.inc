{{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = {{ self, nixpkgs }}:
    let
      nameValuePair = name: value: {{ inherit name value; }};
      genAttrs = names: f: builtins.listToAttrs (map (n: nameValuePair n (f n)) names);
      allSystems = [ "x86_64-linux" "aarch64-linux" "i686-linux" "x86_64-darwin" "aarch64-darwin" ];

      forAllSystems = f: genAttrs allSystems (system: f rec {{
        inherit system;
        pkgs = import nixpkgs {{ inherit system; }};
        lib = pkgs.lib;
      }});
    in
    {{
      devShells = forAllSystems ({{ system, pkgs, ... }}: {{
        default = with pkgs;
          stdenv.mkDerivation {{
            name = "fsm-shell";
            buildInputs = [ {build_inputs} ];
            {environment_variables}

            {ld_library_path}
          }};
      }});
  }};
}}
