//! What the popup looks like: the gpui view and its embedded icons. Knows nothing about
//! positioning.

use std::borrow::Cow;

use gpui::{
    AssetSource, Context, FontWeight, SharedString, Window, WindowAppearance, div, prelude::*, px,
    rgba, svg,
};
use tflick_ipc::{Item, Kind};

use crate::settings::{Settings, Theme};

/// Corner radius of the selected row; the popup's own corners follow it.
const ROW_RADIUS: f32 = 8.0;

/// Theme, font size and tint come from [`Settings`]; size and blur are the controller's.
pub struct Popup {
    pub items: Vec<Item>,
    pub selected: usize,
}

impl Popup {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<Settings>(|_, cx| cx.notify()).detach();
        Self {
            items: Vec::new(),
            selected: 0,
        }
    }
}

fn icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Subcommand => "icons/subcommand.svg",
        Kind::Option => "icons/option.svg",
        Kind::Arg => "icons/arg.svg",
        Kind::Folder => "icons/folder.svg",
        Kind::File => "icons/file.svg",
        Kind::Branch => "icons/branch.svg",
        Kind::History => "icons/history.svg",
    }
}

/// Accent color 0xRRGGBB00, alpha added by the caller.
fn color(kind: Kind) -> u32 {
    match kind {
        Kind::Subcommand => 0x8b5cf600,
        Kind::Option => 0x3b82f600,
        Kind::Arg => 0x14b8a600,
        Kind::Folder => 0xf59e0b00,
        Kind::File => 0x64748b00,
        Kind::Branch => 0xf9731600,
        Kind::History => 0x22c55e00,
    }
}

/// Icons are embedded in the binary, so the popup works from any working directory.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        let bytes: &'static [u8] = match path {
            "icons/subcommand.svg" => include_bytes!("../icons/subcommand.svg"),
            "icons/option.svg" => include_bytes!("../icons/option.svg"),
            "icons/arg.svg" => include_bytes!("../icons/arg.svg"),
            "icons/folder.svg" => include_bytes!("../icons/folder.svg"),
            "icons/file.svg" => include_bytes!("../icons/file.svg"),
            "icons/branch.svg" => include_bytes!("../icons/branch.svg"),
            "icons/history.svg" => include_bytes!("../icons/history.svg"),
            _ => return Ok(None),
        };
        Ok(Some(Cow::Borrowed(bytes)))
    }

    fn list(&self, _path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

impl Render for Popup {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = cx.global::<Settings>();
        // Follow the system theme like native menus: a light popup with dark text in light mode,
        // so text stays readable over the light apps behind it, and the reverse in dark mode.
        // ponytail: picks colors from the system theme, not from what is actually behind the
        // popup; a dark app under a light-mode popup still relies on the tint for contrast.
        let dark = match settings.theme {
            Theme::System => matches!(
                window.appearance(),
                WindowAppearance::Dark | WindowAppearance::VibrantDark
            ),
            Theme::Dark => true,
            Theme::Light => false,
        };
        let (bg, fg) = if dark {
            (0x1e1e2200, 0xffffff00)
        } else {
            (0xf5f5f700, 0x1d1d1f00)
        };
        let rows = self.items.iter().enumerate().map(|(i, item)| {
            let is_selected = i == self.selected;
            // Bare icon in the kind's accent color (white on the selected row).
            let icon = svg()
                .flex_none()
                .path(icon(item.kind))
                .size_4()
                .text_color(rgba(if is_selected {
                    0xffffffff
                } else {
                    color(item.kind) | 0xff
                }));
            div()
                .flex()
                .items_center()
                .gap_2()
                .px_2()
                .py_0p5()
                .rounded(px(ROW_RADIUS))
                .when(is_selected, |d| {
                    d.bg(rgba(0x3b82f6ff)).text_color(rgba(0xffffffff))
                })
                .child(icon)
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(SharedString::from(item.name.clone())),
                )
                .children(item.description.clone().map(|desc| {
                    div()
                        .when(!is_selected, |d| d.text_color(rgba(fg | 0xaa)))
                        .truncate()
                        .child(SharedString::from(desc))
                }))
        });
        div()
            .size_full()
            .p_1()
            // Concentric with the rows: row radius + p_1 (4px) + border (1px).
            .rounded(px(ROW_RADIUS + 5.))
            .border_1()
            .border_color(rgba(fg | 0x22))
            .bg(rgba(bg | settings.tint))
            .text_color(rgba(fg | 0xff))
            .text_size(px(settings.font_size as f32))
            .font_family(".SystemUIFont")
            .flex()
            .flex_col()
            .children(rows)
    }
}
