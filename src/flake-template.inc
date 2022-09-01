{{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  outputs = {{ self, nixpkgs }}:
    let
      nameValuePair = name: value: {{ inherit name value; }};
      genAttrs = names: f: builtins.listToAttrs (map (n: nameValuePair n (f n)) names);
      allSystems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];

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
            name = "riff-shell";
            buildInputs = [
              bashInteractive
              {build_inputs}
            ] ++ lib.optionals (stdenv.isDarwin) [
              libiconv
            ];

            {environment_variables}

            {ld_library_path}
          }};
      }});
  }};
}}
