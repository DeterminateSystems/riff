{{
  outputs = {{ self, nixpkgs }}: {{
    devShells.x86_64-linux.default =
      with import nixpkgs {{ system = "x86_64-linux"; }};
      stdenv.mkDerivation {{
        name = "fsm-shell";
        buildInputs = [ {} ];
      }};
  }};
}}