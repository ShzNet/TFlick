# TFlick

Gợi ý lệnh cho terminal kiểu Fig / Amazon Q, viết lại bằng Rust: **popup đồ hoạ nổi ngay cạnh con trỏ**, dùng kho spec của [withfig/autocomplete](https://github.com/withfig/autocomplete). Ưu tiên macOS.

Không cần đổi terminal hay shell: TFlick chạy bên trong terminal (iTerm2, Terminal.app, Ghostty, VSCode…) và shell (zsh, bash, fish) bạn đang dùng.

> **Trạng thái:** đang phát triển, chưa dùng hằng ngày được. Popup đã bám đúng con trỏ và đọc được lệnh đang gõ, nhưng **danh sách gợi ý vẫn là dữ liệu giả** (parser và spec ở P3–P5). Chi tiết ở [plans/README.md](plans/README.md).

| Phase | Nội dung | Trạng thái |
|---|---|---|
| P0 | Bọc shell trong PTY | ✅ |
| P1 | Tích hợp shell (OSC 697), đọc lệnh đang gõ | 🟡 chờ thử tay |
| P2 | Popup gpui bám con trỏ, theo cửa sổ khi kéo | 🟡 |
| P3–P5 | Parser, spec, gợi ý thật, chèn text, generator | ⬜ |
| P6 | Đóng gói, cài đặt, hoàn thiện | ⬜ |

## Cách hoạt động

```
 terminal của bạn ──phím──▶ tflick ──▶ shell (zsh/bash/fish)
                  ◀─output─┘   │
                               │ JSON qua unix socket
                               ▼
                           tflick-ui  ──▶ popup nổi cạnh con trỏ
```

- **`tflick`**: chạy shell của bạn trong một PTY và chuyển nguyên input/output, nên bạn không thấy khác gì. Song song, nó dựng lại màn hình để biết bạn đang gõ gì và con trỏ ở đâu. Shell báo prompt ở đâu qua mã OSC 697; script tích hợp được **tự nạp**, không cần sửa `.zshrc`/`.bashrc`.
- **`tflick-ui`**: một daemon cho cả máy, vẽ popup (gpui), dùng Accessibility để tìm cửa sổ terminal và vị trí con trỏ. Popup không lấy focus, phím vẫn đi vào terminal.

## Yêu cầu

- macOS (đã thử trên macOS 26, Apple Silicon).
- Rust **1.87+** (edition 2024, đã build được trên 1.87): `rustup update`.
- Xcode Command Line Tools: `xcode-select --install`. Không cần Metal Toolchain (build với `runtime_shaders`).
- Quyền **Accessibility** cho `tflick-ui` (hỏi ở lần chạy đầu).

## Chạy thử từ source

```sh
git clone https://github.com/ShzNet/TFlick.git
cd TFlick
cargo build --release
```

Cần hai cửa sổ terminal:

```sh
# Cửa sổ 1: daemon popup (để chạy nền)
cargo run --release -p tflick-ui
```

```sh
# Cửa sổ 2: shell được bọc. Gõ vài chữ là popup hiện cạnh con trỏ.
cargo run --release -p tflick
```

Lần đầu macOS sẽ hỏi quyền Accessibility: vào **System Settings → Privacy & Security → Accessibility**, bật cho app terminal đang chạy `tflick-ui` (khi chạy bằng `cargo run`, quyền gắn với app terminal đó), rồi chạy lại `tflick-ui`.

Thoát `tflick` bằng `exit` như shell bình thường.

## Cài đặt

> Chưa có bản đóng gói (`.app`, Homebrew): làm ở P6. Hiện cài bằng `cargo`.

```sh
cargo install --path crates/tflick
cargo install --path crates/tflick-ui
```

Hai file chạy nằm ở `~/.cargo/bin` (đã có trong `PATH` nếu cài Rust bằng rustup).

### Tự bật `tflick-ui` khi đăng nhập

Tạo `~/Library/LaunchAgents/net.shz.tflick-ui.plist` (thay `YOU` bằng tên user):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>net.shz.tflick-ui</string>
  <key>ProgramArguments</key><array><string>/Users/YOU/.cargo/bin/tflick-ui</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
</dict>
</plist>
```

```sh
launchctl load ~/Library/LaunchAgents/net.shz.tflick-ui.plist
```

Chạy qua launchd thì quyền Accessibility gắn với chính file `~/.cargo/bin/tflick-ui`: cấp quyền cho file này (nút **+** trong danh sách Accessibility). Sau mỗi lần cài lại, macOS có thể đòi cấp lại.

### Tự bọc shell khi mở terminal

Thêm vào **cuối** `~/.zshrc` (bash: `~/.bashrc`):

```sh
# TFlick: bọc shell tương tác, trừ khi đã ở trong tflick.
if [[ $- == *i* && "${TFLICK_TTY:-}" != "$(tty)" ]] && command -v tflick >/dev/null; then
  tflick
fi
```

Không dùng `exec tflick` khi còn đang phát triển: nếu `tflick` lỗi, bạn vẫn quay về shell thường thay vì terminal bị đóng.

Fish (`~/.config/fish/config.fish`):

```fish
if status is-interactive; and test "$TFLICK_TTY" != (tty); and command -q tflick
    tflick
end
```

### Gỡ cài đặt

```sh
launchctl unload ~/Library/LaunchAgents/net.shz.tflick-ui.plist
rm ~/Library/LaunchAgents/net.shz.tflick-ui.plist
cargo uninstall tflick tflick-ui
```

Rồi xoá đoạn đã thêm vào `.zshrc`/`.bashrc`/`config.fish`.

## Tích hợp shell

`tflick` tự nạp script cho **zsh, bash, fish** khi khởi động shell, không đụng vào file cấu hình của bạn:

| Shell | Cách nạp |
|---|---|
| zsh | `ZDOTDIR` trỏ sang thư mục tạm; vẫn đọc `.zshenv`, `.zshrc`… của bạn như thường |
| bash | `--rcfile`: đọc `~/.bashrc` rồi nạp script (bash 3.2 của macOS được hỗ trợ) |
| fish | `-C`: nạp script sau `config.fish` |

Shell khác vẫn chạy được, nhưng popup chỉ đoán được lúc bạn gõ từ phím.

Muốn tự nạp từ file cấu hình thay vì để `tflick` làm (ví dụ bạn có `ZDOTDIR` đặc biệt), đặt `TFLICK_NO_INJECT=1` và thêm:

```sh
eval "$(tflick init zsh)"     # cuối ~/.zshrc
eval "$(tflick init bash)"    # cuối ~/.bashrc
tflick init fish | source     # trong config.fish
```

## Biến môi trường

| Biến | Dùng cho | Ý nghĩa |
|---|---|---|
| `TFLICK_LOG=/tmp/tflick.log` | `tflick` | Ghi lệnh đang gõ mỗi khi đổi, dạng `nội dung\|vị trí con trỏ` |
| `TFLICK_NO_INJECT=1` | `tflick` | Không tự nạp script tích hợp shell |
| `TFLICK_DEBUG=1` | `tflick-ui` | In chẩn đoán định vị ra stderr |
| `TFLICK_PIN=1` | `tflick-ui` | Giữ popup khi terminal mất focus (để chụp màn hình) |
| `TFLICK_BLUR=30` | `tflick-ui` | Độ mờ nền phía sau popup |
| `TFLICK_TINT=160` | `tflick-ui` | Độ đậm nền popup, 0–255 |

## Xử lý sự cố

- **Popup không hiện:** `tflick-ui` có đang chạy không, đã có quyền Accessibility chưa? Chạy `TFLICK_DEBUG=1 tflick-ui` để xem lỗi. Với tích hợp shell, popup chỉ hiện khi dòng lệnh có chữ.
- **Popup lệch vị trí:** chạy `TFLICK_DEBUG=1 tflick-ui`, gửi log và tên terminal. Vị trí được suy ra từ khung cửa sổ, terminal có tab bar hay padding lạ có thể lệch.
- **`tflick: already running in this terminal`:** bạn chạy `tflick` bên trong một `tflick` khác. Gõ `exit` hoặc mở cửa sổ mới.
- **Xem lệnh `tflick` đọc được:** `TFLICK_LOG=/tmp/tflick.log tflick`, rồi `tail -f /tmp/tflick.log` ở cửa sổ khác.
- **Terminal bị rối sau khi `tflick` crash:** gõ `reset`.

## Phát triển

```sh
cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test
```

- Cấu trúc code, phân tầng và quy ước: [AGENTS.md](AGENTS.md).
- Kế hoạch theo phase, các quyết định đã chốt: [plans/](plans/).
- Repo tham khảo (Amazon Q, withfig) clone vào `refs/` (không commit), lệnh clone ở AGENTS.md.

## Giấy phép

MIT hoặc Apache-2.0, tuỳ bạn chọn. `shell/bash-preexec.sh` thuộc © Ryan Caloras, giấy phép MIT.
