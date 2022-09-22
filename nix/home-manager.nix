{ flake }:
{ config, lib, pkgs, ... }:

let
  inherit (lib) literalExpression mkEnableOption mkIf mkMerge mkOption types;
  cfg = config.programs.riff;
in
{
  options.programs.riff = {
    enable = mkEnableOption "Riff";
    enableDirenvIntegration = mkEnableOption "use_riff direnv integration";
    enableWrapper = mkEnableOption "Wrap Riff with Cargo" // { default = true; };
    offline = mkEnableOption "Riff offline mode";
    telemetry = mkEnableOption "Riff telemetry" // { default = true; };
    cargoPackage = mkOption {
      type = types.package;
      default = pkgs.cargo;
      defaultText = literalExpression "pkgs.cargo";
      description = "Cargo binary to include when enableWrapper is true";
    };
    package = mkOption {
      type = types.package;
      default = flake.packages.${pkgs.system}.riff;
      defaultText = literalExpression ''inputs.riff.packages.''${system}.riff'';
      description = "Package for Riff CLI";
    };
    finalPackage = mkOption {
      type = types.package;
      visible = false;
      readOnly = true;
      description = "Possibly-wrapped Riff package";
    };
  };

  config = mkIf cfg.enable {
    home.packages = [ cfg.finalPackage ];
    home.sessionVariables = mkMerge [
      (mkIf (!cfg.telemetry) { RIFF_DISABLE_TELEMETRY = true; })
      (mkIf cfg.offline { RIFF_OFFLINE = true; })
    ];

    programs.direnv.stdlib = mkIf cfg.enableDirenvIntegration ''
      use_riff() {
        watch_file Cargo.toml watch_file Cargo.lock
        eval "$(riff print-dev-env)"
      }
    '';

    programs.riff.finalPackage =
      if cfg.enableWrapper then
        pkgs.symlinkJoin
          {
            name = "riff-wrapped";
            paths = [ cfg.package cfg.cargoPackage ];
            nativeBuildInputs = [ pkgs.makeWrapper ];
            postFixup = ''
              wrapProgram $out/bin/riff \
                --set PATH ${lib.makeBinPath [ cfg.cargoPackage ]}
            '';
          } else cfg.package;
  };
}
