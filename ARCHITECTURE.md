# How Riff uses Nix

As we mention in the [README], Riff is powered by [Nix]. Although this should be
seen largely as an implementation detail.

This doc is intended solely for those who are curious about how Riff uses Nix
while exposing very little of Nix to users. If you want to know more about how
to use Riff, we recommend consulting the project [README].

## Language support

At the moment, Riff supports [Rust] projects. We intend to provide support for
additional languages in the future

## Riff uses Nix flake

Riff uses an internal [template] to generate a [`flake.nix`][flake]

The `riff shell` command is a wrapper around `nix develop`, while `riff run` is
a wrapper around `nix develop --command`.

## How Riff builds the flake

Riff traverses your project's dependency graph and uses that information to
assemble a `flake.nix` file (that it uses only internally).

### Package metadata

Riff enables you to directly supply Riff-specific metadata in your `Cargo.toml`
file.

### The Riff registry

Riff keeps an internal [registry], in JSON form, of known dependencies. The
structure of the registry directly mirrors the package metadata that you can
provide in your `Cargo.toml`, just in JSON format. Here's an example from the
registry:

```js
"openssl-sys": {
  "build-inputs": [
    "openssl"
  ],
  "targets": {
    "aarch64-apple-darwin": {
      "build-inputs": [
        "darwin.apple_sdk.frameworks.Security"
      ]
    },
    "x86_64-apple-darwin": {
      "build-inputs": [
        "darwin.apple_sdk.frameworks.Security"
      ]
    }
  }
}
```

In this case, our internal tests have determined that the
[`openssl-sys`][openssl-sys] crate can't be built without [OpenSSL]

## How Riff handles dependencies

Everything that Riff installs is stored in your Nix store, which is under
`/nix/store` by default. This is how Riff is able to make executables available
while storing them neither in the local project directory nor under common
system paths, like `/bin` or `/usr/bin`, where they're likely to interfere with
packages that you store there.

[flake]: https://nixos.wiki/wiki/Flakes
[nix]: https://nixos.org
[openssl]: https://openssl.org
[openssl-sys]: https://crates.io/crates/openssl-sys
[readme]: ./README.md
[registry]: ./registry/registry.json
[rust]: https://rust-lang.org
[template]: ./src/flake-template.inc
