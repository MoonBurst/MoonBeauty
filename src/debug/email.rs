pub fn run(args: &[String]) {
    // ── Email connection test (one-shot diagnostic) ──
    if args.iter().any(|a| a == "--test-email") {
        let rt = tokio::runtime::Runtime::new().expect("tokio rt");
        rt.block_on(async {
            match moonbeauty::libraries::backend::email::client::test_connection().await {
                Ok((imap, smtp, report)) => {
                    println!("{}", report);
                    if imap && smtp {
                        println!("\n✅ both imap and smtp OK");
                        std::process::exit(0);
                    } else {
                        println!("\n❌ imap={} smtp={}", imap, smtp);
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("test failed: {}", e);
                    std::process::exit(2);
                }
            }
        });
    }
}
