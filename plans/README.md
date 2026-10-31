# TFlick — Plan tổng

Terminal autocomplete kiểu Fig / Amazon Q, viết lại bằng Rust: **popup đồ hoạ (GPUI) nổi ngay cạnh con trỏ**, dùng nguyên kho spec của `withfig/autocomplete` (~1.480 spec). Ưu tiên macOS.

## Mục lục

| Phase | File | Ước lượng | Kết quả |
|---|---|---|---|
| P0 ✅ | [p0-pty.md](p0-pty.md) | 1–2 ngày | `tflick` bọc shell trong PTY, trong suốt với người dùng |
| P1 | [p1-shell-integration.md](p1-shell-integration.md) | 3–5 ngày | biết prompt, cwd, edit buffer + vị trí cursor |
| P2 🟡 | [p2-popup-window.md](p2-popup-window.md) | 1–1,5 tuần | popup gpui (dữ liệu giả) bám đúng con trỏ |
| P3 | [p3-parser-specs.md](p3-parser-specs.md) | 1–1,5 tuần | parse lệnh + nạp spec qua QuickJS |
| P4 | [p4-suggestions-insert.md](p4-suggestions-insert.md) | 1 tuần | gợi ý thật, chọn bằng phím, chèn text |
| P5 | [p5-generators.md](p5-generators.md) | 1 tuần | generator động (git branch, npm scripts…) |
| P6 | [p6-polish.md](p6-polish.md) | liên tục | xếp hạng, bash/fish, cài đặt, caret AX chính xác |

✅ xong · 🟡 đang làm · ⬜ chưa làm. Trạng thái chi tiết ở đầu file của từng phase.

P2 không phụ thuộc P1 nên có thể làm song song. Vì đây là rủi ro lớn nhất, nên làm spike của P2 sớm nhất có thể.

## Repo tham khảo (`refs/`, chỉ đọc)

| Repo | Dùng gì |
|---|---|
| `amazon-q-developer-cli-autocomplete` (Q CLI v1.19.7, MIT/Apache) | `crates/figterm` (PTY, edit buffer, chặn phím), `crates/alacritty_terminal` (fork có OSC 697), `crates/fig_integrations/src/shell/scripts` (shell hooks), `crates/macos-utils` (Accessibility, caret), `packages/shell-parser`, `packages/autocomplete-parser`, `packages/autocomplete/src/{state,generators,suggestions}` |
| `autocomplete` (withfig, MIT) | 1.484 spec TS → compile ra JS |
| `autocomplete-tools` (withfig) | `types/index.d.ts` (kiểu `Fig.Spec`), compiler spec |
| `amazon-q-developer-cli` | chỉ còn chat, **không dùng** |

Kiro CLI không có repo công khai.

## Kiến trúc

```
 ┌────────── terminal (iTerm2, Terminal.app, Ghostty…) ──────────┐
 │  phím ↓                                          ↑ output     │
 └───────────────────────────────────────────────────────────────┘
        │                                            │
 ┌──────▼────────────── tflick (1 process / 1 shell) ┴───────────┐
 │ pty      portable-pty, chạy $SHELL con                         │
 │ term     alacritty_terminal::Term (lưới màn hình)              │
 │          + quét OSC 697 riêng → prompt, cwd, edit buffer       │
 │ keys     popup mở → chặn Tab/Enter/↑/↓/Esc                     │
 │ parser   shell-parser + autocomplete-parser (port từ TS)       │
 │ specs    rquickjs nạp spec JS đã compile                       │
 │ engine   sinh gợi ý, fuzzy (nucleo-matcher), sort              │
 └──────┬──────────────────────────────────────────────────▲─────┘
        │ JSON lines qua unix socket                       │
        │ Show{items, selected, cursor, grid} / Hide       │ Accept{index}
 ┌──────▼──────────── tflick-ui (1 daemon / user) ─────────┴─────┐
 │ gpui WindowKind::PopUp, focus:false, nền Blurred              │
 │ locate: AX khung cửa sổ terminal + cell size → toạ độ màn hình│
 └───────────────────────────────────────────────────────────────┘
```

**Engine nằm trong `tflick`, UI chỉ để vẽ.** Popup không lấy focus nên phím vẫn đi vào terminal rồi qua `tflick`. `tflick` tự quyết định chặn phím hay không, không phải hỏi UI mỗi lần gõ.

