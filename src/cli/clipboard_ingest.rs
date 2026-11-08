/// Restrict URI-list ingestion to known-safe roots. Any same-user
/// process can `wl-copy file:///home/user/.ssh/id_rsa`, and the
/// clipboard cache would persist a 0600 copy. This confines ingestion
/// to content dirs (Pictures/Downloads/Documents/Desktop/Videos/Music)
/// and the shell's own mbs-tmp scratch. Symlinks are resolved via
/// canonicalize before the check; a symlink inside a safe root that
/// points outside is rejected.
fn is_safe_ingest_root(path: &std::path::Path) -> bool {
    let Ok(canon) = std::fs::canonicalize(path) else {
        return false;
    };
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    if let Some(home) = dirs::home_dir() {
        for sub in ["Pictures", "Downloads", "Documents", "Desktop", "Videos", "Music"] {
            roots.push(home.join(sub));
        }
    }
    if let Ok(rt) = std::env::var("XDG_RUNTIME_DIR") {
        roots.push(std::path::PathBuf::from(rt).join("mbs-tmp"));
    }
    roots.iter().any(|root| {
        std::fs::canonicalize(root)
            .ok()
            .is_some_and(|cr| canon.starts_with(&cr))
    })
}

pub fn run(args: &[String]) {
    if args.iter().any(|a| a == "--clipboard-ingest") {
        use std::io::Read;
        // 100 MB cap. stdin is untrusted (any same-user process can
        // pipe to wl-copy). Without this, `yes | wl-copy` buffers GB
        // into memory before we can react. Truncate silently — the
        // head of the paste is what the user wants.
        const MAX_INGEST: u64 = 100 * 1024 * 1024;
        let mut bytes = Vec::new();
        let mut limited = std::io::Read::take(std::io::stdin(), MAX_INGEST + 1);
        if limited.read_to_end(&mut bytes).is_ok() && !bytes.is_empty() {
            if bytes.len() as u64 > MAX_INGEST {
                bytes.truncate(MAX_INGEST as usize);
                eprintln!(
                    "[clipboard-ingest] truncated to {} MB",
                    MAX_INGEST / (1024 * 1024)
                );
            }
            let dir = dirs::data_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("moonbeauty")
                .join("clipboard");
            let _ = std::fs::create_dir_all(&dir);
            let index_path = dir.join("index.json");

            let id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            // Detect images by Magic Bytes (PNG, JPEG, GIF, WebP)
            // JPEG XL: either the 12-byte container signature or a bare
            // codestream (starts with 0xFF 0x0A).
            let is_jxl = bytes.starts_with(&[0xFF, 0x0A])
                || bytes.starts_with(&[
                    0x00, 0x00, 0x00, 0x0C, 0x4A, 0x58, 0x4C, 0x20, 0x0D, 0x0A, 0x87, 0x0A,
                ]);
            let is_image = is_jxl
                || bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47])
                || bytes.starts_with(&[0xFF, 0xD8, 0xFF])
                || bytes.starts_with(b"GIF89a")
                || bytes.starts_with(b"GIF87a")
                || (bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP");

            let ext = if is_jxl {
                "jxl"
            } else if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
                "png"
            } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
                "jpg"
            } else if bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a") {
                "gif"
            } else if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
                "webp"
            } else {
                "txt"
            };
            let path = dir.join(format!("{}.{}", id, ext));
            let _ = moonbeauty::libraries::process::private_write_file(&path, &bytes);

            // Pre-generate the 512x512 preview so the long-running UI
            // process never has to decode the full 4K image later.
            if is_image {
                let _ = moonbeauty::overlays::clipboard::ensure_image_preview(&path);
            }

            // URI-list ingest (annotator saves, file-manager copies).
            if bytes.starts_with(b"file:") {
                let text = String::from_utf8_lossy(&bytes);
                let mut ingested = 0usize;
                for line in text.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }
                    let path_str = line.strip_prefix("file://").unwrap_or(line);
                    let path = std::path::PathBuf::from(path_str);
                    if !path.is_file() {
                        continue;
                    }
                    if !is_safe_ingest_root(&path) {
                        eprintln!(
                            "[clipboard-ingest] reject URI outside safe roots: {}",
                            path.display()
                        );
                        continue;
                    }
                    if moonbeauty::overlays::clipboard::ClipboardOverlay::add_entry(&path, None)
                        .is_some()
                    {
                        ingested += 1;
                        eprintln!("[clipboard-ingest] URI: {}", path.display());
                    }
                }
                eprintln!("[clipboard-ingest] URI batch: {} new", ingested);
                std::process::exit(0);
            }

            let mut items: Vec<moonbeauty::overlays::clipboard::ClipItem> = Vec::new();
            if let Ok(content) = std::fs::read_to_string(&index_path) {
                if let Ok(parsed) = serde_json::from_str(&content) {
                    items = parsed;
                }
            }
            let mut text_preview = if is_image {
                String::new()
            } else {
                String::from_utf8_lossy(&bytes[..bytes.len().min(100)]).to_string()
            };

            if is_image {
                let name_file = moonbeauty::libraries::process::mbs_runtime_dir()
                    .join("mbs-last-clip-name.txt");
                if let Ok(txt) = std::fs::read_to_string(&name_file) {
                    text_preview = txt.trim().to_string();
                    let _ = std::fs::remove_file(&name_file);
                }
            }

            // Dedupe by content hash. `wl-paste --watch` can fire more
            // than once per wl-copy (fork-mode ownership change), which
            // previously produced N identical rows for one clipboard
            // write. Compare against the TOP entry only — inserting a
            // genuinely-new occurrence of the same content later still
            // works because it lands at items[0] and pushes the old one
            // down.
            let incoming_hash: String = {
                use sha2::Digest as _;
                format!("{:x}", sha2::Sha256::digest(&bytes))
            };

            // Backfill top entry's hash if missing (one-shot migration
            // for rows written before this patch).
            if let Some(top) = items.first_mut() {
                if top.content_hash.is_none() {
                    if let Some(p) =
                        moonbeauty::overlays::clipboard::ClipboardOverlay::get_file_path(
                            &dir, top.id,
                        )
                    {
                        if let Ok(b) = std::fs::read(&p) {
                            use sha2::Digest as _;
                            top.content_hash = Some(format!("{:x}", sha2::Sha256::digest(&b)));
                        }
                    }
                }
            }

            if items.first().and_then(|t| t.content_hash.as_deref()) == Some(incoming_hash.as_str())
            {
                eprintln!("[clipboard-ingest] duplicate content, skipping insert");
                // The mbs-last-clip-name.txt we may have read from the
                // first fire is already consumed; nothing left to do.
                std::process::exit(0);
            }

            items.insert(
                0,
                moonbeauty::overlays::clipboard::ClipItem {
                    id,
                    text: text_preview,
                    is_image,
                    content_hash: Some(incoming_hash),
                    source_path: None,
                    ext: Some(ext.to_string()),
                },
            );
            items.truncate(2000);

            if let Ok(json) = serde_json::to_string(&items) {
                let tmp = dir.join("index.json.tmp");
                if moonbeauty::libraries::process::private_write_file(&tmp, json.as_bytes()).is_ok()
                {
                    let _ = std::fs::rename(&tmp, &index_path);
                    // Purge AFTER successful write: if the write fails,
                    // the old index still references files we would
                    // have deleted.
                    moonbeauty::overlays::clipboard::ClipboardOverlay::purge_orphans(&dir, &items);
                    eprintln!(
                        "[clipboard-ingest] Saved {} items, is_image={}",
                        items.len(),
                        is_image
                    );
                }
            }
        }
        std::process::exit(0);
    }
}
