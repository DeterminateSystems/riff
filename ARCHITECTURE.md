# How Riff uses Nix

Riff is written in [Rust] but powered by the [Nix] package manager. This doc is
mostly conceptual and intended for those who are curious about how Riff uses Nix
while exposing very little of Nix to users. If you want to know more about how
to *use* Riff, we recommend consulting the project [README].

At the moment, Riff supports [Rust] projects. We intend to provide support for
additional languages in the future, so stay tuned for updates from us.

## How Riff uses Nix flakes

[Flakes][flake] are an experimental, opt-in feature for Nix that enable you to
encapsulate your Nix dependencies and outputs&mdash;packages, dev environments,
libraries, and more&mdash;in a standardized and declarative way.

Riff uses an internal [template] to generate a [`flake.nix`][flake] file tailored to
the specific needs of your Rust project. To build that `flake.nix`, Riff traverses
your Rust project's dependency graph, using the [`cargo metadata`][cargo
metadata] command, and supplies the necessary external dependencies to the
shell's `buildInputs` (the list of packages that are included in the Nix shell
environment).

Once Riff has generated its internal flake, the `riff shell` command essentially
wraps the [`nix develop`][nix develop] command while `riff run` wraps `nix
develop --command`. The `flake.nix` file itself, however, is written to a
temporary directory and thus doesn't end up in your project directory.

## How Riff handles dependencies

Everything that Riff installs is stored in your Nix store, which is under
`/nix/store` by default. This enables Riff to make executables available to `riff
shell` and `riff run` while storing them neither in your local project directory
nor under common system paths, like `/bin` or `/usr/bin`, where they're likely
to interfere with packages that you store there.

### The Riff registry

Riff keeps an internal [registry], in JSON form, of crates with known external
dependencies. The structure of the registry directly mirrors the package
metadata that you can provide in your `Cargo.toml`, but in JSON format. Here's
an example from the registry:

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
[`openssl-sys`][openssl-sys] crate can't be built without [OpenSSL]. If your
project has a direct or indirect dependency on `openssl-sys`, Riff adds the
`openssl` Nix package to your project's
[`buildInputs`](#how-riff-uses-nix-flakes). If the current system is a macOS
system&mdash;that is, if the Rust target triple is `aarch64-apple-darwin` or
`x86_64-apple-darwin`&mdash;Riff adds the [`Security`][security] framework to your
`buildInputs`.

[cargo metadata]: https://doc.rust-lang.org/cargo/commands/cargo-metadata.html
[flake]: https://nixos.wiki/wiki/Flakes
[nix]: https://nixos.org
[nix develop]: https://nixos.org/manual/nix/stable/command-ref/new-cli/nix3-develop.html
[nix store]: https://nixos.org/manual/nix/stable/introduction.html
[openssl]: https://openssl.org
[openssl-sys]: https://crates.io/crates/openssl-sys
[readme]: ./README.md
[registry]: ./registry/registry.json
[rust]: https://rust-lang.org
[security]: https://developer.apple.com/documentation/security
[template]: ./src/flake-template.inc
