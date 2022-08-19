# fsm

`fsm` (Flying Spaghetti Monster) is a tool that automatically provides
external dependencies for building software projects. When developing
in a language like Rust, you typically use a language-specific package
manager like Cargo to handle dependencies. However, these tools often
don't handle dependencies written in other languages very well, expect
you to install these via your system package manager, and fail
mysteriously when they're missing:

```
   Compiling openssl-sys v0.9.75
error: failed to run custom build command for `openssl-sys v0.9.75`
  run pkg_config fail: "`\"pkg-config\" \"--libs\" \"--cflags\"
    \"openssl\"` did not exit successfully: \n... No package 'openssl' found\n"
```

It's then up to you to install the missing dependency, which is often
laborious and error-prone.

`fsm` instead lets you start a shell in which the external
dependencies required by your project are present automatically. These
shells are *transient*, meaning that they don't affect anything
outside the shell. No software is installed globally, so you don't
have to worry that the installation of a dependency will break
anything on your system — when you exit the shell, the dependencies
are gone.

`fsm` currently supports Rust/Cargo-based projects, with support for
other languages to be added in the future.

Internally, `fsm` uses the [Nix package manager](nixos.org/nix/) to
fetch or build native dependencies, but you do not need to know Nix or
write any Nix files.

## Requirements

In order to use `fsm`, you will need the following binaries available:

* [`nix`](https://nixos.org/nix/)
* [`cargo`](https://www.rust-lang.org/tools/install)

## Installation

TODO: download the statically linked binary

TODO: run/install via Nix, once our repo is public: `nix run
github:DeterminateSystems/fsm` or `nix profile install
github:DeterminateSystems/fsm`

## Example Usage

In this example, we build the [Tremor
project](https://github.com/tremor-rs/tremor-runtime) from source. It
has a number of native dependencies, such as OpenSSL and the Protobuf
compiler. `fsm` downloads or builds these dependencies for you
automatically, without installing them into your regular environment.

```
# git clone https://github.com/tremor-rs/tremor-runtime.git

# cd tremor-runtime

# fsm shell

# type -p protoc
/nix/store/2qg94y58v1jr4dw360bmpxlrs30m31ca-protobuf-3.19.4/bin/protoc

# cargo build

# exit

# protoc
protoc: command not found
```
