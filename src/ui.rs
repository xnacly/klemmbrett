use std::error::Error;
use std::process::Command;
use std::sync::Arc;

use crate::proto::{self, Entry};
use crate::theme::Theme;

use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, FontData, FontDefinitions, FontFamily, FontId, Frame, Key, Layout,
    Margin, Rect, RichText, Sense, Shadow, Stroke, TextStyle,
};

const WIDTH: f32 = 640.0;
const HEIGHT: f32 = 560.0;
/// Entries fetched per request, more are fetched once the end of the list is scrolled into view
const PAGE_SIZE: usize = 10;
/// Characters of an entries first line shown as its preview
const PREVIEW_LEN: usize = 120;
const THUMBNAIL_HEIGHT: f32 = 96.0;
const BORDER: f32 = 2.0;
/// offset of the hard drop shadow behind each card
const SHADOW: i8 = 4;
/// width of the accent strip on the left edge of each card
const STRIP: f32 = 6.0;

enum Preview {
    Text(String),
    /// uri the image bytes are registered under in the egui image loader
    Image(String),
    Binary,
}

struct Item {
    entry: Entry,
    preview: Preview,
}

struct Ui {
    theme: Theme,
    items: Vec<Item>,
    has_more: bool,
    selected: usize,
    /// set when the selection moved via keyboard, so the list scrolls to it
    scroll_to_selected: bool,
    error: Option<String>,
}

/// Flat card in `fill` with a thin border, a hard shadow and an `accent` strip along its left edge
fn card<R>(ui: &mut egui::Ui, theme: &Theme, accent: Color32, fill: Color32, body: impl FnOnce(&mut egui::Ui) -> R) -> egui::Response {
    let res = Frame::new()
        .fill(fill)
        .stroke(Stroke::new(BORDER, theme.border))
        .inner_margin(Margin { left: 12 + STRIP as i8, right: 12, top: 8, bottom: 10 })
        .outer_margin(Margin { left: 0, right: SHADOW, top: 0, bottom: SHADOW })
        .shadow(Shadow { offset: [SHADOW, SHADOW], blur: 0, spread: 0, color: theme.border })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui)
        })
        .response;
    // the response includes the outer margin reserved for the shadow, the card itself ends before it
    let card = Rect::from_min_max(res.rect.min, res.rect.max - egui::Vec2::splat(SHADOW as f32));
    let strip = Rect::from_min_size(card.min + egui::Vec2::splat(BORDER), egui::vec2(STRIP, card.height() - 2.0 * BORDER));
    ui.painter().rect_filled(strip, CornerRadius::ZERO, accent);
    res
}

/// Opens a window listing the clipboard history, clicking an entry or pressing its number makes it
/// the clipboard content
pub fn run() -> Result<(), Box<dyn Error>> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id("klemmbrett")
            .with_title("klemmbrett")
            .with_inner_size([WIDTH, HEIGHT])
            // a fixed size makes tiling compositors like sway float the window
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        "klemmbrett",
        options,
        Box::new(|cc| {
            egui_extras::install_image_loaders(&cc.egui_ctx);
            let (theme, theme_error) = Theme::load();
            setup_style(&cc.egui_ctx, &theme);
            let mut ui = Ui { theme, items: Vec::new(), has_more: true, selected: 0, scroll_to_selected: false, error: None };
            ui.load_more(&cc.egui_ctx);
            ui.error = ui.error.or(theme_error);
            Ok(Box::new(ui))
        }),
    )?;
    Ok(())
}

/// Resolves a fontconfig pattern to the font file the system would use for it
fn system_font(pattern: &str) -> Option<FontData> {
    let out = Command::new("fc-match").args(["-f", "%{file}\n%{index}", pattern]).output().ok()?;
    let out = String::from_utf8(out.stdout).ok()?;
    let (file, index) = out.split_once('\n')?;
    let mut font = FontData::from_owned(std::fs::read(file).ok()?);
    font.index = index.trim().parse().unwrap_or(0);
    Some(font)
}

/// Uses the system fonts if fontconfig is available and bumps the text sizes, egui already scales
/// everything by the outputs scale factor on top of these
fn setup_style(ctx: &egui::Context, theme: &Theme) {
    ctx.set_theme(if theme.is_dark() { egui::Theme::Dark } else { egui::Theme::Light });

    let mut fonts = FontDefinitions::default();
    for (pattern, family) in [("sans-serif:bold", FontFamily::Proportional), ("monospace", FontFamily::Monospace)] {
        if let Some(font) = system_font(pattern) {
            fonts.font_data.insert(pattern.into(), Arc::new(font));
            fonts.families.entry(family).or_default().insert(0, pattern.into());
        }
    }
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(14.0)),
            (TextStyle::Body, FontId::proportional(18.0)),
            (TextStyle::Button, FontId::proportional(18.0)),
            (TextStyle::Heading, FontId::proportional(28.0)),
            (TextStyle::Monospace, FontId::monospace(17.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    });
}

fn human_size(len: usize) -> String {
    match len {
        0..1024 => format!("{len} B"),
        1024..1048576 => format!("{:.1} KiB", len as f64 / 1024.0),
        _ => format!("{:.1} MiB", len as f64 / 1048576.0),
    }
}

impl Item {
    fn new(ctx: &egui::Context, entry: Entry) -> Self {
        let preview = if entry.mime.starts_with("text/") {
            let text = String::from_utf8_lossy(&entry.data);
            let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or_default().trim();
            let mut preview: String = line.chars().take(PREVIEW_LEN).collect();
            if line.chars().count() > PREVIEW_LEN || text.trim().lines().count() > 1 {
                preview.push('…');
            }
            Preview::Text(preview)
        } else if entry.mime.starts_with("image/") {
            let uri = format!("bytes://klemmbrett/{}", entry.id.iter().map(|b| format!("{b:02x}")).collect::<String>());
            ctx.include_bytes(uri.clone(), entry.data.clone());
            Preview::Image(uri)
        } else {
            Preview::Binary
        };
        Self { entry, preview }
    }
}

