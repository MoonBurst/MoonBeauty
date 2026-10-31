use iced::widget::{button, column, container, row, scrollable, text, text_input, Row, Stack};
use iced::{Alignment, Color, Element, Length, Padding};

use crate::libraries::config::settings::ShellSettings;
use crate::libraries::config::theme::ShellTheme;
use crate::libraries::style::shape_box::{ShapeBox, ShapeType};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClipItem {
    pub id: u64,
    pub text: String,
    pub is_image: bool,
    pub thumb_path: Option<String>,
    /// SHA-256 of the raw ingested bytes. Used to skip duplicate inserts
    /// when `wl-paste --watch` fires more than once per wl-copy (fork-mode
    /// ownership change produces two events for the same payload).
    #[serde(default)]
    pub content_hash: Option<String>,
    /// Durable source file (archive for annotator saves,
    /// origin for external URI ingests). Re-paste prefers this.
    #[serde(default)]
    pub source_path: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone)]
pub enum ClipboardMessage {
    SearchChanged(String),
    SelectEntry(u64),
    SelectNext,
    SelectPrev,
    CopySelected,
    CopyEntry(u64),
    DeleteEntry(u64),
    DeleteSelected,
    WipeAll,
    RunOcr(u64),
    SetFilter(ClipFilter),
    RevealImage(u64),
    Reloaded(Vec<ClipItem>),
    PreviewLoaded {
        text: String,
        image_path: Option<String>,
    },
    Dismiss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipFilter {
    All,
    Text,
    Images,
}

// ── Payload dispatch (fix4) ────────────────────────────────────────
// Firefox requests image/png for raw image bytes; JPEG/WebP/GIF/JXL
// blobs are silently ignored on paste. text/uri-list is universally
// accepted — the target loads the file via its own decoders
// (image.jxl.enabled covers JXL). Non-PNG is served as text/uri-list.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Payload { UriList, Bytes(&'static str) }

impl Payload {
    pub fn for_extension(ext: &str) -> Self {
        match ext.to_ascii_lowercase().as_str() {
            "jxl" | "webp" | "gif" | "bmp" => Self::UriList,
            "png" => Self::Bytes("image/png"),
            "jpg" | "jpeg" => Self::Bytes("image/jpeg"),
            _ => Self::UriList,
        }
    }
}

pub fn serve_uri_list(paths: &[std::path::PathBuf]) {
    if paths.is_empty() { return; }
    use std::io::Write;
    let body: String = paths.iter().map(|p| format!("file://{}\n", p.display())).collect();
    if let Ok(mut child) = std::process::Command::new("wl-copy")
        .args(["--type", "text/uri-list"])
        .stdin(std::process::Stdio::piped())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(body.as_bytes());
        }
        let _ = child.wait();
    }
}

fn decode_jxl_rgba(path: &std::path::Path) -> Option<(u32, u32, Vec<u8>)> {
    let tmp = crate::libraries::process::mbs_unique_tmp("mbs-thumb", "png");
    let ok = std::process::Command::new("djxl").arg(path).arg(&tmp).status().ok()?.success();
    if !ok { let _ = std::fs::remove_file(&tmp); return None; }
    let img = image::open(&tmp).ok();
    let _ = std::fs::remove_file(&tmp);
    let img = img?;
    let thumb = if img.width() > 256 {
        let h = (256.0 * img.height() as f32 / img.width() as f32) as u32;
        image::imageops::resize(&img.to_rgba8(), 256, h, image::imageops::FilterType::Triangle)
    } else { img.to_rgba8() };
    Some((thumb.width(), thumb.height(), thumb.into_raw()))
}

pub fn handle_for_path(path: &std::path::Path) -> Option<iced::widget::image::Handle> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<std::path::PathBuf, iced::widget::image::Handle>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(h) = cache.lock().unwrap().get(path) { return Some(h.clone()); }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    let handle = if ext == "jxl" {
        let (w, h, bytes) = decode_jxl_rgba(path)?;
        iced::widget::image::Handle::from_rgba(w, h, bytes)
    } else {
        iced::widget::image::Handle::from_path(path)
    };
    cache.lock().unwrap().insert(path.to_path_buf(), handle.clone());
    Some(handle)
}

