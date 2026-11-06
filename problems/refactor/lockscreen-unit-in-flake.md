# Ship `moonbeauty-lockscreen.service` via `flake.nix`

**Effort:** 15m
**Subsystem:** flake / systemd
**Depends on:** —
**Good first issue:** yes

## Current state

`moonbeauty-lockscreen.service` currently lives only in
`~/.config/systemd/user/` on hosts where it was installed manually.
Users who import `homeManagerModules.default` or
`nixosModules.default` do not get it, so `--toggle lockscreen` fails
silently (or worse, falls back to old behavior if the code is out of
sync — but the current code does not fall back).

## Desired state

Both modules in `flake.nix` add the unit:

    systemd.user.services.moonbeauty-lockscreen = {
      Unit = {
        Description = "Moon Beauty Lockscreen Coordinator";
        After = [ "graphical-session.target" ];
      };
      Service = {
        ExecStart = "${cfg.package}/bin/moonbeauty --lockscreen";
        Restart = "on-failure";
        RestartSec = "500ms";
        Environment = [ "RUST_BACKTRACE=1" ];
      };
      Install = { };  # Intentionally no WantedBy — started on demand
    };

Same for both `homeManagerModules.default` and `nixosModules.default`.

Documentation: add a note to the module `description` that
`--toggle lockscreen` requires this unit, and that the module
installs it automatically.

## Verification

- `nix build .#` succeeds
- `nixos-rebuild switch --flake .#` or `home-manager switch --flake .#`
  installs the unit
- `systemctl --user cat moonbeauty-lockscreen.service` shows the unit
- `moonbeauty --toggle lockscreen` locks the screen

## Out of scope

Removing the dev-only `%h/projects/moonbeauty/target/release/moonbeauty`
path in the current local unit file — the flake-package path
(`${cfg.package}/bin/moonbeauty`) is the correct one for shipped units.