impl Ui {
    /// Fetches the next page, plus one entry to know if there is another page after it
    fn load_more(&mut self, ctx: &egui::Context) {
        match proto::list(PAGE_SIZE as u8 + 1, self.items.len() as u32) {
            Ok(mut entries) => {
                self.has_more = entries.len() > PAGE_SIZE;
                entries.truncate(PAGE_SIZE);
                self.items.extend(entries.into_iter().map(|e| Item::new(ctx, e)));
                self.error = None;
            }
            Err(e) => {
                self.has_more = false;
                self.error = Some(format!("failed to list entries: {e}"));
            }
        }
    }

    fn copy(&mut self, ctx: &egui::Context, idx: usize) {
        let Some(item) = self.items.get(idx) else { return };
        match proto::set(item.entry.id) {
            Ok(()) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            Err(e) => self.error = Some(format!("failed to copy entry: {e}")),
        }
    }

    /// ↑/↓ or k/j move the selection, enter copies it, 0-9 copy that entry directly, esc closes
    fn handle_keys(&mut self, ctx: &egui::Context) {
        const DIGITS: [Key; 10] =
            [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
        let none = egui::Modifiers::NONE;
        let (down, up, enter, esc, digit) = ctx.input_mut(|i| {
            (
                i.consume_key(none, Key::ArrowDown) | i.consume_key(none, Key::J),
                i.consume_key(none, Key::ArrowUp) | i.consume_key(none, Key::K),
                i.consume_key(none, Key::Enter),
                i.consume_key(none, Key::Escape),
                DIGITS.iter().position(|k| i.consume_key(none, *k)),
            )
        });

        if esc {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if let Some(idx) = digit {
            self.copy(ctx, idx);
        } else if enter {
            self.copy(ctx, self.selected);
        } else if down || up {
            if down && self.selected + 1 >= self.items.len() && self.has_more {
                self.load_more(ctx);
            }
            self.selected = if down { self.selected + 1 } else { self.selected.saturating_sub(1) };
            self.selected = self.selected.min(self.items.len().saturating_sub(1));
            self.scroll_to_selected = true;
        }
    }

    fn row(&self, ui: &mut egui::Ui, idx: usize) -> egui::Response {
        let t = &self.theme;
        let item = &self.items[idx];
        let selected = idx == self.selected;
        let text = if selected { t.on_accent } else { t.text };

        let muted = if selected { t.on_accent } else { t.muted };

        let body = |ui: &mut egui::Ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let caption = format!("#{idx} · {} · {}", item.entry.mime, human_size(item.entry.data.len()));
            ui.add(egui::Label::new(RichText::new(caption).small().color(muted)).truncate());
            match &item.preview {
                Preview::Text(s) => {
                    ui.add(egui::Label::new(RichText::new(s).monospace().color(text)).truncate());
                }
                Preview::Image(uri) => {
                    ui.add(
                        egui::Image::new(uri.as_str())
                            .max_height(THUMBNAIL_HEIGHT)
                            .max_width(ui.available_width())
                            .corner_radius(CornerRadius::ZERO),
                    );
                }
                Preview::Binary => {
                    ui.label(RichText::new("binary data").italics().color(text));
                }
            }
        };

        let fill = if selected { t.selected } else { t.surface };
        let res = card(ui, t, t.accent(idx), fill, body);
        ui.interact(res.rect, ui.id().with(("row", idx)), Sense::click()).on_hover_cursor(CursorIcon::PointingHand)
    }
}

impl eframe::App for Ui {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_keys(&ctx);

        let panel = Frame::new().fill(self.theme.background).inner_margin(16);
        egui::CentralPanel::default().frame(panel).show(ui, |ui| {
            let t = &self.theme;
            ui.horizontal(|ui| {
                ui.label(RichText::new("klemmbrett").heading().strong().color(t.text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(RichText::new("↑↓ select · enter copy · 0-9 pick · esc close").small().color(t.muted));
                });
            });
            ui.add_space(SHADOW as f32);

            if let Some(error) = &self.error {
card(ui, t, t.error, t.surface, |ui| ui.label(RichText::new(error).color(t.text)));
            }
            if self.items.is_empty() && self.error.is_none() {
                ui.label(RichText::new("clipboard history is empty").color(t.muted));
            }

            let mut clicked = None;
            let mut hovered = None;
            let mut reached_end = false;
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                for idx in 0..self.items.len() {
                    let res = self.row(ui, idx);
                    if res.clicked() {
                        clicked = Some(idx);
                    }
                    // only follow the pointer when it moves, a resting pointer would fight the keyboard
                    if res.hovered() && ui.input(|i| i.pointer.is_moving()) {
                        hovered = Some(idx);
                    }
                    if idx == self.selected && self.scroll_to_selected {
                        res.scroll_to_me(None);
                    }
                }
                if self.has_more {
                    let res = ui.add(egui::Spinner::new().color(self.theme.text));
                    reached_end = ui.is_rect_visible(res.rect);
                }
            });
            self.scroll_to_selected = false;

            if let Some(idx) = hovered {
                self.selected = idx;
            }
            if let Some(idx) = clicked {
                self.copy(&ctx, idx);
            }
            if reached_end {
                self.load_more(&ctx);
            }
        });
    }
}
