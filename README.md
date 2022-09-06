# Riff

![logo-light](img/logo/riff-white.svg#gh-dark-mode-only)
![logo-dark](img/logo/riff-black.svg#gh-light-mode-only)

**Riff** is a tool that automatically provides external dependencies[^1] for
software projects. To enter a shell environment with all your project's external
dependencies installed, run this at the project root:

```shell
riff shell
```

You can also directly run commands with the shell environment applied but
without entering the shell:

```shell
riff run cargo build
```

Riff currently supports [Rust] with support for other languages coming soon.
It uses the [Nix] package manager to handle dependencies but doesn't require
you to know or use Nix.

> For a video demo of Riff in action, see [below](#video-demo).

## Requirements

To use Riff, you need to install these binaries on your system:

* [`nix`][nix-install]
* [`cargo`][rust-install]

## Installation

### Using Nix

To install Riff using Nix (make sure to have [flakes] enabled):

```shell
nix profile install github:DeterminateSystems/riff
```

### Using Homebrew

To install Riff on macOS using [Homebrew]:

```shell
brew tap DeterminateSystems/riff
brew install riff
```

> **Note**: The `riff` Homebrew formula does *not* install [Nix] or [Cargo].

### Using cURL

You can find instructions for installing Riff using cURL on the
[releases page][releases].

## What Riff provides

Most programming languages use language-specific package managers to handle
dependencies, such as [Cargo] for the [Rust] language. But these
language-specific tools typically don't handle dependencies written in other
languages very well. They expect you to install those dependencies using some
other tool and fail in mysterious ways when they're missing. Here's an
example error from trying to build the [`octocrab`][octocrab] crate without
[OpenSSL] installed:

```shell
--- stderr
thread 'main' panicked at '

Could not find directory of OpenSSL installation, and this `-sys` crate cannot
proceed without this knowledge. If OpenSSL is installed and this crate had
trouble finding it,  you can set the `OPENSSL_DIR` environment variable for the
compilation process.

Make sure you also have the development packages of openssl installed.
For example, `libssl-dev` on Ubuntu or `openssl-devel` on Fedora.
```

In cases like this, it's up to you to install missing external dependencies,
which can be laborious, error prone, and hard to reproduce.

Riff enables you to bypass this problem entirely. It uses your your project's
language-specific configuration to infer which external dependencies are
required and creates a shell environment with those dependencies both installed
and properly linked. In cases where those dependencies can't be inferred, for
example in your [`build.rs`][build.rs] script, you can [explicitly declare
them](#how-to-declare-package-inputs) in your `Cargo.toml`.

These environments are *transient* in the sense that they don't affect
anything outside the shell; they install dependencies neither globally nor in
your current project, so you don't have to worry about Riff breaking anything
on your system. When you exit the Riff shell, the dependencies are gone.

### Offline mode

In cases where you want to limit Riff's access to the Internet, you can run it
in offline mode, which disables all network usage *except* what's required by
the `nix develop` command (which Riff runs in the background). You can enable
offline mode using either the `--offline` flag or the `RIFF_OFFLINE` environment
variable. Here are some examples:

```shell
# Via flag
riff run --offline

# Via environment variable
RIFF_OFFLINE=true riff shell
```

## Example usage

In this example, we'll build the [Prost] project from source. Prost has an
external dependency on [OpenSSL], without which commands like `cargo build`
and `cargo run` are doomed to fail. Riff provides those dependencies
automatically, without you needing to install them in your regular
environment. Follow these steps to see dependency inference in action:

```shell
git clone https://github.com/tokio-rs/prost.git
cd prost

# Enter the Riff shell environment
riff shell
# ✓ 🦀 rust: cargo, cmake, curl, openssl, pkg-config, rustc, rustfmt, zlib

# Check for the presence of openssl
which openssl
# The path should look like this:
# /nix/store/f3xbf94zykbh6drw6wfg9hdrfgwrkck7-openssl-1.1.1q-bin/bin/openssl
# This means that Riff is using the Nix-provided openssl

# Build the project
cargo build

# Leave the shell environment
exit

# Check for openssl again
which openssl
# This should either point to an openssl executable on your PATH or fail
```

## How to declare package inputs

While Riff does its best to infer external dependencies from your project's
crate dependencies, you can explicitly declare external dependencies if
necessary by adding a `riff` block to the `package.metadata` block in your
`Cargo.toml`. Riff currently supports three types of inputs:

* `build-inputs` are external dependencies that some crates may need to link
  against.
* `environment-variables` are environment variables you want to set in your dev
  shell.
* `runtime-inputs` are libraries you want to add to your `LD_LIBRARY_PATH` to
  ensure that your dev shell works as expected.

Both `build-inputs` and `runtime-inputs` can be any packages available in
[Nixpkgs]. You may find this particularly useful for [`build.rs`
scripts][build.rs].

Here's an example `Cargo.toml` with an explicitly supplied Riff configuration:

```toml
[package]
name = "riff-example"
version = "0.1.0"
edition = "2021"

[package.metadata.riff]
build-inputs = [ "openssl" ]
runtime-inputs = [ "libGL" ]

[package.metadata.riff.environment-variables]
HI = "BYE"

# Other configuration
```

When you run `riff shell` in this project, Riff

* adds [OpenSSL] to your build environment
* sets the `LD_LIBRARY_PATH` environment variable to include [libGL]'s library
  path
* sets the `HI` environment variable to have a value of `BYE`

### Target-specific dependencies

If a project has OS-, architecture-, or vendor-specific dependencies, you can
define them in a `targets` block under `package.metadata.riff`. Here's an
example for Apple M1 (`aarch64-apple-darwin`) systems:

```toml
[package.metadata.riff.targets.aarch64-apple-darwin]
build-inputs = [
  "darwin.apple_sdk.frameworks.CoreServices",
  "darwin.apple_sdk.frameworks.Security"
]
```

The Rust project maintains [a list of well-known targets][targets]
that you can view by running `nix run nixpkgs#rustup target list`. This
field can also contain custom targets, such as `riscv32imac-unknown-xous-elf`,
although `riff` makes no effort to support cross compiling at this time.

When target-specific dependencies are present, the `build-inputs` and
`runtime-inputs` sections are *unioned* (joined), while the target-specific
environment variables *override* default environment variables.

#### macOS framework dependencies

macOS users may encounter issues with so-called "framework" dependencies, such
as [`Foundation`][foundation], [`CoreServices`][coreservices], and
[`Security`][security]. When these dependencies are missing, you may see error
messages like this:

```
= note: ld: framework not found CoreFoundation
```

You can solve this by adding framework dependencies to your `build-inputs` as
`darwin.apple_sdk.frameworks.<framework>`, for example
`darwin.apple_sdk.frameworks.Security`. Here's an example `Cargo.toml`
configuration that adds multiple framework dependencies:

```toml
[package.metadata.riff.targets.x86_64-apple-darwin]
build-inputs = [
  "darwin.apple_sdk.frameworks.CoreServices",
  "darwin.apple_sdk.frameworks.Security"
]

[package.metadata.riff.targets.aarch64-apple-darwin]
build-inputs = [
  "darwin.apple_sdk.frameworks.CoreServices",
  "darwin.apple_sdk.frameworks.Security"
]
```

#### Riff understands dependencies transitively

If you add [Riff metadata](#how-to-declare-package-inputs) to `Cargo.toml`, this
doesn't just make it easier to build and run your project: it actually benefits
consumers of your crate as well. That's because Riff can use this metadata
transitively to infer which external dependencies are necessary *across the
entire crate dependency graph*. Let's say that you release a crate called
`make-it-pretty` that has an external dependency on [libGL] and you add that
to your `Cargo.toml`:

```toml
[package.metadata.riff]
runtime-inputs = [ "libGL" ]
```

Now let's say that another Rust dev releases a crate called `artify` that
depends on your `make-it-pretty` crate. If someone tries to build `artify` using
Cargo, they may receive an error if they don't have libGL installed. *But* if
they use Riff to build `artify`, Riff knows to install libGL without any user
input.

The implication is that adding Riff metadata to your crates&mdash;if they have
external dependencies&mdash;can benefit the Rust ecosystem more broadly.

## How it works

When you run `riff shell` in a Rust project, Riff

- **reads** your [`Cargo.toml`][cargo-toml] configuration manifest to determine
  which external dependencies your project requires and then
- **uses** the [Nix] package manager&mdash;in the background and without
  requiring any intervention on your part&mdash;to install any external
  dependencies, such as [OpenSSL] or [Protobuf], and also sets any environment
  variables necessary to discover those tools. Once it knows which external
  tools are required, it
- **builds** a custom shell environment that enables you to use commands like
  `cargo build` and `cargo run` without encountering the missing dependency
  errors that so often dog Rust development.

This diagram provides a basic visual description of that process:

<!-- Image editable at: https://miro.com/app/board/uXjVPdUOswQ=/ -->
<p align="center">
  <img
    src="img/riff.jpg"
    alt="Riff reads your Cargo.toml to infer external dependencies and then
      uses Nix to build a shell environment that provides those dependencies"
    style="width:70%;" />
</p>

Because Riff uses Nix, all of the dependencies that it installs are stored in
your local [Nix store], by default under `/nix/store`.

## Video demo

You can see a video demo of Riff in action here (click on the image for a
larger version):

<p align="center">
  <img src="img/riff-demo.gif"
      alt="Asciicast video demo of Riff with preview image"
      style="width:80%;" />
</p>

In the video, running `cargo build` in the [Prost] project fails due to missing
external dependencies. But running `riff run cargo build` succeeds because Riff
is able to infer which external dependencies are missing and provide them in the
background using Nix.

## Direnv Integration

You can add Riff support to Direnv on a project-specific or global basis. To
enable Riff in a project, create a `.envrc` file that contains this:

```bash
# reload when these files change
watch_file Cargo.toml Cargo.lock
# add any other files you might want to trigger a riff reload
# load the riff dev env
eval "$(riff print-dev-env)"
```

You can enable Riff support globally by either adding a `use_riff` function
either to your `~/.config/direnv/direnvrc` file or a new
`~/.config/direnv/lib/riff.sh` file. The `use_riff` function should look
something like this:

```bash
use_riff() {
  watch_file Cargo.toml watch_file Cargo.lock
  eval "$(riff print-dev-env)"
}
```

With Direnv now aware of this function, you can enable Riff in any directory
with:

```bash
echo "use riff" > .envrc
```

When you run `direnv allow` you will automatically enter the Riff shell every
time you navigate to the project directory.

## Privacy policy

For the sake of improving user experience, Riff does collect some [telemetry].
You can read the full privacy policy for [Determinate Systems], the
creators of Riff, [here][privacy].

To disable telemetry on any Riff command invocation, you can either

* Use the `--disable-telemetry` flag or
* Set the `RIFF_DISABLE_TELEMETRY` environment variable to any value except
  `false`,`0`, or an empty string (`""`).

Here are some examples:

```shell
# Via flag
riff shell --disable-telemetry

# Via environment variable
RIFF_DISABLE_TELEMETRY=true riff run cargo build
```

## Community

If you'd like to discuss Riff with other users, join our [Discord].

[build.rs]: https://doc.rust-lang.org/cargo/reference/build-scripts.html
[cargo]: https://doc.rust-lang.org/cargo
[cargo-toml]: https://doc.rust-lang.org/cargo/reference/manifest.html
[coreservices]: https://developer.apple.com/documentation/coreservices
[determinate systems]: https://determinate.systems
[discord]: https://discord.gg/urAzkgf7YM
[flakes]: https://nixos.wiki/wiki/Flakes
[foundation]: https://developer.apple.com/documentation/foundation
[homebrew]: https://brew.sh
[libgl]: https://dri.freedesktop.org/wiki/libGL
[nix]: https://nixos.org/nix
[nix-install]: https://nixos.org/download.html
[nixpkgs]: https://search.nixos.org/packages
[nix store]: https://nixos.wiki/wiki/Nix_package_manager
[octocrab]: https://github.com/XAMPPRocky/octocrab
[openssl]: https://openssl.org
[privacy]: https://determinate.systems/privacy
[prost]: https://github.com/tokio-rs/prost
[protobuf]: https://developers.google.com/protocol-buffers
[releases]: https://github.com/DeterminateSystems/riff/releases
[rust]: https://rust-lang.org
[rust-install]: https://www.rust-lang.org/tools/install
[security]: https://developer.apple.com/documentation/security
[targets]: https://doc.rust-lang.org/nightly/rustc/platform-support.html
[telemetry]: ./src/telemetry.rs

[^1]: We define **external** dependencies as those that are written in another
  language and thus can't be installed using the same language-specific package
  manager that you use to build your code.