pub struct ClipboardOverlay {
    pub search_query: String,
    pub entries: Vec<ClipItem>,
    pub selected_id: Option<u64>,
    pub preview_text: String,
    pub preview_image_path: Option<String>,
    pub filter: ClipFilter,
}

impl Default for ClipboardOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl ClipboardOverlay {
    pub fn filtered_ids(&self) -> Vec<u64> {
        self.entries
            .iter()
            .filter(|e| match self.filter {
                ClipFilter::All => true,
                ClipFilter::Text => !e.is_image,
                ClipFilter::Images => e.is_image,
            })
            .filter(|e| {
                self.search_query.is_empty()
                    || e.text
                        .to_lowercase()
                        .contains(&self.search_query.to_lowercase())
            })
            .map(|e| e.id)
            .collect()
    }

    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            entries: Vec::new(),
            selected_id: None,
            preview_text: String::new(),
            preview_image_path: None,
            filter: ClipFilter::All,
        }
    }

    /// Look up a ClipItem by id from index.json.

    /// Ingest a file into the index by reference. Used by annotator
    /// save and URI-list ingest. Sync — no tokio runtime needed.
    pub fn add_entry(source: &std::path::Path, display_name: Option<String>) -> Option<u64> {
        if !source.is_file() { return None; }
        let bytes = std::fs::read(source).ok()?;
        if bytes.is_empty() { return None; }
        let hash = {
            use sha2::Digest as _;
            format!("{:x}", sha2::Sha256::digest(&bytes))
        };
        let dir = dirs::data_dir().unwrap_or_else(std::env::temp_dir)
            .join("moon-beauty-shell").join("clipboard");
        std::fs::create_dir_all(&dir).ok()?;
        let index_path = dir.join("index.json");
        let mut items: Vec<ClipItem> = std::fs::read_to_string(&index_path).ok()
            .and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        if items.first().and_then(|t| t.content_hash.as_deref()) == Some(hash.as_str()) {
            return None;
        }
        let id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64).unwrap_or(0);
        let ext = source.extension().and_then(|e| e.to_str()).unwrap_or("bin");
        let is_image = matches!(
            ext.to_ascii_lowercase().as_str(),
            "jxl" | "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"
        );
        let text = if is_image {
            display_name.or_else(|| source.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| format!("Image {}", id))
        } else {
            String::from_utf8_lossy(&bytes[..bytes.len().min(100)]).to_string()
        };
        items.insert(0, ClipItem {
            id, text, is_image,
            thumb_path: Some(source.to_string_lossy().into_owned()),
            content_hash: Some(hash),
            source_path: Some(source.to_path_buf()),
        });
        items.truncate(100);
        let tmp = dir.join("index.json.tmp");
        std::fs::write(&tmp, serde_json::to_string(&items).ok()?).ok()?;
        std::fs::rename(&tmp, &index_path).ok()?;
        Some(id)
    }

    /// Resolve the preferred paste path: source_path if the file still
    /// exists, else the cache copy.
    pub fn resolve_path(id: u64) -> Option<std::path::PathBuf> {
        let dir = dirs::data_dir().unwrap_or_else(std::env::temp_dir)
            .join("moon-beauty-shell").join("clipboard");
        let index = std::fs::read_to_string(dir.join("index.json")).ok()?;
        let items: Vec<ClipItem> = serde_json::from_str(&index).ok()?;
        let item = items.into_iter().find(|i| i.id == id)?;
        if let Some(sp) = item.source_path {
            if sp.is_file() { return Some(sp); }
        }
        Self::get_file_path(&dir, id)
    }

    pub fn get_file_path(dir: &std::path::Path, id: u64) -> Option<std::path::PathBuf> {
        for ext in &["jxl", "png", "jpg", "gif", "webp", "txt", "bin"] {
            let p = dir.join(format!("{}.{}", id, ext));
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    /// Return a path iced's image widget can actually render. iced 0.14's
    /// decoder path goes through `image::load_from_memory`, which uses
    /// magic-byte detection that doesn't know JXL. For `.jxl` entries we
    /// generate a one-time `{id}.preview.png` companion via the registered
    /// JXL decoder and return that instead. PNG is iced-native, so this
    /// always renders.
    

    pub async fn reload() -> Vec<ClipItem> {
        let dir = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("moon-beauty-shell")
            .join("clipboard");
        let index_path = dir.join("index.json");

        let mut list = Vec::new();
        if let Ok(content) = std::fs::read_to_string(&index_path) {
            if let Ok(items) = serde_json::from_str::<Vec<ClipItem>>(&content) {
                for mut item in items {
                    if item.is_image {
                        if let Some(path) = Self::get_file_path(&dir, item.id) {
                            if item.text.is_empty()
                                || item.text == "📸 Image"
                                || item.text.starts_with("📸 Image 1")
                            {
                                crate::overlays::screenshot::annotator::register_jxl_decoder();
                        if let Ok((w, h)) = ::image::image_dimensions(&path) {
                                    item.text = format!("📸 Image {}x{}", w, h);
                                } else {
                                    item.text = format!("📸 Image {}", item.id);
                                }
                            }
                            item.thumb_path = Self::get_file_path(&dir, item.id).map(|p| p.to_string_lossy().into_owned());
                        } else {
                            if item.text.is_empty() {
                                item.text = format!("📸 Image {}", item.id);
                            }
                        }
                    }
                    list.push(item);
                }
            }
        }
        list
    }

    pub async fn load_preview(id: u64) -> (String, Option<String>) {
        let dir = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("moon-beauty-shell")
            .join("clipboard");
        if let Some(path) = Self::get_file_path(&dir, id) {
            if let Ok(bytes) = std::fs::read(&path) {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if ext == "txt" || ext == "bin" {
                    if let Ok(text) = String::from_utf8(bytes) {
                        return (text, None);
                    }
                }
            }
        }
        // For image entries (including JXL) hand iced the PNG preview
        // path. iced's image widget can't decode JXL directly, so
        // display_path generates a {id}.preview.png companion on first
        // view and returns that.
        if let Some(raw) = Self::resolve_path(id).or_else(|| Self::get_file_path(&dir, id)) {
            return (String::new(), Some(raw.to_string_lossy().into_owned()));
        }
        (String::new(), None)
    }

    pub async fn copy(id: u64) {
        let Some(paste_path) = Self::resolve_path(id) else { return };
        let ext = paste_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let payload = Payload::for_extension(ext);
        use tokio::io::AsyncWriteExt;
        let (mime, body) = match payload {
            Payload::UriList => (
                "text/uri-list",
                format!("file://{}\n", paste_path.display()).into_bytes(),
            ),
            Payload::Bytes(m) => {
                let Ok(data) = std::fs::read(&paste_path) else { return };
                (m, data)
            }
        };
        let Ok(mut child) = tokio::process::Command::new("wl-copy")
            .args(["--type", mime])
            .stdin(std::process::Stdio::piped())
            .spawn()
        else { return; };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(&body).await;
        }
        let _ = child.wait().await;
    }

    pub fn delete(id: u64) {
        let dir = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("moon-beauty-shell")
            .join("clipboard");
        let index_path = dir.join("index.json");

        if let Some(path) = Self::get_file_path(&dir, id) {
            let _ = std::fs::remove_file(path);
        }

        if let Ok(content) = std::fs::read_to_string(&index_path) {
            if let Ok(mut items) = serde_json::from_str::<Vec<ClipItem>>(&content) {
                items.retain(|i| i.id != id);
                if let Ok(new_json) = serde_json::to_string(&items) {
                    let _ = std::fs::write(&index_path, new_json);
                }
            }
        }
    }

    pub async fn wipe() {
        let dir = dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("moon-beauty-shell")
            .join("clipboard");
        let _ = std::fs::remove_dir_all(&dir);
    }

    pub async fn ocr(_id: u64) -> String {
        String::new() // OCR can be re-added via external tool if needed
    }

    pub fn view<'a>(
        &'a self,
        theme: &'a ShellTheme,
        settings: &ShellSettings,
    ) -> Element<'a, ClipboardMessage> {
        let is_image_selected = self
            .selected_id
            .and_then(|id| self.entries.iter().find(|e| e.id == id))
            .map(|e| e.is_image)
            .unwrap_or(false);

        let (win_w, win_h) = settings.get_window_size("clipboard", 1000, 700);
        let cut = theme.overlay_cut as u16;

        let cut_f = theme.overlay_cut;
        let angle_deg = theme.overlay_angle;
        let rad = angle_deg.to_radians();
        let sqrt2 = std::f32::consts::SQRT_2;
        let dx = (cut_f * rad.cos() * sqrt2).min(win_w * 0.45).max(0.0);
        let dy = (cut_f * rad.sin() * sqrt2).min(win_h * 0.45).max(0.0);

        let pad_top = 18.0_f32;
        let pad_bottom = 16.0_f32;

        let search_inward = if dy > pad_top {
            (dx * (1.0 - (pad_top / dy))).max(0.0) + 16.0
        } else {
            16.0
        };

        let list_top_inset = if dy > 72.0 {
            (dx * (1.0 - (72.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let list_bottom_inset = if dy > 45.0 {
            (dx * (1.0 - (45.0 / dy))).max(0.0) + 16.0
        } else {
            16.0
        };
        let list_side_inset = list_top_inset.max(list_bottom_inset).max(20.0);

        // Top Row
        let search_input = text_input("Search clipboard...", &self.search_query)
            .on_input(ClipboardMessage::SearchChanged)
            .padding(Padding {
                top: 12.0,
                right: 24.0,
                bottom: 12.0,
                left: 24.0,
            })
            .size((theme.font_size_overlay) as f32)
            .width(Length::Fill)
            .style(move |_t, _s| text_input::Style {
                background: iced::Background::Color(Color::TRANSPARENT),
                border: iced::Border::default(),
                icon: theme.muted,
                placeholder: theme.muted,
                value: theme.text,
                selection: theme.surface_alt,
            });

        let search_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(theme.background)
            .border(theme.border, theme.control_border_width)
            .size(Length::Fill, Length::Fixed(48.0));

        let search_bar = container(Stack::new().push(search_bg.view()).push(search_input))
            .height(Length::Fixed(48.0));

        let wipe_btn = button(
            container(
                text("Wipe")
                    .size((theme.font_size_overlay) as f32)
                    .color(theme.danger),
            )
            .center_y(Length::Fill)
            .center_x(Length::Fill),
        )
        .on_press(ClipboardMessage::WipeAll)
        .height(Length::Fixed(48.0))
        .width(Length::Fixed(100.0))
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            border: iced::Border::default(),
            ..Default::default()
        });

        let wipe_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(theme.background)
            .border(theme.danger, theme.control_border_width)
            .size(Length::Fixed(100.0), Length::Fixed(48.0));

        let wipe_bar = container(Stack::new().push(wipe_bg.view()).push(wipe_btn))
            .height(Length::Fixed(48.0))
            .width(Length::Fixed(100.0));

        let ocr_btn = button(
            container(text("OCR").size((theme.font_size_overlay) as f32).color(
                if is_image_selected {
                    theme.text
                } else {
                    theme.muted
                },
            ))
            .center_y(Length::Fill)
            .center_x(Length::Fill),
        )
        .on_press_maybe(Some(ClipboardMessage::RunOcr(
            self.selected_id.unwrap_or(0),
        )))
        .height(Length::Fixed(48.0))
        .width(Length::Fixed(80.0))
        .style(move |_t, _s| button::Style {
            background: Some(iced::Background::Color(Color::TRANSPARENT)),
            border: iced::Border::default(),
            ..Default::default()
        });

        let ocr_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(theme.background)
            .border(
                if is_image_selected {
                    theme.text
                } else {
                    theme.border
                },
                theme.control_border_width,
            )
            .size(Length::Fixed(80.0), Length::Fixed(48.0));

        let ocr_bar = container(Stack::new().push(ocr_bg.view()).push(ocr_btn))
            .height(Length::Fixed(48.0))
            .width(Length::Fixed(80.0));

        let mut top_row = Row::new().spacing(12).align_y(Alignment::Center);
        top_row = top_row.push(search_bar);
        top_row = top_row.push(ocr_bar);
        top_row = top_row.push(wipe_bar);

        let input_row = container(top_row).width(Length::Fill).padding(Padding {
            top: 0.0,
            right: search_inward,
            bottom: 0.0,
            left: search_inward,
        });

        // Bottom Row
        let mk_tab = |label: &'static str, filter: ClipFilter| -> Element<'a, ClipboardMessage> {
            let is_active = self.filter == filter;
            let bg = if is_active {
                theme.surface_alt
            } else {
                theme.background
            };
            let border = if is_active { theme.text } else { theme.border };
            let text_w = label.chars().count() as f32 * (theme.font_size_overlay as f32) * 0.6;
            let w = (text_w + (cut as f32 * 2.0) + 24.0).max(80.0);
            let shape = ShapeBox::new()
                .shape(ShapeType::Hexagon { cut })
                .background(bg)
                .border(border, theme.control_border_width)
                .size(Length::Fixed(w), Length::Fixed(48.0));
            button(
                Stack::new().push(shape.view()).push(
                    container(
                        text(label)
                            .size((theme.font_size_overlay) as f32)
                            .color(theme.text),
                    )
                    .center_x(Length::Fill)
                    .center_y(Length::Fill)
                    .width(Length::Fixed(w))
                    .height(Length::Fixed(48.0)),
                ),
            )
            .on_press(ClipboardMessage::SetFilter(filter))
            .padding(0)
            .style(|_t, _s| button::Style {
                background: None,
                ..Default::default()
            })
            .into()
        };

        let reveal_bg_col = if is_image_selected {
            theme.surface_alt
        } else {
            theme.background
        };
        let reveal_border_col = if is_image_selected {
            theme.text
        } else {
            theme.border
        };

        let reveal_text_w =
            "Reveal".chars().count() as f32 * (theme.font_size_overlay as f32) * 0.6;
        let reveal_w = (reveal_text_w + (cut as f32 * 2.0) + 24.0).max(80.0);

        let reveal_shape = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(reveal_bg_col)
            .border(reveal_border_col, theme.control_border_width)
            .size(Length::Fixed(reveal_w), Length::Fixed(48.0));

        let reveal_btn = button(
            Stack::new().push(reveal_shape.view()).push(
                container(
                    text("Reveal")
                        .size((theme.font_size_overlay) as f32)
                        .color(theme.text),
                )
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .width(Length::Fixed(reveal_w))
                .height(Length::Fixed(48.0)),
            ),
        )
        .on_press_maybe(if is_image_selected {
            Some(ClipboardMessage::RevealImage(self.selected_id.unwrap_or(0)))
        } else {
            None
        })
        .padding(0)
        .style(|_t, _s| button::Style {
            background: None,
            ..Default::default()
        });

        let reveal_bar = container(reveal_btn)
            .height(Length::Fixed(48.0))
            .width(Length::Fixed(reveal_w));

        let mut bot_row = Row::new().spacing(12).align_y(Alignment::Center);
        bot_row = bot_row.push(mk_tab("Text", ClipFilter::Text));
        bot_row = bot_row.push(mk_tab("Images", ClipFilter::Images));
        bot_row = bot_row.push(reveal_bar);

        let filter_row = container(bot_row).width(Length::Fill).padding(Padding {
            top: 12.0,
            right: search_inward,
            bottom: 0.0,
            left: search_inward,
        });

        // Left Pane (List)
        let mut list_col = column![].spacing(8);
        for item in &self.entries {
            if let ClipFilter::Text = self.filter {
                if item.is_image {
                    continue;
                }
            } else if let ClipFilter::Images = self.filter {
                if !item.is_image {
                    continue;
                }
            }

            if !self.search_query.is_empty()
                && !item
                    .text
                    .to_lowercase()
                    .contains(&self.search_query.to_lowercase())
            {
                continue;
            }

            let is_sel = self.selected_id == Some(item.id);
            let border_color = if is_sel { theme.danger } else { theme.border };
            let text_color = if is_sel { theme.text } else { theme.text };

            let raw_text = item.text.lines().next().unwrap_or("").to_string();
            let display_text = if raw_text.chars().count() > 45 {
                let truncated: String = raw_text.chars().take(42).collect();
                format!("{}...", truncated)
            } else {
                raw_text
            };

            let mut item_row = row![].spacing(12).align_y(Alignment::Center);
            if let Some(thumb) = &item.thumb_path {
                item_row = item_row.push(
                    {
                        let __el: Element<'_, ClipboardMessage> = match handle_for_path(std::path::Path::new(&thumb)) {
                            Some(h) => iced::widget::image(h)
                                .width(Length::Fixed(40.0))
                                .height(Length::Fixed(40.0))
                                .content_fit(iced::ContentFit::Contain)
                                .into(),
                            None => text("⚠").into(),
                        };
                        __el
                    },
                );
            } else {
                item_row = item_row.push(text("■").size(10).color(theme.muted));
            }

            let text_container = container(
                text(display_text)
                    .size((theme.font_size_overlay) as f32)
                    .color(text_color),
            )
            .width(Length::Fill)
            .clip(true);
            item_row = item_row.push(text_container);

            let btn_content = container(item_row)
                .padding(Padding {
                    top: 0.0,
                    right: 24.0,
                    bottom: 0.0,
                    left: 24.0,
                })
                .width(Length::Fill)
                .height(Length::Fixed(56.0))
                .align_y(Alignment::Center);

            let item_bg = ShapeBox::new()
                .shape(ShapeType::Hexagon { cut })
                .background(theme.background)
                .border(border_color, theme.control_border_width)
                .size(Length::Fill, Length::Fixed(56.0));

            let item_btn = button(Stack::new().push(item_bg.view()).push(btn_content))
                .on_press(ClipboardMessage::SelectEntry(item.id))
                .width(Length::Fill)
                .height(Length::Fixed(56.0))
                .style(move |_t, _s| button::Style {
                    background: Some(iced::Background::Color(Color::TRANSPARENT)),
                    border: iced::Border::default(),
                    ..Default::default()
                });

            list_col = list_col.push(item_btn);
        }

        let left_pane = container(
            scrollable(list_col)
                .id(iced::widget::Id::new("clip_list"))
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::FillPortion(1))
        .height(Length::Fill)
        .clip(true)
        .padding(Padding {
            top: list_top_inset,
            right: 24.0,
            bottom: list_bottom_inset,
            left: list_side_inset,
        });

        // Right Pane (Preview)
        let inner_pad = cut_f + 4.0;
        let inner_padding = Padding {
            top: inner_pad,
            right: inner_pad,
            bottom: inner_pad,
            left: inner_pad,
        };

        let preview_content: Element<'a, ClipboardMessage> = if let Some(img_path) =
            &self.preview_image_path
        {
            container(
                container(
                    {
                        let __el: Element<'_, ClipboardMessage> = match handle_for_path(std::path::Path::new(img_path)) {
                            Some(h) => iced::widget::image(h)
                                .width(Length::Fill)
                                .content_fit(iced::ContentFit::Contain)
                                .into(),
                            None => container(
                                text("(could not decode image)")
                                    .size((theme.font_size_overlay) as f32)
                                    .color(theme.muted),
                            ).center_x(Length::Fill).center_y(Length::Fill).into(),
                        };
                        __el
                    },
                )
                .padding(inner_padding)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .into()
        } else if !self.preview_text.is_empty() {
            container(
                scrollable(
                    container(
                        text(&self.preview_text)
                            .size((theme.font_size_overlay) as f32)
                            .font(iced::Font::MONOSPACE)
                            .color(theme.text)
                            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
                    )
                    .width(Length::Fill),
                )
                .width(Length::Fill)
                .height(Length::Fill),
            )
            .padding(inner_padding)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        } else {
            container(
                text("Select an entry to preview")
                    .size((theme.font_size_overlay) as f32)
                    .color(theme.muted),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
        };

        let right_bg = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(theme.background)
            .border(theme.border, theme.control_border_width)
            .size(Length::Fill, Length::Fill);

        let right_pane = container(Stack::new().push(right_bg.view()).push(preview_content))
            .width(Length::FillPortion(2))
            .clip(true)
            .height(Length::Fill)
            .clip(true);

        let content = column![
            input_row,
            filter_row,
            row![left_pane, right_pane]
                .spacing(16)
                .height(Length::Fill)
                .width(Length::Fill)
        ]
        .spacing(16)
        .padding(Padding {
            top: pad_top,
            right: 0.0,
            bottom: pad_bottom,
            left: 0.0,
        })
        .width(Length::Fixed(win_w))
        .height(Length::Fixed(win_h));

        let bg_shape = ShapeBox::new()
            .shape(ShapeType::Hexagon { cut })
            .background(theme.background)
            .border(theme.border, theme.control_border_width)
            .size(Length::Fixed(win_w), Length::Fixed(win_h));

        container(Stack::new().push(bg_shape.view()).push(content))
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }
}
