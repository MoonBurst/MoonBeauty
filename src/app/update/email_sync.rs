use super::*;

impl MoonBeautyShellApp {
    pub(crate) fn handle_tick_email_sync(&mut self) -> Command<Message> {
        // Only the primary bar and the standalone email overlay
        // sync IMAP. The notif-surface doesn't need email data and
        // duplicating the sync doubles IMAP traffic + log noise.
        if self.is_notif_surface {
            return Command::none();
        }
        if moonbeauty::libraries::backend::email::client::config_exists() {
            eprintln!("[sync] Boot/Timer triggered email sync");
            let folder = "INBOX".to_string();
            return Command::perform(
                async move {
                    let old_envs =
                        moonbeauty::libraries::backend::email::client::load_index(&folder);
                    if let Ok(new_envs) =
                        moonbeauty::libraries::backend::email::client::fetch_from_network(&folder)
                            .await
                    {
                        moonbeauty::libraries::backend::email::client::save_index(
                            &folder, &new_envs,
                        );
                        moonbeauty::libraries::backend::email::client::sync_bodies_background(
                            folder,
                        )
                        .await;
                        Ok::<_, String>((old_envs, new_envs))
                    } else {
                        Err("fetch failed".into())
                    }
                },
                |res| match res {
                    Ok((old, new)) => Message::EmailSyncedWithDiff(old, new),
                    Err(_) => Message::EmailSynced,
                },
            );
        }

        Command::none()
    }

    pub(crate) fn handle_email_synced_with_diff(
        &mut self,
        old: Vec<moonbeauty::libraries::backend::email::client::NativeEnvelope>,
        new: Vec<moonbeauty::libraries::backend::email::client::NativeEnvelope>,
    ) -> Command<Message> {
        eprintln!(
            "[sync] EmailSyncedWithDiff: old={} new={}",
            old.len(),
            new.len()
        );

        // Cross-process dedup file: prevent two processes from notifying the same email
        let dedup_path = dirs::runtime_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("/tmp"))
            .join("mbs-email-notified.txt");
        let mut notified: std::collections::HashSet<String> = std::fs::read_to_string(&dedup_path)
            .ok()
            .map(|s| {
                s.lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let old_ids: std::collections::HashSet<String> = old.iter().map(|e| e.id.clone()).collect();
        for env in new {
            if !old_ids.contains(&env.id) && !notified.contains(&env.id) {
                eprintln!("[sync] New email from {}: {}", env.sender, env.subject);

                // Visual notification
                let _ = moonbeauty::libraries::process::spawn_and_reap(
                    std::process::Command::new("notify-send").args([
                        "-a",
                        "Email",
                        "--",
                        &env.sender,
                        &env.subject,
                    ]),
                );

                // Spoken announcement
                moonbeauty::overlays::notifications::NotificationOverlay::speak_tts(
                    "Email",
                    &env.sender,
                    &env.subject,
                    Some("New email from {sender}: {body}"),
                    self.settings.tts_command.clone(),
                    self.settings.notif_volume,
                );

                notified.insert(env.id.clone());
            }
        }

        // Persist dedup list (cap at 200 most recent)
        let mut sorted: Vec<_> = notified.into_iter().collect();
        sorted.sort();
        sorted.truncate(200);
        let _ = moonbeauty::libraries::process::private_write_file(
            &dedup_path,
            sorted.join("\n").as_bytes(),
        );

        Command::none()
    }

    pub(crate) fn handle_email_synced(&mut self) -> Command<Message> {
        Command::none()
    }
}
