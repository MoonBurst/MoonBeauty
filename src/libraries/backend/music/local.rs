//! Native local music backend — replaces `mpd_client` with `rodio` + `lofty`.
//!
//! Architecture:
//!   - Library discovery scans `ShellSettings::load().music_dirs` (falling back
//!     to `dirs::audio_dir()` / `~/Music`) and caches metadata to
//!     `$XDG_CACHE_HOME/moonbeauty/music_index.json`.
//!   - Playback state (queue, active track path, position, volume) is saved to
//!     `$XDG_CACHE_HOME/moonbeauty/music_state.json` so daemon restarts resume
//!     the exact track and timestamp instead of resetting to track 0.
//!   - Audio playback runs in a self-managed `--audio-daemon` subprocess
//!     launched into `app.slice` via `systemd-run` (with `spawn_and_reap`
//!     fallback) so playback survives bar exits and restarts uninterrupted.
//!   - `rodio::MixerDeviceSink` is `!Send`/`!Sync` and lives exclusively on a
//!     dedicated audio thread inside the daemon.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::tag::Accessor;
use rodio::{DeviceSinkBuilder, Player};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::mpris::MprisTrack;

const AUDIO_EXTS: &[&str] = &["mp3", "flac", "ogg", "opus", "m4a", "wav", "aac"];
const CACHE_TTL_SECS: u64 = 300;
const MAX_INDEX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_STATE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_IPC_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexedTrack {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_sec: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MusicIndexCache {
    #[serde(default)]
    music_dirs: Vec<String>,
    scanned_at_epoch: u64,
    tracks: Vec<IndexedTrack>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SavedMusicState {
    #[serde(default)]
    queue_paths: Vec<String>,
    #[serde(default)]
    current_path: Option<String>,
    #[serde(default)]
    position_sec: f64,
    #[serde(default = "default_volume_pct")]
    volume_pct: u8,
    #[serde(default)]
    is_playing: bool,
}

fn default_volume_pct() -> u8 {
    100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum AudioCmd {
    Poll,
    Play,
    Pause,
    TogglePlayPause,
    Next,
    Previous,
    Seek(f64),
    SetVolume(u8),
    PlayQueue {
        urls: Vec<String>,
        start_index: usize,
    },
    ReloadLibrary,
}

enum ThreadMsg {
    Client(AudioCmd, mpsc::SyncSender<AudioResp>),
    DefaultQueue(Vec<IndexedTrack>),
    ReloadedQueue(Vec<IndexedTrack>),
    Shutdown(mpsc::SyncSender<()>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DaemonTrackState {
    title: String,
    artist: String,
    album: String,
    is_playing: bool,
    position_sec: f64,
    length_sec: f64,
    volume_pct: u8,
    url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum AudioResp {
    Ok,
    State(DaemonTrackState),
    Err(String),
}

fn default_music_dir() -> PathBuf {
    if let Some(dir) = dirs::audio_dir() {
        return dir;
    }
    dirs::home_dir()
        .map(|h| h.join("Music"))
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn expand_tilde(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return dirs::home_dir().unwrap_or_else(|| PathBuf::from(trimmed));
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(trimmed)
}

pub fn get_music_dirs() -> Vec<PathBuf> {
    let settings = crate::libraries::config::ShellSettings::load();
    let mut dirs_out = Vec::new();
    let mut seen = HashSet::new();
    for raw in &settings.music_dirs {
        let t = raw.trim();
        if t.is_empty() {
            continue;
        }
        let p = expand_tilde(t);
        if seen.insert(p.clone()) {
            dirs_out.push(p);
        }
    }
    if dirs_out.is_empty() {
        dirs_out.push(default_music_dir());
    }
    dirs_out
}

pub fn get_music_dir() -> PathBuf {
    get_music_dirs()
        .into_iter()
        .next()
        .unwrap_or_else(default_music_dir)
}

fn index_cache_path() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(crate::libraries::process::mbs_runtime_dir);
    base.join("moonbeauty").join("music_index.json")
}

fn music_state_path() -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(crate::libraries::process::mbs_runtime_dir);
    base.join("moonbeauty").join("music_state.json")
}

fn socket_path() -> PathBuf {
    crate::libraries::process::mbs_runtime_dir().join("moonbeauty-audio.sock")
}

fn daemon_log_path() -> PathBuf {
    crate::libraries::process::mbs_runtime_dir().join("mbs-audio-daemon.log")
}

fn now_epoch_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|ext| AUDIO_EXTS.iter().any(|a| ext.eq_ignore_ascii_case(a)))
}

fn fallback_title(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

fn extract_metadata(path: &Path) -> IndexedTrack {
    let mut title = String::new();
    let mut artist = String::new();
    let mut album = String::new();
    let mut duration_sec = 0.0;

    if let Ok(tagged) = lofty::read_from_path(path) {
        duration_sec = tagged.properties().duration().as_secs_f64();
        if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
            if let Some(t) = tag.title() {
                title = t.trim().to_string();
            }
            if let Some(a) = tag.artist() {
                artist = a.trim().to_string();
            }
            if let Some(al) = tag.album() {
                album = al.trim().to_string();
            }
        }
    }

    if title.is_empty() {
        title = fallback_title(path);
    }

    IndexedTrack {
        path: path.to_string_lossy().into_owned(),
        title,
        artist,
        album,
        duration_sec,
    }
}

fn scan_music_library(music_dirs: &[PathBuf]) -> Vec<IndexedTrack> {
    let mut tracks = Vec::new();
    let mut seen_files = HashSet::new();

    for dir in music_dirs {
        if !dir.exists() {
            continue;
        }
        for entry in WalkDir::new(dir).follow_links(true).into_iter().flatten() {
            let p = entry.path();
            if p.is_file() && is_audio_file(p) {
                let key = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
                if seen_files.insert(key) {
                    tracks.push(extract_metadata(p));
                }
            }
        }
    }

    tracks.sort_by_key(|a| a.title.to_lowercase());
    tracks
}

fn load_or_build_index(force_rebuild: bool) -> Vec<IndexedTrack> {
    static MEM_CACHE: OnceLock<Mutex<Option<(Instant, Vec<String>, Vec<IndexedTrack>)>>> =
        OnceLock::new();
    let mem = MEM_CACHE.get_or_init(|| Mutex::new(None));

    let music_dirs = get_music_dirs();
    let dir_keys: Vec<String> = music_dirs
        .iter()
        .map(|d| d.to_string_lossy().into_owned())
        .collect();

    if !force_rebuild {
        let g = mem.lock().unwrap();
        if let Some((t, cached_dirs, tracks)) = &*g {
            if *cached_dirs == dir_keys && t.elapsed().as_secs() < 30 {
                return tracks.clone();
            }
        }
    }

    let cache_path = index_cache_path();
    if !force_rebuild {
        if let Some(cached) = crate::libraries::process::read_json_capped::<MusicIndexCache>(
            &cache_path,
            MAX_INDEX_BYTES,
        ) {
            let age = now_epoch_secs().saturating_sub(cached.scanned_at_epoch);
            if cached.music_dirs == dir_keys && age < CACHE_TTL_SECS {
                *mem.lock().unwrap() =
                    Some((Instant::now(), dir_keys, cached.tracks.clone()));
                return cached.tracks;
            }
        }
    }

    let tracks = scan_music_library(&music_dirs);
    let payload = MusicIndexCache {
        music_dirs: dir_keys.clone(),
        scanned_at_epoch: now_epoch_secs(),
        tracks: tracks.clone(),
    };

    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec(&payload) {
        let tmp = cache_path.with_extension("json.tmp");
        if crate::libraries::process::private_write_file(&tmp, &json).is_ok() {
            let _ = std::fs::rename(&tmp, &cache_path);
        }
    }

    *mem.lock().unwrap() = Some((Instant::now(), dir_keys, tracks.clone()));
    tracks
}

fn resolve_track_from_url(raw: &str, index: &[IndexedTrack]) -> Option<IndexedTrack> {
    let clean = raw.strip_prefix("file://").unwrap_or(raw);
    if clean.contains("..") {
        return None;
    }
    let candidate = if Path::new(clean).is_absolute() {
        PathBuf::from(clean)
    } else {
        get_music_dir().join(clean)
    };
    let candidate_str = candidate.to_string_lossy().into_owned();
    if let Some(hit) = index.iter().find(|t| t.path == candidate_str || t.path == clean) {
        return Some(hit.clone());
    }
    if candidate.is_file() && is_audio_file(&candidate) {
        return Some(extract_metadata(&candidate));
    }
    None
}

// ── Audio Daemon (--audio-daemon) ─────────────────────────────────────────

pub fn run_audio_daemon() -> ! {
    crate::libraries::process::install_sigterm_handler();

    let sock = socket_path();
    if UnixStream::connect(&sock).is_ok() {
        eprintln!("[audio-daemon] already running at {:?}", sock);
        std::process::exit(0);
    }
    let _ = std::fs::remove_file(&sock);

    let listener = match UnixListener::bind(&sock) {
        Ok(l) => {
            if let Ok(meta) = std::fs::metadata(&sock) {
                let mut perms = meta.permissions();
                perms.set_mode(0o600);
                let _ = std::fs::set_permissions(&sock, perms);
            }
            l
        }
        Err(e) => {
            eprintln!("[audio-daemon] bind {:?} failed: {e}", sock);
            std::process::exit(1);
        }
    };
    let _ = listener.set_nonblocking(true);

    let (cmd_tx, cmd_rx) = mpsc::channel::<ThreadMsg>();

    let tx_for_audio = cmd_tx.clone();
    std::thread::spawn(move || {
        audio_thread_loop(cmd_rx, tx_for_audio);
    });

    eprintln!("[audio-daemon] listening on {:?}", sock);
    loop {
        if crate::libraries::process::was_sigterm_signaled() {
            let (stx, srx) = mpsc::sync_channel(1);
            let _ = cmd_tx.send(ThreadMsg::Shutdown(stx));
            let _ = srx.recv_timeout(Duration::from_millis(400));
            let _ = std::fs::remove_file(&sock);
            std::process::exit(0);
        }
        match listener.accept() {
            Ok((stream, _)) => {
                let tx = cmd_tx.clone();
                std::thread::spawn(move || handle_daemon_client(stream, tx));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => {
                eprintln!("[audio-daemon] accept error: {e}");
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    }
}

fn handle_daemon_client(stream: UnixStream, tx: mpsc::Sender<ThreadMsg>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

    let Ok(cloned) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(cloned);
    let mut line = String::new();
    let mut writer = stream;

    let Ok(n) = std::io::Read::take(&mut reader, MAX_IPC_BYTES).read_line(&mut line) else {
        return;
    };
    if n == 0 {
        return;
    }

    let resp = match serde_json::from_str::<AudioCmd>(line.trim()) {
        Ok(cmd) => {
            let (rtx, rrx) = mpsc::sync_channel(1);
            if tx.send(ThreadMsg::Client(cmd, rtx)).is_ok() {
                rrx.recv_timeout(Duration::from_secs(5))
                    .unwrap_or_else(|_| AudioResp::Err("audio thread timeout".into()))
            } else {
                AudioResp::Err("audio thread exited".into())
            }
        }
        Err(e) => AudioResp::Err(format!("invalid command: {e}")),
    };

    if let Ok(mut out) = serde_json::to_string(&resp) {
        out.push('\n');
        let _ = writer.write_all(out.as_bytes());
    }
}

struct PlaybackEngine {
    _sink_handle: Option<rodio::MixerDeviceSink>,
    player: Option<Player>,
    queue: Vec<IndexedTrack>,
    current_idx: Option<usize>,
    user_paused: bool,
    pending_resume: bool,
    resume_retries: u8,
    last_resume_attempt: Instant,
    volume_pct: u8,
    pending_seek_sec: Option<f64>,
    saved_current_path: Option<String>,
    last_state_save: Instant,
}

impl PlaybackEngine {
    fn new() -> Self {
        let saved = crate::libraries::process::read_json_capped::<SavedMusicState>(
            &music_state_path(),
            MAX_STATE_BYTES,
        )
        .unwrap_or_default();

        let pending_seek_sec = if saved.position_sec > 0.5 {
            Some(saved.position_sec)
        } else {
            None
        };
        let saved_current_path = saved.current_path.clone();
        let volume_pct = saved.volume_pct.min(100);
        let pending_resume = saved.is_playing;

        let mut restored_queue = Vec::new();
        if !saved.queue_paths.is_empty() {
            let cached_index = crate::libraries::process::read_json_capped::<MusicIndexCache>(
                &index_cache_path(),
                MAX_INDEX_BYTES,
            )
            .map(|c| c.tracks)
            .unwrap_or_default();

            for raw in &saved.queue_paths {
                if let Some(t) = resolve_track_from_url(raw, &cached_index) {
                    restored_queue.push(t);
                }
            }
        }

        let current_idx = if restored_queue.is_empty() {
            None
        } else if let Some(ref cp) = saved_current_path {
            restored_queue
                .iter()
                .position(|t| &t.path == cp)
                .or(Some(0))
        } else {
            Some(0)
        };

        let sink_handle = match DeviceSinkBuilder::open_default_sink() {
            Ok(s) => Some(s),
            Err(e) => {
                eprintln!("[audio-daemon] open_default_sink failed: {e}");
                None
            }
        };

        Self {
            _sink_handle: sink_handle,
            player: None,
            queue: restored_queue,
            current_idx,
            user_paused: !pending_resume,
            pending_resume,
            resume_retries: 0,
            last_resume_attempt: Instant::now(),
            volume_pct,
            pending_seek_sec,
            saved_current_path,
            last_state_save: Instant::now(),
        }
    }

    fn save_state(&mut self) {
        self.last_state_save = Instant::now();
        let pos = self
            .player
            .as_ref()
            .map(|p| p.get_pos().as_secs_f64())
            .or(self.pending_seek_sec)
            .unwrap_or(0.0);
        let current_path = self
            .current_idx
            .and_then(|i| self.queue.get(i))
            .map(|t| t.path.clone())
            .or_else(|| self.saved_current_path.clone());
        let queue_paths: Vec<String> = self.queue.iter().map(|t| t.path.clone()).collect();
        let is_playing = self
            .player
            .as_ref()
            .is_some_and(|p| !p.is_paused() && !p.empty())
            || (self.pending_resume && !self.user_paused);

        let state = SavedMusicState {
            queue_paths,
            current_path,
            position_sec: pos,
            volume_pct: self.volume_pct,
            is_playing,
        };
        let path = music_state_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec(&state) {
            let tmp = path.with_extension("json.tmp");
            if crate::libraries::process::private_write_file(&tmp, &json).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }

    fn ensure_sink(&mut self) -> bool {
        if self._sink_handle.is_some() {
            return true;
        }
        match DeviceSinkBuilder::open_default_sink() {
            Ok(sink_handle) => {
                self._sink_handle = Some(sink_handle);
                true
            }
            Err(e) => {
                eprintln!("[audio-daemon] open_default_sink retry failed: {e}");
                false
            }
        }
    }

    fn load_and_play(&mut self, idx: usize) -> Result<(), String> {
        let track = self
            .queue
            .get(idx)
            .cloned()
            .ok_or_else(|| "queue index out of bounds".to_string())?;
        if !self.ensure_sink() {
            return Err("no audio output device available".into());
        }

        let file = File::open(&track.path).map_err(|e| format!("open {}: {e}", track.path))?;
        let source =
            rodio::Decoder::try_from(BufReader::new(file)).map_err(|e| format!("decode: {e}"))?;

        self.player = None;
        let Some(sink) = self._sink_handle.as_ref() else {
            return Err("no audio output device available".into());
        };
        let player = Player::connect_new(sink.mixer());
        player.set_volume(self.volume_pct as f32 / 100.0);
        player.append(source);
        if let Some(seek_sec) = self.pending_seek_sec.take() {
            if seek_sec > 0.5 && (track.duration_sec <= 0.0 || seek_sec < track.duration_sec) {
                let _ = player.try_seek(Duration::from_secs_f64(seek_sec));
            }
        }
        player.play();

        self.player = Some(player);
        self.current_idx = Some(idx);
        self.saved_current_path = Some(track.path);
        self.user_paused = false;
        self.pending_resume = false;
        self.save_state();
        Ok(())
    }

    fn current_state(&self) -> DaemonTrackState {
        let pos = self
            .player
            .as_ref()
            .map(|p| p.get_pos().as_secs_f64())
            .or(self.pending_seek_sec)
            .unwrap_or(0.0);
        let playing = self
            .player
            .as_ref()
            .is_some_and(|p| !p.is_paused() && !p.empty());

        if let Some(track) = self.current_idx.and_then(|i| self.queue.get(i)) {
            DaemonTrackState {
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                is_playing: playing,
                position_sec: pos,
                length_sec: track.duration_sec,
                volume_pct: self.volume_pct,
                url: Some(format!("file://{}", track.path)),
            }
        } else {
            DaemonTrackState {
                title: "Local (Idle)".into(),
                artist: "No Track".into(),
                album: String::new(),
                is_playing: false,
                position_sec: 0.0,
                length_sec: 0.0,
                volume_pct: self.volume_pct,
                url: None,
            }
        }
    }

    fn tick_auto_advance(&mut self) {
        if self.pending_resume
            && !self.user_paused
            && self.player.is_none()
            && self.current_idx.is_some()
        {
            if self.resume_retries < 10
                && self.last_resume_attempt.elapsed() >= Duration::from_millis(500)
            {
                self.last_resume_attempt = Instant::now();
                self.resume_retries += 1;
                if let Some(idx) = self.current_idx {
                    let _ = self.load_and_play(idx);
                }
            } else if self.resume_retries >= 10 {
                self.pending_resume = false;
                self.user_paused = true;
                self.save_state();
            }
            return;
        }
        let finished = self.player.as_ref().is_some_and(|p| p.empty());
        if finished && !self.user_paused {
            if let Some(idx) = self.current_idx {
                self.pending_seek_sec = None;
                if idx + 1 < self.queue.len() {
                    let _ = self.load_and_play(idx + 1);
                } else {
                    self.player = None;
                    self.user_paused = true;
                    self.current_idx = Some(0);
                    self.saved_current_path = self.queue.first().map(|t| t.path.clone());
                    self.save_state();
                }
            }
        } else if !self.user_paused && self.last_state_save.elapsed() >= Duration::from_secs(2) {
            self.save_state();
        }
    }

    fn handle_cmd(&mut self, cmd: AudioCmd, bg_tx: &mpsc::Sender<ThreadMsg>) -> AudioResp {
        match cmd {
            AudioCmd::Poll => AudioResp::State(self.current_state()),
            AudioCmd::Play => {
                if let Some(p) = &self.player {
                    if !p.empty() {
                        p.play();
                        self.user_paused = false;
                        self.pending_resume = false;
                        self.save_state();
                        return AudioResp::Ok;
                    }
                }
                if self.queue.is_empty() {
                    let tracks = load_or_build_index(false);
                    if !tracks.is_empty() {
                        self.current_idx = self
                            .saved_current_path
                            .as_deref()
                            .and_then(|cp| tracks.iter().position(|t| t.path == cp))
                            .or(Some(0));
                        self.queue = tracks;
                    }
                }
                if !self.queue.is_empty() {
                    let idx = self.current_idx.unwrap_or(0);
                    match self.load_and_play(idx) {
                        Ok(()) => AudioResp::Ok,
                        Err(e) => AudioResp::Err(e),
                    }
                } else {
                    AudioResp::Ok
                }
            }
            AudioCmd::Pause => {
                if let Some(p) = &self.player {
                    p.pause();
                }
                self.pending_resume = false;
                self.user_paused = true;
                self.save_state();
                AudioResp::Ok
            }
            AudioCmd::TogglePlayPause => {
                let is_playing = self
                    .player
                    .as_ref()
                    .is_some_and(|p| !p.is_paused() && !p.empty());
                if is_playing || (self.pending_resume && !self.user_paused) {
                    if let Some(p) = &self.player {
                        p.pause();
                    }
                    self.pending_resume = false;
                    self.user_paused = true;
                    self.save_state();
                    AudioResp::Ok
                } else {
                    self.handle_cmd(AudioCmd::Play, bg_tx)
                }
            }
            AudioCmd::Next => {
                if let Some(idx) = self.current_idx {
                    if idx + 1 < self.queue.len() {
                        self.pending_seek_sec = None;
                        return match self.load_and_play(idx + 1) {
                            Ok(()) => AudioResp::Ok,
                            Err(e) => AudioResp::Err(e),
                        };
                    }
                }
                AudioResp::Ok
            }
            AudioCmd::Previous => {
                let pos = self
                    .player
                    .as_ref()
                    .map(|p| p.get_pos().as_secs_f64())
                    .unwrap_or(0.0);
                if pos > 3.0 {
                    if let Some(p) = &self.player {
                        let _ = p.try_seek(Duration::ZERO);
                    }
                    self.pending_seek_sec = None;
                    self.save_state();
                    return AudioResp::Ok;
                }
                if let Some(idx) = self.current_idx {
                    self.pending_seek_sec = None;
                    if idx > 0 {
                        return match self.load_and_play(idx - 1) {
                            Ok(()) => AudioResp::Ok,
                            Err(e) => AudioResp::Err(e),
                        };
                    } else if let Some(p) = &self.player {
                        let _ = p.try_seek(Duration::ZERO);
                        self.save_state();
                    }
                }
                AudioResp::Ok
            }
            AudioCmd::Seek(secs) => {
                let target = secs.max(0.0);
                if let Some(p) = &self.player {
                    let _ = p.try_seek(Duration::from_secs_f64(target));
                } else {
                    self.pending_seek_sec = Some(target);
                }
                self.save_state();
                AudioResp::Ok
            }
            AudioCmd::SetVolume(vol) => {
                self.volume_pct = vol.min(100);
                if let Some(p) = &self.player {
                    p.set_volume(self.volume_pct as f32 / 100.0);
                }
                self.save_state();
                AudioResp::Ok
            }
            AudioCmd::PlayQueue { urls, start_index } => {
                let index = load_or_build_index(false);
                let tracks: Vec<IndexedTrack> = urls
                    .iter()
                    .filter_map(|u| resolve_track_from_url(u, &index))
                    .collect();
                if tracks.is_empty() {
                    return AudioResp::Err("no playable tracks in queue".into());
                }
                let start = start_index.min(tracks.len().saturating_sub(1));
                self.queue = tracks;
                self.pending_seek_sec = None;
                match self.load_and_play(start) {
                    Ok(()) => AudioResp::Ok,
                    Err(e) => AudioResp::Err(e),
                }
            }
            AudioCmd::ReloadLibrary => {
                let tx = bg_tx.clone();
                std::thread::spawn(move || {
                    let tracks = load_or_build_index(true);
                    let _ = tx.send(ThreadMsg::ReloadedQueue(tracks));
                });
                AudioResp::Ok
            }
        }
    }
}

fn audio_thread_loop(cmd_rx: mpsc::Receiver<ThreadMsg>, bg_tx: mpsc::Sender<ThreadMsg>) {
    let mut engine = PlaybackEngine::new();
    if engine.pending_resume {
        if let Some(idx) = engine.current_idx {
            let _ = engine.load_and_play(idx);
        }
    }

    // Populate default library queue asynchronously if nothing was restored from saved state.
    let init_tx = bg_tx.clone();
    std::thread::spawn(move || {
        let tracks = load_or_build_index(false);
        let _ = init_tx.send(ThreadMsg::DefaultQueue(tracks));
    });

    loop {
        match cmd_rx.recv_timeout(Duration::from_millis(200)) {
            Ok(ThreadMsg::Client(cmd, reply_tx)) => {
                let resp = engine.handle_cmd(cmd, &bg_tx);
                let _ = reply_tx.send(resp);
            }
            Ok(ThreadMsg::DefaultQueue(tracks)) => {
                if engine.queue.is_empty() && !tracks.is_empty() {
                    let idx = engine
                        .saved_current_path
                        .as_deref()
                        .and_then(|cp| tracks.iter().position(|t| t.path == cp))
                        .or(Some(0));
                    engine.queue = tracks;
                    engine.current_idx = idx;
                    if engine.pending_resume {
                        if let Some(i) = idx {
                            let _ = engine.load_and_play(i);
                        }
                    } else {
                        engine.user_paused = true;
                    }
                }
            }
            Ok(ThreadMsg::ReloadedQueue(tracks)) => {
                let is_idle = engine.player.as_ref().is_none_or(|p| p.empty());
                if engine.queue.is_empty() || (engine.user_paused && is_idle) {
                    let cur_path = engine
                        .current_idx
                        .and_then(|i| engine.queue.get(i))
                        .map(|t| t.path.clone())
                        .or_else(|| engine.saved_current_path.clone());
                    let idx = if tracks.is_empty() {
                        None
                    } else {
                        cur_path
                            .as_deref()
                            .and_then(|cp| tracks.iter().position(|t| t.path == cp))
                            .or(Some(0))
                    };
                    engine.current_idx = idx;
                    engine.queue = tracks;
                    engine.save_state();
                }
            }
            Ok(ThreadMsg::Shutdown(ack_tx)) => {
                engine.save_state();
                let _ = ack_tx.send(());
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                engine.tick_auto_advance();
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                engine.save_state();
                break;
            }
        }
    }
}

// ── Client API (`LocalMusicBackend`) ──────────────────────────────────────

pub struct LocalMusicBackend;

impl LocalMusicBackend {
    pub fn get_music_dir() -> PathBuf {
        get_music_dir()
    }

    pub fn get_music_dirs() -> Vec<PathBuf> {
        get_music_dirs()
    }

    fn cache() -> &'static Mutex<Option<(Instant, Option<MprisTrack>)>> {
        static CACHE: OnceLock<Mutex<Option<(Instant, Option<MprisTrack>)>>> = OnceLock::new();
        CACHE.get_or_init(|| Mutex::new(None))
    }

    pub fn clear_cache() {
        let mut g = Self::cache().lock().unwrap();
        *g = None;
    }

    fn ensure_daemon() -> bool {
        let sock = socket_path();
        if UnixStream::connect(&sock).is_ok() {
            return true;
        }

        static SPAWN_LOCK: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
        let lock = SPAWN_LOCK.get_or_init(|| Mutex::new(None));
        {
            let mut last = lock.lock().unwrap();
            if last.is_some_and(|t| t.elapsed() < Duration::from_secs(2)) {
                return UnixStream::connect(&sock).is_ok();
            }
            *last = Some(Instant::now());
        }

        let exe = crate::libraries::process::resolve_exe();

        // Spawn inside its own transient systemd unit (app.slice) so closing or
        // restarting the bar/service never kills active music playback.
        let systemd_ok = std::process::Command::new("systemd-run")
            .args([
                "--user",
                "--collect",
                "--quiet",
                "--slice=app.slice",
                "--",
            ])
            .arg(&exe)
            .arg("--audio-daemon")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success());

        if !systemd_ok {
            let mut cmd = std::process::Command::new(exe);
            cmd.arg("--audio-daemon")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null());

            if let Ok(log_file) = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .mode(0o600)
                .open(daemon_log_path())
            {
                cmd.stderr(log_file);
            } else {
                cmd.stderr(std::process::Stdio::null());
            }

            if crate::libraries::process::spawn_and_reap(&mut cmd).is_err() {
                return false;
            }
        }

        for _ in 0..40 {
            if UnixStream::connect(&sock).is_ok() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        false
    }

    fn send_cmd_sync(cmd: AudioCmd, start_if_down: bool) -> Option<AudioResp> {
        let sock = socket_path();
        let stream = match UnixStream::connect(&sock) {
            Ok(s) => s,
            Err(_) if start_if_down && Self::ensure_daemon() => UnixStream::connect(&sock).ok()?,
            Err(_) => return None,
        };

        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));

        let mut writer = stream.try_clone().ok()?;
        let mut encoded = serde_json::to_string(&cmd).ok()?;
        encoded.push('\n');
        writer.write_all(encoded.as_bytes()).ok()?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        std::io::Read::take(&mut reader, MAX_IPC_BYTES)
            .read_line(&mut line)
            .ok()?;
        serde_json::from_str::<AudioResp>(line.trim()).ok()
    }

    async fn send_cmd(cmd: AudioCmd, start_if_down: bool) -> Option<AudioResp> {
        tokio::task::spawn_blocking(move || Self::send_cmd_sync(cmd, start_if_down))
            .await
            .ok()
            .flatten()
    }

    pub async fn rescan_library() {
        let _ = tokio::task::spawn_blocking(|| load_or_build_index(true)).await;
        let _ = Self::send_cmd(AudioCmd::ReloadLibrary, false).await;
        Self::clear_cache();
    }

    pub async fn poll() -> Option<MprisTrack> {
        {
            let g = Self::cache().lock().unwrap();
            if let Some((t, tr)) = &*g {
                if t.elapsed().as_secs() < 5 {
                    return tr.clone();
                }
            }
        }
        let result = Self::poll_uncached().await;
        {
            let mut g = Self::cache().lock().unwrap();
            *g = Some((Instant::now(), result.clone()));
        }
        result
    }

    async fn poll_uncached() -> Option<MprisTrack> {
        match Self::send_cmd(AudioCmd::Poll, true).await {
            Some(AudioResp::State(st)) => Some(MprisTrack {
                title: st.title,
                artist: st.artist,
                album: st.album,
                is_playing: st.is_playing,
                position_sec: st.position_sec,
                length_sec: st.length_sec,
                volume_pct: st.volume_pct,
                url: st.url,
            }),
            _ => None,
        }
    }

    pub async fn search_songs(query: &str) -> Vec<(String, String)> {
        let q = query.trim().to_lowercase();
        tokio::task::spawn_blocking(move || {
            let tracks = load_or_build_index(false);
            tracks
                .into_iter()
                .filter(|t| {
                    q.is_empty()
                        || t.title.to_lowercase().contains(&q)
                        || t.artist.to_lowercase().contains(&q)
                        || t.album.to_lowercase().contains(&q)
                        || t.path.to_lowercase().contains(&q)
                })
                .take(500)
                .map(|t| {
                    let label = if t.artist.is_empty() {
                        t.title
                    } else {
                        format!("{} — {}", t.artist, t.title)
                    };
                    (label, t.path)
                })
                .collect()
        })
        .await
        .unwrap_or_default()
    }

    pub async fn play_queue(urls: &[String], start_index: usize) {
        let urls_owned = urls.to_vec();
        let _ = Self::send_cmd(
            AudioCmd::PlayQueue {
                urls: urls_owned,
                start_index,
            },
            true,
        )
        .await;
        Self::clear_cache();
    }

    pub async fn play() {
        let _ = Self::send_cmd(AudioCmd::Play, true).await;
        Self::clear_cache();
    }

    pub async fn pause() {
        let _ = Self::send_cmd(AudioCmd::Pause, false).await;
        Self::clear_cache();
    }

    pub async fn toggle_play_pause() {
        let _ = Self::send_cmd(AudioCmd::TogglePlayPause, true).await;
        Self::clear_cache();
    }

    pub async fn next() {
        let _ = Self::send_cmd(AudioCmd::Next, false).await;
        Self::clear_cache();
    }

    pub async fn previous() {
        let _ = Self::send_cmd(AudioCmd::Previous, false).await;
        Self::clear_cache();
    }

    pub async fn seek(secs: f64) {
        let _ = Self::send_cmd(AudioCmd::Seek(secs), false).await;
        Self::clear_cache();
    }

    pub async fn set_volume(vol: u8) {
        let _ = Self::send_cmd(AudioCmd::SetVolume(vol), true).await;
        Self::clear_cache();
    }
}
