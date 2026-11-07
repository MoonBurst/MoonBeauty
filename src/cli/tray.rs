//! --tray-context-menu <json> <bus> <menu_path> <w> <h>.
//! Spawns the tray-context child process.


pub fn run(args: &[String]) {
    if let Some(idx) = args.iter().position(|a| a == "--tray-context-menu") {
        let json_path = args.get(idx + 1).cloned().unwrap_or_default();
        let bus = args.get(idx + 2).cloned().unwrap_or_default();
        let menu_path = args.get(idx + 3).cloned().unwrap_or_default();
        let w = args
            .get(idx + 4)
            .and_then(|s| s.parse().ok())
            .unwrap_or(250u32);
        let h = args
            .get(idx + 5)
            .and_then(|s| s.parse().ok())
            .unwrap_or(400u32);
        if let Ok(json) = std::fs::read_to_string(&json_path) {
            if let Ok(items) =
                serde_json::from_str::<Vec<moonbeauty::bar::tray::TrayMenuItem>>(&json)
            {
                let _ = std::fs::remove_file(&json_path);
                std::process::exit(
                    match moonbeauty::bar::tray::context::run(items, bus, menu_path, w, h) {
                        Ok(_) => 0,
                        Err(_) => 1,
                    },
                );
            }
        }
        std::process::exit(1);
    }
}
