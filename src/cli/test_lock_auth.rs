pub fn run(args: &[String]) {
    // ── Lock auth test mode ──
    if args.iter().any(|a| a == "--test-lock-auth") {
        moonbeauty::overlays::lockscreen::LockCoordinator::run_auth_test();
    }
}
