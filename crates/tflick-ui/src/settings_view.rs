//! The settings window, like Amazon Q's dashboard: a sidebar of pages, each a list of settings
//! that apply as soon as they are clicked. Settings of features not built yet are listed greyed
//! out, so the roadmap is visible without storing values that do nothing.

use std::process::Command;

use gpui::{
    AnyElement, App, ClipboardItem, Context, FontWeight, Global, SharedString, TitlebarOptions,
    Window, WindowAppearance, WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, rgb,
    size,
};

use crate::settings::{self, Settings, Theme};
use crate::{login, macos};

const ACCENT: u32 = 0x3b82f6;

/// Shell snippet that starts tflick in every new terminal.
const AUTOSTART: &str = r#"# TFlick: bọc shell tương tác, trừ khi đã ở trong tflick.
if [[ $- == *i* && "${TFLICK_TTY:-}" != "$(tty)" ]] && command -v tflick >/dev/null; then
  tflick
fi"#;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Page {
    Autocomplete,
    Appearance,
    Integrations,
    Keybindings,
}

impl Page {
    const ALL: [Page; 4] = [
        Page::Autocomplete,
        Page::Appearance,
        Page::Integrations,
        Page::Keybindings,
    ];

    fn title(self) -> &'static str {
        match self {
            Page::Autocomplete => "Gợi ý",
            Page::Appearance => "Giao diện",
            Page::Integrations => "Tích hợp",
            Page::Keybindings => "Phím tắt",
        }
    }
}

pub struct SettingsView {
    page: Page,
}

/// The open settings window, if any, so a second `tflick settings` focuses it.
#[derive(Default)]
struct OpenWindow(Option<WindowHandle<SettingsView>>);

impl Global for OpenWindow {}

/// Opens the settings window (on `page` if given), or brings the open one to the front.
pub fn open(cx: &mut App, page: Option<Page>) {
    let existing = cx.default_global::<OpenWindow>().0;
    let focused = existing.is_some_and(|handle| {
        handle
            .update(cx, |view, window, cx| {
                if let Some(page) = page {
                    view.page = page;
                    cx.notify();
                }
                window.activate_window();
            })
            .is_ok()
    });
    if !focused {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(780.), px(540.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("Cài đặt TFlick".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let opened = cx.open_window(options, |_, cx| {
            cx.new(|cx| {
                cx.observe_global::<Settings>(|_, cx| cx.notify()).detach();
                SettingsView {
                    page: page.unwrap_or(Page::Autocomplete),
                }
            })
        });
        match opened {
            Ok(handle) => cx.set_global(OpenWindow(Some(handle))),
            Err(e) => eprintln!("tflick-ui: opening settings: {e}"),
        }
    }
    // We run as an accessory app: activate explicitly so the window takes keyboard focus.
    cx.activate(true);
}

/// Colors of the settings window, following macOS light/dark mode.
struct Palette {
    bg: u32,
    sidebar: u32,
    fg: u32,
    muted: u32,
    line: u32,
    control: u32,
}

fn palette(window: &Window) -> Palette {
    match window.appearance() {
        WindowAppearance::Dark | WindowAppearance::VibrantDark => Palette {
            bg: 0x1e1e22,
            sidebar: 0x26262b,
            fg: 0xf5f5f7,
            muted: 0x9a9aa2,
            line: 0x34343a,
            control: 0x3a3a40,
        },
        _ => Palette {
            bg: 0xffffff,
            sidebar: 0xf3f3f5,
            fg: 0x1d1d1f,
            muted: 0x6e6e73,
            line: 0xe5e5ea,
            control: 0xe5e5ea,
        },
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(window);
        let s = cx.global::<Settings>().clone();
        let sidebar = div()
            .w(px(190.))
            .h_full()
            .flex_none()
            .bg(rgb(p.sidebar))
            .border_r_1()
            .border_color(rgb(p.line))
            .p_3()
            .flex()
            .flex_col()
            .gap_1()
            .children(Page::ALL.map(|page| {
                div()
                    .id(page.title())
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .when(page == self.page, |d| {
                        d.bg(rgb(ACCENT)).text_color(rgb(0xffffff))
                    })
                    .child(page.title())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.page = page;
                        cx.notify();
                    }))
            }));
        let rows = match self.page {
            Page::Autocomplete => autocomplete_page(&p, &s),
            Page::Appearance => appearance_page(&p, &s),
            Page::Integrations => integrations_page(&p),
            Page::Keybindings => keybindings_page(&p),
        };
        div()
            .size_full()
            .flex()
            .bg(rgb(p.bg))
            .text_color(rgb(p.fg))
            .font_family(".SystemUIFont")
            .text_sm()
            .child(sidebar)
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .px_8()
                    .py_6()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .mb_2()
                            .child(self.page.title()),
                    )
                    .children(rows),
            )
    }
}

