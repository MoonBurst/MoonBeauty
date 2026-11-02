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

        # Shared library list — patchelf rpaths the binary against these
        # so no LD_LIBRARY_PATH wrapper is needed at runtime.
        linkLibs = p: with p; [
          wayland
          wayland-protocols
          libxkbcommon
          libGL
          vulkan-loader
          fontconfig
          freetype
          pipewire
          pam
          dbus
          openssl
        ];

        # External CLI tools the shell invokes via Command::new().
        # Added to PATH via wrapProgram at build time, so the systemd
        # service doesn't need to know about them.
        runtimeTools = p: with p; [
          grim              # screenshots
          slurp             # region selection
          sway              # swaymsg for output queries
          wl-clipboard      # wl-copy / wl-paste
          libnotify         # notify-send
          imagemagick       # watermark / annotation compose
          libjxl            # cjxl/djxl for JXL encode/decode
          libjxl            # cjxl/djxl for JXL encode/decode
          tesseract         # OCR
          curl              # weather
          pipewire          # pw-play
          pulseaudio        # pactl fallback
          pciutils          # lspci for GPU detection
          iproute2          # ss for net procs
          procps            # kill / ps
          xdg-utils         # xdg-open
          util-linux        # script for sudo PTY
          pass              # password manager
          pamtester         # lockscreen auth
          xdg-desktop-portal
          xdg-desktop-portal-gtk
          sops              # secrets encryption
          age               # sops age backend
          gnupg             # sops gpg backend
        ];

        # Source tree pruned of build artifacts + VCS noise.
        source = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: _type:
            !(builtins.elem (baseNameOf path) [ "target" ".git" ".direnv" ]);
        };

        mkShellPackage = p: p.rustPlatform.buildRustPackage {
          pname = "moon-beauty-shell";
          version = "0.2.0";
          src = source;
          cargoLock = { lockFile = ./Cargo.lock; };

          nativeBuildInputs = with p; [
            pkg-config
            stdenv.cc
            mold
            lld
            makeWrapper
          ];
          buildInputs = linkLibs p;

          # openssl-sys needs an explicit hint in the hermetic Nix sandbox.
          # Belt-and-suspenders alongside pkg-config: OPENSSL_DIR for
          # the direct-lookup path, OPENSSL_NO_VENDOR to fail cleanly
          # (not attempt a network-vendored build needing perl).
          env = {
            OPENSSL_DIR = "${p.openssl.dev}";
            OPENSSL_LIB_DIR = "${p.openssl.out}/lib";
            OPENSSL_INCLUDE_DIR = "${p.openssl.dev}/include";
            OPENSSL_NO_VENDOR = "1";
          };

          doCheck = true;
          # Restrict test phase to unit tests. Cargo's default checkPhase
          # runs bare `cargo test`, which includes doctests. plate_watermark.rs
          # has shell-command examples in doc comments that Rustdoc tries to
          # compile as Rust. The security-relevant tests are all --lib unit
          # tests, so restrict here.
          checkPhase = "cargo test --release --lib";
          preCheck = ''
            unset XDG_RUNTIME_DIR XDG_DATA_HOME XDG_CACHE_HOME XDG_CONFIG_HOME || true
          '';

          postInstall = ''
            # Hardening: refuse to package if lock-debug-bypass is compiled in.
            if grep -aqF 'DEBUG BYPASS (compile-time feature)' $out/bin/moon-beauty-shell; then
              echo "FATAL: lock-debug-bypass feature compiled into release binary" >&2
              echo "       Rebuild without --features lock-debug-bypass." >&2
              exit 1
            fi

            wrapProgram $out/bin/moon-beauty-shell \
              --prefix PATH : ${p.lib.makeBinPath (runtimeTools p)}
          '';

          meta = with p.lib; {
            description = "High-performance Wayland desktop shell and overlay matrix";
            mainProgram = "moon-beauty-shell";
            platforms = platforms.linux;
          };
        };
      in {
        packages.default = mkShellPackage pkgs;
        packages.moon-beauty-shell = mkShellPackage pkgs;

        overlays.default = final: _prev: {
          moon-beauty-shell = mkShellPackage final;
        };

        devShells.default = pkgs.mkShell {
          name = "moon-beauty-shell-dev";
          nativeBuildInputs = with pkgs; [
            rustc cargo rustfmt clippy pkg-config
            stdenv.cc gcc binutils mold lld
            cargo-audit   # run: cargo audit
          ];
          buildInputs = (linkLibs pkgs) ++ (runtimeTools pkgs);
          shellHook = ''
            # Bake rpath at link time instead of exporting LD_LIBRARY_PATH.
            # LD_LIBRARY_PATH forces glibc's loader to probe every entry for
            # every library — ~700 failed openat() calls per spawn, ~130 ms
            # of wasted disk I/O. rpath uses ld.so.cache, one probe per lib.
                        export OPENSSL_DIR=${pkgs.openssl.dev}
            export OPENSSL_LIB_DIR=${pkgs.openssl.out}/lib
            export OPENSSL_INCLUDE_DIR=${pkgs.openssl.dev}/include
export NIX_RPATH_PATHS="${pkgs.lib.makeLibraryPath (linkLibs pkgs)}:/run/opengl-driver/lib:/run/opengl-driver-32/lib"
            export RUSTFLAGS="-C link-arg=-fuse-ld=mold -C link-arg=-Wl,--enable-new-dtags,-rpath,$NIX_RPATH_PATHS"
          '';
        };

        # ── Home Manager module ─────────────────────────────────────
        # Usage in home.nix:
        #   imports = [ inputs.moon-beauty-shell.homeManagerModules.default ];
        #   services.moon-beauty-shell.enable = true;
        homeManagerModules.default = { config, lib, pkgs, ... }:
          let cfg = config.services.moon-beauty-shell; in {
            options.services.moon-beauty-shell = {
              enable = lib.mkEnableOption "Moon Beauty Shell (bar, tooltips, overlays)";

              harden = lib.mkOption {
                type = lib.types.bool;
                default = true;
                description = ''
                  Strip MBS_DEBUG and MBS_LOCK_DEBUG_PASS from the
                  service environment via systemd UnsetEnvironment.
                '';
              };

              package = lib.mkOption {
                type = lib.types.package;
                default = self.packages.${pkgs.system}.default;
                description = "The moon-beauty-shell package to run as a service";
              };

              output = lib.mkOption {
                type = lib.types.str;
                default = "";
                description = ''
                  Wayland output to bind the bar to (e.g. "DP-1").
                  Empty string → shell picks primary/focused output itself.
                '';
              };

              notifDaemon = lib.mkOption {
                type = lib.types.bool;
                default = true;
                description = ''
                  Install the D-Bus activation service for org.freedesktop.Notifications
                  so this shell handles desktop notifications, lazily spawned on demand.
                  Set false if you use dunst, mako, or swaync instead.
                '';
              };
            };

            config = lib.mkIf cfg.enable {
              home.packages = [ cfg.package ];

              # Lazy notification daemon: D-Bus activates us on the first Notify(),
              # we exit after 30s idle. Saves ~55 MB at rest vs eager-spawning.
              xdg.dataFile."dbus-1/services/org.freedesktop.Notifications.service" =
                lib.mkIf cfg.notifDaemon {
                  text = ''
                    [D-BUS Service]
                    Name=org.freedesktop.Notifications
                    Exec=${cfg.package}/bin/moon-beauty-shell --notif-surface
                  '';
                };

              systemd.user.services.moon-beauty-shell = {
                Unit = {
                  Description = "Moon Beauty Shell";
                  After = [ "graphical-session.target" ];
                  PartOf = [ "graphical-session.target" ];
                };

                Service = {
                  ExecStart = "${cfg.package}/bin/moon-beauty-shell";
                  Restart = "always";
                  RestartSec = 2;
                  # Default KillMode=control-group reaps notif-surface
                  # and tray-watcher threads cleanly on restart.
                  Environment = [
                    "RUST_BACKTRACE=1"
                  ] ++ lib.optionals (cfg.output != "") [
                    "MBS_OUTPUT=${cfg.output}"
                  ];

                  UnsetEnvironment = lib.optionals cfg.harden [
                    "MBS_DEBUG"
                    "MBS_LOCK_DEBUG_PASS"
                  ];
                };

                Install.WantedBy = [ "graphical-session.target" ];
              };
            };
          };

        # ── NixOS module ────────────────────────────────────────────
        # Usage in configuration.nix:
        #   imports = [ inputs.moon-beauty-shell.nixosModules.default ];
        #   services.moon-beauty-shell.enable = true;
        nixosModules.default = { config, lib, pkgs, ... }:
          let cfg = config.services.moon-beauty-shell; in {
            options.services.moon-beauty-shell = {
              enable = lib.mkEnableOption "Moon Beauty Shell for all users";

              harden = lib.mkOption {
                type = lib.types.bool;
                default = true;
                description = ''
                  Strip MBS_DEBUG and MBS_LOCK_DEBUG_PASS from the
                  service environment via systemd UnsetEnvironment.
                '';
              };

              package = lib.mkOption {
                type = lib.types.package;
                default = self.packages.${pkgs.system}.default;
              };

              notifDaemon = lib.mkOption {
                type = lib.types.bool;
                default = true;
                description = ''
                  Install the D-Bus activation service for org.freedesktop.Notifications
                  system-wide. Set false if you use dunst, mako, or swaync.
                '';
              };
            };

            config = lib.mkIf cfg.enable {
              environment.systemPackages = [
                cfg.package
              ] ++ lib.optional cfg.notifDaemon (pkgs.writeTextDir
                "share/dbus-1/services/org.freedesktop.Notifications.service"
                ''
                  [D-BUS Service]
                  Name=org.freedesktop.Notifications
                  Exec=${cfg.package}/bin/moon-beauty-shell --notif-surface
                ''
              );

              # Sway must import WAYLAND_DISPLAY + SWAYSOCK into systemd
              # --user so the service inherits them, then start the unit.
              # This snippet lands in /etc/sway/config.d/ and is merged
              # into the effective Sway config at startup.
              environment.etc."sway/config.d/50-moon-beauty-shell.conf".text = ''
                exec systemctl --user import-environment WAYLAND_DISPLAY SWAYSOCK XDG_CURRENT_DESKTOP
                exec_always systemctl --user start moon-beauty-shell.service
              '';

              systemd.user.services.moon-beauty-shell = {
                Unit = {
                  Description = "Moon Beauty Shell";
                  After = [ "graphical-session.target" ];
                  PartOf = [ "graphical-session.target" ];
                };
                Service = {
                  ExecStart = "${cfg.package}/bin/moon-beauty-shell";
                  Restart = "always";
                  RestartSec = 2;
                  Environment = [ "RUST_BACKTRACE=1" ];

                  UnsetEnvironment = lib.optionals cfg.harden [
                    "MBS_DEBUG"
                    "MBS_LOCK_DEBUG_PASS"
                  ];
                };
                Install.WantedBy = [ "graphical-session.target" ];
              };
            };
          };
      });
}
