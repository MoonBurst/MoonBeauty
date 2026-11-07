//! --restart-bar: restart the bar with orphan detection.
//!
//! Three-path restart logic:
//!   1. systemctl user unit active → kill ppid=1 orphans, then delegate
//!   2. no unit → pkill bar + watchers + tooltips, then respawn
//!   3. exit immediately if --restart-bar absent

use moonbeauty::libraries::process::resolve_exe;

pub fn run(args: &[String]) {
    // Restart bar (used by Settings when changing monitor)
    if args.iter().any(|a| a == "--restart-bar") {
        // F1: Under systemd with Restart=always, pkill+spawn races the
        // service auto-restart and produces two bars for ~1s. But an
        // orphaned bar (parent died → re-parented to PID 1, no unit,
        // `systemctl status` says not-found) is invisible to every
        // scope-aware check. So:
        //   1. Kill orphans first (ppid=1, not under any unit)
        //   2. Delegate to systemctl if a unit is active
        //   3. Fall through to pkill+spawn otherwise
        let user_active = std::process::Command::new("systemctl")
            .args(["--user", "is-active", "--quiet", "moonbeauty.service"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if user_active {
            // Kill orphans reparented to PID 1 before delegating.
            if let Ok(out) = std::process::Command::new("pgrep")
                .args(["-f", "(^|/)(moonbeauty|moon-beauty-shell)$"])
                .output()
            {
                for line in String::from_utf8_lossy(&out.stdout).lines() {
                    let pid = line.trim();
                    if pid.parse::<u32>().is_err() {
                        continue;
                    }
                    let ppid = std::process::Command::new("ps")
                        .args(["-o", "ppid=", "-p", pid])
                        .output()
                        .ok()
                        .and_then(|o| {
                            String::from_utf8_lossy(&o.stdout)
                                .trim()
                                .parse::<u32>()
                                .ok()
                        });
                    if ppid == Some(1) {
                        eprintln!(
                            "[restart-bar] killing orphan bar pid={} (ppid=1, no unit)",
                            pid
                        );
                        let _ = std::process::Command::new("kill")
                            .args(["-9", pid])
                            .status();
                    }
                }
            }
            eprintln!("[restart-bar] delegating to systemctl --user restart moonbeauty.service");
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "restart", "moonbeauty.service"])
                .status();
            std::process::exit(0);
        }

        // Kill orphaned clipboard watchers FIRST. Their cmdline ends with
        // "--clipboard-ingest" (not "moonbeauty$"), so the
        // anchored shell pkill below never matches them. Without this,
        // every restart spawns a new watcher and the clipboard UI ends
        // up with N copies of each item after N restarts.
        let _ = std::process::Command::new("pkill")
            .args(["-f", "wl-paste --watch.*moonbeauty"])
            .status();
        // Finding 22: anchored patterns — require path-prefix before the
        // binary name and end-of-argv after the flag. Prevents matching
        // scripts / editors that merely mention the strings.
        let _ = std::process::Command::new("pkill")
            .args(["-f", "(^|/)(moonbeauty|moon-beauty-shell)$"])
            .status();
        let _ = std::process::Command::new("pkill")
            .args([
                "-f",
                "(^|/)(moonbeauty|moon-beauty-shell) (--(sys|weather|alarm|info)-tooltip|--tray-popup)$",
            ])
            .status();
        std::thread::sleep(std::time::Duration::from_millis(400));
        let exe = resolve_exe();
        let mut cmd = std::process::Command::new(exe);
        let _ = moonbeauty::libraries::process::spawn_and_reap(&mut cmd);
        std::process::exit(0);
    }
}
