{ pkgs }:

let
  inherit (pkgs) writeScriptBin;
in
[
  # Check spelling
  (writeScriptBin "ci-check-spelling" ''
    codespell \
      --ignore-words-list crate,pullrequest,pullrequests,ser \
      --skip target \
      .
  '')

  # Rust formatting check
  (writeScriptBin "ci-check-rustfmt" "cargo fmt --check")

  # Rust test
  (writeScriptBin "ci-test-rust" "cargo test")

  (writeScriptBin "ci-check-nixpkgs-fmt" ''
    git ls-files '*.nix' | xargs | nixpkgs-fmt --check
  '')

  (writeScriptBin "ci-check-registry-format" ''
    ./registry/format.sh && git diff --exit-code
  '')

  (writeScriptBin "ci-check-editorconfig" ''
    git ls-files . | xargs eclint
  '')

  (writeScriptBin "ci-all" ''
    ci-check-spelling
    ci-check-rustfmt
    ci-test-rust
    ci-check-nixpkgs-fmt
    ci-check-registry-format
    ci-check-editorconfig
  '')
]
