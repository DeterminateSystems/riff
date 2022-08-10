# fsm

Software written in languages such as Rust often needs some *native
dependencies* that are not handled by the language's package manager;
developers are supposed to install these manually, usually via the
system package manager. `fsm` (Flying Spaghetti Monster) is a tool
that handles these native dependencies for you. It lets you start a
shell in which the native dependencies required by your project are
present automatically.

`fsm` uses the [Nix package manager](nixos.org/nix/) to fetch or build
native dependencies, but you do not need to know Nix or write any Nix
files. Since Nix development shells are transient, you don't have to
worry about the installation of a dependency breaking anything on your
system: when you exit the shell, the dependencies are gone.

`fsm` currently supports Rust/Cargo-based projects, with support for
other languages to be added in the future.

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
