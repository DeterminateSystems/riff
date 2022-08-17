//! The developer environment setup.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use eyre::eyre;
use itertools::Itertools;
use once_cell::sync::Lazy;
use owo_colors::OwoColorize;
use tokio::process::Command;

use crate::cargo_metadata::CargoMetadata;

#[derive(Default)]
pub struct DevEnvironment {
    build_inputs: HashSet<String>,
    environment_variables: HashMap<String, String>,
    ld_library_path: HashSet<String>,
}

impl DevEnvironment {
    pub fn to_flake(&self) -> String {
        // TODO: use rnix for generating Nix?
        format!(
            include_str!("flake-template.inc"),
            build_inputs = self.build_inputs.iter().join(" "),
            environment_variables = self
                .environment_variables
                .iter()
                .map(|(name, value)| format!("\"{}\" = \"{}\";", name, value))
                .join("\n"),
            ld_library_path = if !self.ld_library_path.is_empty() {
                format!(
                    "\"LD_LIBRARY_PATH\" = \"{}\";",
                    self.ld_library_path
                        .iter()
                        .map(|v| format!("${{lib.getLib {v}}}/lib"))
                        .join(":")
                )
            } else {
                "".to_string()
            }
        )
    }

    pub async fn detect(&mut self, project_dir: &Path) -> color_eyre::Result<()> {
        let mut any_found = false;

        if project_dir.join("Cargo.toml").exists() {
            self.add_deps_from_cargo(project_dir).await?;
            any_found = true;
        }

        if !any_found {
            eprintln!(
                "'{}' does not contain a project recognized by FSM.",
                project_dir.display()
            );
        }

        Ok(())
    }