## Cấu trúc thư mục dự kiến

```
TFlick/
├─ Cargo.toml              workspace
├─ crates/
│  ├─ tflick/              bin + lib: pty, term, keys, parser, specs, engine
│  ├─ tflick-ui/           bin: gpui popup, định vị, Accessibility
│  └─ tflick-ipc/          kiểu message JSON dùng chung (1 file)
├─ shell/                  tflick.zsh / .bash / .fish (nhúng bằng include_str!)
├─ specs/                  spec JS đã compile (sinh ra khi build, không commit)
├─ plans/                  plan (thư mục này)
└─ refs/                   repo tham khảo (không commit, không sửa)
```

## Quyết định đã chốt

| # | Quyết định | Lý do | Đã cân nhắc |
|---|---|---|---|
| D1 | Popup đồ hoạ nổi, không vẽ trong terminal | người dùng muốn giao diện đẹp | vẽ ANSI trong terminal (kiểu inshellisense) |
| D2 | UI bằng **gpui 0.2.2** (spike đạt 2026-10-31) | `PopUp` = NonactivatingPanel + PopUpWindowLevel (`gpui-0.2.2/src/platform/mac/window.rs:622,805`), có blur, đa nền tảng | webview tao/wry (nặng), egui (bị loại), AppKit thuần (chỉ chạy macOS) |
| D3 | Dời cửa sổ qua `HasWindowHandle` + objc2 `setFrameOrigin:` | gpui 0.2.2 chỉ có `resize`, chưa có API dời cửa sổ | mỗi lần dời lại tạo cửa sổ mới (chớp) |
| D4 | Chạy spec bằng **rquickjs** trên JS đã compile | 226 spec có `postProcess`/`custom`, 198 spec có `script` | chỉ dùng phần khai báo (mất git branch, npm scripts…) |
| D5 | Lưới màn hình bằng `alacritty_terminal` bản upstream, OSC 697 tự quét riêng | không phải giữ một fork | copy fork của Q (`refs/.../crates/alacritty_terminal`) nếu cách quét riêng không ổn |
| D6 | IPC bằng JSON lines qua unix socket | đơn giản, dễ debug bằng `nc -U` | protobuf như Q |
| D7 | Nhận diện edit buffer từ lưới màn hình (giống Q) | không cần hook ZLE hay readline của từng shell | widget ZLE gửi `$BUFFER` (chỉ chạy được với zsh) |

## Rủi ro

| Rủi ro | Phase | Giảm thiểu |
|---|---|---|
| ~~gpui 0.2.2 cần Rust mới hơn 1.87~~ | P2 | không xảy ra: build được trên 1.87. Cần feature `runtime_shaders` vì máy chưa có Metal Toolchain |
| Popup lệch vị trí ở terminal có padding/tab bar | P2 | offset theo bundle id trong config; ưu tiên `ws_xpixel/ws_ypixel` của TIOCGWINSZ; P6 thêm caret AX |
| Popup chớp hoặc trễ khi dời liên tục | P2 | spike đo trước; chỉ gửi Show khi nội dung hoặc vị trí đổi |
| Edit buffer sai với prompt nhiều dòng, RPROMPT, zsh-autosuggestions | P1 | lọc ô màu autosuggestion (cách của Q), test với oh-my-zsh, starship, p10k |
| Spec phụ thuộc helper `@fig/autocomplete-generators` (ví dụ `ai`) | P3/P5 | esbuild bundle helper vào; generator `ai` coi như không hỗ trợ, trả rỗng |

## Chưa làm (thêm khi cần)
- Input method như `fig_input_method`: chỉ làm khi định vị bằng AX lệch quá nhiều.
- SSH remote (`fig_remote_ipc`), Linux/Windows, inline ghost-text AI, dashboard, telemetry, auto-update.

## Crate
`portable-pty 0.9`, `alacritty_terminal 0.26`, `vte 0.15`, `crossterm`, `rquickjs 0.14`, `nucleo-matcher 0.3`, `serde`/`serde_json`, `gpui 0.2.2`, `raw-window-handle`, `objc2` + `objc2-app-kit 0.3`, `accessibility-sys 0.2`, `core-foundation`.