fn autocomplete_page(p: &Palette, s: &Settings) -> Vec<AnyElement> {
    vec![
        row(
            p,
            "Bật gợi ý",
            "Hiện popup gợi ý khi gõ lệnh trong shell được tflick bọc.",
            toggle("enabled", !s.disabled, |_, cx, on| {
                Settings::change(cx, |s| s.disabled = !on)
            }),
        ),
        soon(
            p,
            "Tự chèn dấu cách",
            "Thêm dấu cách sau khi chọn lệnh con hoặc tuỳ chọn.",
            "P4",
        ),
        soon(
            p,
            "Tìm mờ",
            "Khớp gợi ý theo các ký tự rời rạc (fuzzy), không chỉ theo phần đầu.",
            "P4",
        ),
        soon(
            p,
            "Thứ tự sắp xếp",
            "Theo độ liên quan và tần suất dùng, hoặc theo bảng chữ cái.",
            "P4",
        ),
        soon(
            p,
            "Chỉ hiện khi bấm Tab",
            "Popup không tự hiện khi gõ, chỉ hiện khi bấm Tab.",
            "P4",
        ),
        soon(
            p,
            "Tắt cho các lệnh",
            "Các lệnh không hiện gợi ý, ví dụ ssh, vim.",
            "P4",
        ),
        soon(
            p,
            "Thời gian chờ script",
            "Giới hạn thời gian chạy của gợi ý động (git branch, npm scripts…).",
            "P5",
        ),
    ]
}

fn appearance_page(p: &Palette, s: &Settings) -> Vec<AnyElement> {
    vec![
        row(
            p,
            "Giao diện",
            "Màu của popup. Hệ thống: theo chế độ sáng/tối của macOS.",
            segmented(
                p,
                "theme",
                s.theme,
                &[
                    (Theme::System, "Hệ thống"),
                    (Theme::Dark, "Tối"),
                    (Theme::Light, "Sáng"),
                ],
                |s, v| s.theme = v,
            ),
        ),
        row(
            p,
            "Chiều rộng",
            "Chiều rộng popup.",
            stepper(p, "width", s.width, settings::WIDTH, "pt", |s, v| {
                s.width = v
            }),
        ),
        row(
            p,
            "Chiều cao",
            "Chiều cao tối đa của popup.",
            stepper(p, "height", s.height, settings::HEIGHT, "pt", |s, v| {
                s.height = v
            }),
        ),
        row(
            p,
            "Cỡ chữ",
            "Cỡ chữ trong popup.",
            stepper(p, "font", s.font_size, settings::FONT_SIZE, "pt", |s, v| {
                s.font_size = v
            }),
        ),
        row(
            p,
            "Độ mờ nền",
            "Làm mờ những gì nằm sau popup. 0: không mờ.",
            stepper(p, "blur", s.blur, settings::BLUR, "", |s, v| s.blur = v),
        ),
        row(
            p,
            "Độ đậm nền",
            "Nền popup che phía sau nhiều hay ít. 255: đặc hoàn toàn.",
            stepper(p, "tint", s.tint, settings::TINT, "", |s, v| s.tint = v),
        ),
        row(
            p,
            "Khôi phục mặc định",
            "Đưa các mục giao diện về giá trị ban đầu.",
            button(p, "reset", "Khôi phục", |_, cx| {
                Settings::change(cx, |s| {
                    *s = Settings {
                        disabled: s.disabled,
                        ..Settings::default()
                    }
                })
            }),
        ),
    ]
}

fn integrations_page(p: &Palette) -> Vec<AnyElement> {
    let trusted = macos::accessibility_trusted(false);
    let shell = std::env::var("SHELL").unwrap_or_default();
    let mut rows = vec![
        row(
            p,
            "Quyền Accessibility",
            if trusted {
                "Đã cấp: TFlick tìm được cửa sổ terminal và vị trí con trỏ."
            } else {
                "Chưa cấp: popup không biết đặt ở đâu. Bật cho tflick-ui trong System Settings, \
                 rồi khởi động lại tflick-ui."
            },
            if trusted {
                badge(p, "Đã cấp", 0x22c55e).into_any_element()
            } else {
                button(p, "ax", "Mở System Settings", |_, _| {
                    open_url(
                        "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
                    )
                })
                .into_any_element()
            },
        ),
        row(
            p,
            "Mở cùng máy",
            "Tự chạy tflick-ui khi đăng nhập macOS (LaunchAgent trỏ tới file đang chạy).",
            toggle("login", login::enabled(), |window, _, on| {
                if let Err(e) = login::set(on) {
                    eprintln!("tflick-ui: launch at login: {e}");
                }
                window.refresh();
            }),
        ),
        row(
            p,
            "Tích hợp shell",
            &format!(
                "zsh, bash và fish được tflick tự nạp khi khởi động, không cần sửa file cấu hình. \
                 Shell hiện tại: {}.",
                if shell.is_empty() { "?" } else { &shell }
            ),
            badge(p, "Tự động", ACCENT),
        ),
        row(
            p,
            "Tự bọc shell khi mở terminal",
            "Thêm đoạn dưới vào cuối ~/.zshrc (bash: ~/.bashrc) để mỗi cửa sổ terminal mới chạy \
             trong tflick.",
            button(p, "copy", "Sao chép", |_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(AUTOSTART.into()))
            }),
        ),
    ];
    rows.push(
        div()
            .p_3()
            .mb_2()
            .rounded_md()
            .bg(rgb(p.sidebar))
            .font_family("Menlo")
            .text_xs()
            .children(AUTOSTART.lines().map(|l| div().child(l.to_string())))
            .into_any_element(),
    );
    rows.push(row(
        p,
        "File cài đặt",
        &settings::path().display().to_string(),
        button(p, "reveal", "Hiện trong Finder", |_, cx| {
            // Make sure there is a file to show.
            if !settings::path().exists() {
                let _ = cx.global::<Settings>().save();
            }
            let _ = Command::new("open").arg("-R").arg(settings::path()).spawn();
        }),
    ));
    rows
}

