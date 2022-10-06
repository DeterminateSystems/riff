# Releasing a new version

1. Bump the version in [`Cargo.toml`](./Cargo.toml) (and run e.g.
`cargo build` in order to update the `Cargo.lock` version to match) and
[`registry.json`](./registry/registry.json) -- for an example, see
https://github.com/DeterminateSystems/riff/pull/158
1. Create the release for real via
https://github.com/DeterminateSystems/riff/releases/new
1. Add the `x86_64-linux`, `x86_64-darwin`, and `aarch64-darwin` binaries
from CI to the release
1. Produce an `aarch64-linux` binary and add it to the release
1. Bump the version used in the GitHub Action
(https://github.com/DeterminateSystems/install-riff-action)
1. Bump the version used in the Homebrew formula
(https://github.com/DeterminateSystems/homebrew-riff)
1. Bump the Riff version on riff.sh
1. Bump the Riff version in the telemetry server
