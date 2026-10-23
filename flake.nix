{
  description = "Moon-Beauty-Shell: High-performance Rust & Iced desktop shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        runtimeLibs = with pkgs; [
          wayland
          wayland-protocols
          libxkbcommon
          libGL
          vulkan-loader
          fontconfig
          freetype
          pipewire
          pam
          tesseract
          imagemagick
          wl-clipboard
          pciutils
          dbus
          nerd-fonts.symbols-only
        ];
      in {
        devShells.default = pkgs.mkShell {
          name = "moon-beauty-shell-dev";
          nativeBuildInputs = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            pkg-config
            stdenv.cc
            gcc
            binutils
          mold
          lld
          ];
          buildInputs = runtimeLibs;
          shellHook = ''
            export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath runtimeLibs}:/run/opengl-driver/lib:/run/opengl-driver-32/lib''${LD_LIBRARY_PATH:+:''$LD_LIBRARY_PATH}"
          '';
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "moon-beauty-shell";
          version = "0.2.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          nativeBuildInputs = with pkgs; [ pkg-config stdenv.cc mold lld ];
          buildInputs = runtimeLibs;
        };
      });
}
