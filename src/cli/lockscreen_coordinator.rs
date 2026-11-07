pub fn run(args: &[String]) {
    // ── Multi-monitor lock coordinator (parent mode) ──
    // If called as `--lockscreen` (and NOT `--lockscreen-instance`),
    // act as the coordinator: spawn one child per output, broker auth,
    // watch hotplug. Blocks forever (exits via process::exit).
    if args.iter().any(|a| a == "--lockscreen")
        && !args.iter().any(|a| a == "--lockscreen-instance")
    {
        moonbeauty::overlays::lockscreen::LockCoordinator::spawn();
    }
}