    #[tracing::instrument(skip_all, fields(project_dir = %project_dir.display()))]
    async fn add_deps_from_cargo(&mut self, project_dir: &Path) -> color_eyre::Result<()> {
        // We do this because of `clippy::type-complexity`
        struct KnownCrateRegistryValue {
            build_inputs: HashSet<&'static str>,
            environment_variables: HashMap<&'static str, &'static str>,
            ld_library_path_inputs: HashSet<&'static str>,
        }
        static KNOWN_CRATE_REGISTRY: Lazy<HashMap<&'static str, KnownCrateRegistryValue>> =
            Lazy::new(|| {
                let mut m = HashMap::new();
                macro_rules! crate_to_build_inputs {
                    ($collection:ident, $rust_package:expr, $build_inputs:expr) => {
                        crate_to_build_inputs!(
                            $collection,
                            $rust_package,
                            $build_inputs,
                            env = [],
                            ld = []
                        );
                    };
                    ($collection:ident, $rust_package:expr, $build_inputs:expr, env = $environment_variables:expr) => {
                        crate_to_build_inputs!(
                            $collection,
                            $rust_package,
                            $build_inputs,
                            env = $environment_variables,
                            ld = []
                        );
                    };
                    ($collection:ident, $rust_package:expr, $build_inputs:expr, ld = $ld_library_path_inputs:expr) => {
                        crate_to_build_inputs!(
                            $collection,
                            $rust_package,
                            $build_inputs,
                            env = [],
                            ld = $ld_library_path_inputs
                        );
                    };
                    ($collection:ident, $rust_package:expr, $build_inputs:expr, env = $environment_variables:expr, ld = $ld_library_path_inputs:expr) => {
                        $collection.insert(
                            $rust_package,
                            KnownCrateRegistryValue {
                                build_inputs: $build_inputs.into_iter().collect(),
                                environment_variables: $environment_variables.into_iter().collect(),
                                ld_library_path_inputs: $ld_library_path_inputs
                                    .into_iter()
                                    .collect(),
                            },
                        )
                    };
                }
                crate_to_build_inputs!(
                    m,
                    "ash",
                    [
                        "vulkan-loader",
                        "vulkan-tools",
                        "vulkan-headers",
                        "vulkan-validation-layers"
                    ],
                    ld = ["vulkan-loader"]
                );
                crate_to_build_inputs!(m, "openssl-sys", ["openssl"]);
                crate_to_build_inputs!(m, "alsa-sys", ["alsa-lib"]);
                crate_to_build_inputs!(m, "atk-sys", ["atk"]);
                crate_to_build_inputs!(m, "bindgen", ["rustPlatform.bindgenHook"]);
                crate_to_build_inputs!(m, "bzip2-sys", ["bzip2"]);
                crate_to_build_inputs!(m, "cairo-sys-rs", ["cairo"]);
                crate_to_build_inputs!(
                    m,
                    "clang-sys",
                    ["llvmPackages.libclang", "llvm"],
                    env = [("LIBCLANG_PATH", "${llvmPackages.libclang.lib}/lib"),]
                );
                crate_to_build_inputs!(m, "curl-sys", ["curl"]);
                crate_to_build_inputs!(m, "egl", ["libGL"]);
                crate_to_build_inputs!(m, "expat-sys", ["expat"]);
                crate_to_build_inputs!(m, "freetype-sys", ["freetype"]);
                crate_to_build_inputs!(m, "gdk-pixbuf-sys", ["gdk-pixbuf"]);
                crate_to_build_inputs!(m, "gdk-sys", ["wrapGAppsHook", "gtk3"]);
                crate_to_build_inputs!(m, "gio-sys", ["glib"]);
                crate_to_build_inputs!(m, "gstreamer-audio-sys", ["gst_all_1.gst-plugins-base"]);
                crate_to_build_inputs!(m, "gstreamer-base-sys", ["gst_all_1.gstreamer"]);
                crate_to_build_inputs!(m, "gtk4-sys", ["gtk4"]);
                crate_to_build_inputs!(m, "hidapi", ["udev"]);
                crate_to_build_inputs!(m, "libadwaita-sys", ["libadwaita"]);
                crate_to_build_inputs!(m, "libdbus-sys", ["dbus"]);
                crate_to_build_inputs!(m, "libgit2-sys", ["libgit2"]);
                crate_to_build_inputs!(m, "libshumate-sys", ["libshumate"]);
                crate_to_build_inputs!(m, "libsqlite3-sys", ["sqlite"]);
                crate_to_build_inputs!(m, "libudev-sys", ["eudev"]);
                crate_to_build_inputs!(m, "libusb1-sys", ["libusb"]);
                crate_to_build_inputs!(
                    m,
                    "libz-sys",
                    ["zlib", "cmake" /* For `zlib-ng` feature */]
                );
                crate_to_build_inputs!(m, "pango-sys", ["pango"]);
                crate_to_build_inputs!(m, "pkg-config", ["pkg-config"]);
                crate_to_build_inputs!(m, "prost", ["cmake"]);
                crate_to_build_inputs!(m, "qt_3d_render", ["libGL"]);
                crate_to_build_inputs!(m, "qt_gui", ["qt5.full"]);
                crate_to_build_inputs!(m, "rdkafka-sys", ["rdkafka", "cyrus_sasl"]);
                crate_to_build_inputs!(m, "servo-fontconfig-sys", ["fontconfig"]);
                crate_to_build_inputs!(m, "smithay-client-toolkit", ["libxkbcommon", "pkg-config"]);
                crate_to_build_inputs!(m, "spirv-tools-sys", ["spirv-tools"]);
                crate_to_build_inputs!(m, "wayland-sys", ["wayland"]);
                crate_to_build_inputs!(m,
                    "wgpu-hal",
                    [],
                    env = [("ALSA_PLUGIN_DIR", "${pkgs.symlinkJoin { name = \"merged-alsa-plugins\"; paths = with pkgs; [ alsaPlugins pipewire.lib ]; }}/lib/alsa-lib")],
                    ld = ["libGL", "spirv-tools", "vulkan-tools", "vulkan-loader", "vulkan-headers", "vulkan-extension-layer", "vulkan-validation-layers", "mesa", "mesa_drivers", "alsaPlugins", "pipewire"]
                );
                crate_to_build_inputs!(
                    m,
                    "winit",
                    ["xorg.libX11"],
                    ld = [
                        "xorg.libX11",
                        "xorg.libXcursor",
                        "xorg.libXrandr",
                        "xorg.libXi",
                        "libGL",
                        "glxinfo",
                    ]
                );
                crate_to_build_inputs!(m, "xcb", ["xorg.libxcb"]);
                crate_to_build_inputs!(m, "xkbcommon-sys", ["libxkbcommon"]);
                crate_to_build_inputs!(
                    m,
                    "zstd-sys",
                    ["zlib", "clang" /* For `bindgen` feature */]
                );
                m
            });

        tracing::debug!("Adding Cargo dependencies...");

        let mut cmd = Command::new("cargo");
        cmd.args(&["metadata", "--format-version", "1"]);
        cmd.arg("--manifest-path");
        cmd.arg(project_dir.join("Cargo.toml"));

        let output = cmd.output().await?;
        if !output.status.success() {
            return Err(eyre!("`cargo metadata` failed to execute"));
        }

        let stdout = std::str::from_utf8(&output.stdout)?;
        let metadata: CargoMetadata = serde_json::from_str(stdout)?;

        let mut found_build_inputs = HashSet::new();
        let mut found_envs = HashMap::new();
        let mut found_ld_inputs = HashSet::new();

        found_build_inputs.insert("rustc".to_string());
        found_build_inputs.insert("cargo".to_string());
        found_build_inputs.insert("rustfmt".to_string());

        for package in metadata.packages {
            let name = package.name;

            if let Some(KnownCrateRegistryValue {
                build_inputs: known_build_inputs,
                environment_variables: known_envs,
                ld_library_path_inputs: known_ld_inputs,
            }) = KNOWN_CRATE_REGISTRY.get(&*name)
            {
                let known_build_inputs = known_build_inputs
                    .iter()
                    .map(ToString::to_string)
                    .collect::<HashSet<_>>();

                let known_ld_inputs = known_ld_inputs
                    .iter()
                    .map(ToString::to_string)
                    .collect::<HashSet<_>>();
                tracing::debug!(
                    package_name = %name,
                    buildInputs = %known_build_inputs.iter().join(", "),
                    environment_variables = %known_envs.iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                    "Detected known crate information"
                );
                found_build_inputs = found_build_inputs
                    .union(&known_build_inputs)
                    .cloned()
                    .collect();

                for (known_key, known_value) in known_envs {
                    found_envs.insert(known_key.to_string(), known_value.to_string());
                }

                found_ld_inputs = found_ld_inputs.union(&known_ld_inputs).cloned().collect();
            }

            let metadata_object = match package.metadata {
                Some(metadata_object) => metadata_object,
                None => continue,
            };

            let fsm_object = match metadata_object.fsm {
                Some(fsm_object) => fsm_object,
                None => continue,
            };

            let package_build_inputs = match &fsm_object.build_inputs {
                Some(build_inputs_table) => {
                    let mut package_build_inputs = HashSet::new();
                    for (key, _value) in build_inputs_table.iter() {
                        // TODO(@hoverbear): Add version checking
                        package_build_inputs.insert(key.to_string());
                    }
                    package_build_inputs
                }
                None => Default::default(),
            };

            let package_envs = match &fsm_object.environment_variables {
                Some(envs_table) => {
                    let mut package_envs = HashMap::new();
                    for (key, value) in envs_table.iter() {
                        package_envs.insert(key.to_string(), value.to_string());
                    }
                    package_envs
                }
                None => Default::default(),
            };

            let package_ld_inputs = match &fsm_object.ld_library_path_inputs {
                Some(ld_table) => {
                    let mut package_ld_inputs = HashSet::new();
                    for (key, _value) in ld_table.iter() {
                        // TODO(@hoverbear): Add version checking
                        package_ld_inputs.insert(key.to_string());
                    }
                    package_ld_inputs
                }
                None => Default::default(),
            };

            tracing::debug!(
                package = %name,
                "build-inputs" = %package_build_inputs.iter().join(", "),
                "environment-variables" = %package_envs.iter().map(|(k, v)| format!("{k}={v}")).join(", "),
                "LD_LIBRARY_PATH-inputs" = %package_ld_inputs.iter().join(", "),
                "Detected `package.fsm` in `Crate.toml`"
            );
            found_build_inputs = found_build_inputs
                .union(&package_build_inputs)
                .cloned()
                .collect();

            for (package_env_key, package_env_value) in package_envs {
                found_envs.insert(package_env_key, package_env_value);
            }

            found_ld_inputs = found_ld_inputs.union(&package_ld_inputs).cloned().collect();
        }

        eprintln!(
            "{check} {lang}: {colored_inputs}{maybe_colored_envs}",
            check = "✓".green(),
            lang = "🦀 rust".bold().red(),
            colored_inputs = {
                let mut sorted_build_inputs = found_build_inputs
                    .union(&found_ld_inputs)
                    .collect::<Vec<_>>();
                sorted_build_inputs.sort();
                sorted_build_inputs.iter().map(|v| v.cyan()).join(", ")
            },
            maybe_colored_envs = {
                if !found_envs.is_empty() {
                    let mut sorted_build_inputs =
                        found_envs.iter().map(|(k, _)| k).collect::<Vec<_>>();
                    sorted_build_inputs.sort();
                    format!(
                        " ({})",
                        sorted_build_inputs.iter().map(|v| v.green()).join(", ")
                    )
                } else {
                    "".to_string()
                }
            }
        );

        self.build_inputs = found_build_inputs;
        self.environment_variables = found_envs;
        self.ld_library_path = found_ld_inputs;

        Ok(())
    }
}
