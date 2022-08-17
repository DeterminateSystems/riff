use std::collections::{HashMap, HashSet};

use once_cell::sync::Lazy;

pub struct KnownCrateRegistryValue {
    pub build_inputs: HashSet<&'static str>,
    pub environment_variables: HashMap<&'static str, &'static str>,
    pub ld_library_path_inputs: HashSet<&'static str>,
}
pub static KNOWN_CRATE_REGISTRY: Lazy<HashMap<&'static str, KnownCrateRegistryValue>> = Lazy::new(
    || {
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
                        ld_library_path_inputs: $ld_library_path_inputs.into_iter().collect(),
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
        crate_to_build_inputs!(m, "libz-sys", ["zlib", "cmake" /* For `zlib-ng` feature */]);
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
        crate_to_build_inputs!(m, "zstd-sys", ["zlib", "clang" /* For `bindgen` feature */]);
        m
    },
);