fn keybindings_page(p: &Palette) -> Vec<AnyElement> {
    [
        ("Chọn gợi ý", "↑  ↓"),
        ("Chấp nhận gợi ý", "Tab  ↩"),
        ("Đóng popup", "Esc"),
    ]
    .into_iter()
    .map(|(label, keys)| soon(p, label, keys, "P4"))
    .collect()
}

fn open_url(url: &str) {
    let _ = Command::new("open").arg(url).spawn();
}

/// A setting: label and description on the left, its control on the right.
fn row(p: &Palette, label: &str, desc: &str, control: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_6()
        .py_3()
        .border_b_1()
        .border_color(rgb(p.line))
        .child(
            // flex_1 + min_w_0: the text takes the room left by the control, and wraps.
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .child(label.to_string()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(p.muted))
                        .child(desc.to_string()),
                ),
        )
        .child(div().flex_none().child(control))
        .into_any_element()
}

/// A setting of a feature not built yet (`phase` in plans/).
fn soon(p: &Palette, label: &str, desc: &str, phase: &str) -> AnyElement {
    div()
        .opacity(0.5)
        .child(row(
            p,
            label,
            desc,
            badge(p, &format!("Sắp có · {phase}"), p.muted),
        ))
        .into_any_element()
}

fn badge(p: &Palette, text: &str, color: u32) -> impl IntoElement {
    div()
        .px_2()
        .py_0p5()
        .rounded_md()
        .border_1()
        .border_color(rgb(color))
        .text_color(rgb(color))
        .text_xs()
        .bg(rgb(p.bg))
        .child(text.to_string())
}

fn button(
    p: &Palette,
    id: &'static str,
    label: &'static str,
    on_click: fn(&mut Window, &mut App),
) -> impl IntoElement {
    div()
        .id(id)
        .px_3()
        .py_1()
        .rounded_md()
        .bg(rgb(p.control))
        .cursor_pointer()
        .child(label)
        .on_click(move |_, window, cx| on_click(window, cx))
}

fn toggle(id: &'static str, on: bool, set: fn(&mut Window, &mut App, bool)) -> impl IntoElement {
    div()
        .id(id)
        .w(px(38.))
        .h(px(22.))
        .p(px(2.))
        .flex()
        .when(on, |d| d.justify_end())
        .rounded_full()
        .bg(rgb(if on { ACCENT } else { 0x8e8e93 }))
        .cursor_pointer()
        .child(div().size(px(18.)).rounded_full().bg(rgb(0xffffff)))
        .on_click(move |_, window, cx| set(window, cx, !on))
}

fn segmented<T: Copy + PartialEq + 'static>(
    p: &Palette,
    id: &str,
    current: T,
    options: &[(T, &'static str)],
    set: fn(&mut Settings, T),
) -> impl IntoElement {
    div()
        .flex()
        .p(px(2.))
        .rounded_md()
        .bg(rgb(p.control))
        .children(options.iter().map(|&(value, label)| {
            div()
                .id(SharedString::from(format!("{id}-{label}")))
                .px_3()
                .py_1()
                .rounded(px(5.))
                .cursor_pointer()
                .when(value == current, |d| {
                    d.bg(rgb(ACCENT)).text_color(rgb(0xffffff))
                })
                .child(label)
                .on_click(move |_, _, cx| Settings::change(cx, |s| set(s, value)))
        }))
}

/// `- value +` within `(min, max, step)`.
fn stepper(
    p: &Palette,
    id: &str,
    value: u32,
    (min, max, step): (u32, u32, u32),
    unit: &str,
    set: fn(&mut Settings, u32),
) -> impl IntoElement {
    let step_button = |label: &'static str, next: u32| {
        let enabled = next != value;
        div()
            .id(SharedString::from(format!("{id}-{label}")))
            .w(px(28.))
            .h(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(rgb(p.control))
            .when(enabled, |d| {
                d.cursor_pointer()
                    .on_click(move |_, _, cx| Settings::change(cx, |s| set(s, next)))
            })
            .when(!enabled, |d| d.opacity(0.4))
            .child(label)
    };
    div()
        .flex()
        .items_center()
        .gap_2()
        .child(step_button("−", value.saturating_sub(step).max(min)))
        .child(
            div()
                .w(px(64.))
                .text_center()
                .child(format!("{value}{unit}")),
        )
        .child(step_button("+", (value + step).min(max)))
}
