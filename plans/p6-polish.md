# P6 — Hoàn thiện

**Mục tiêu:** từ chỗ "chạy được" thành "dùng sướng". Làm theo thứ tự ưu tiên, mỗi mục là một PR riêng.
**Phụ thuộc:** P4 (một số mục cần P5)

## Danh sách (theo ưu tiên)
1. **Xếp hạng + fuzzy**
   - `nucleo-matcher` cho fuzzy, có tô các ký tự khớp (`Item.matched`).
   - Port `suggestions/sorting.ts` (83 dòng): `priority`, khớp chính xác hay khớp prefix, tần suất dùng (lưu số lần chọn vào `~/.local/state/tflick/usage.json`).
2. **Cài đặt + tự khởi động**
   - `tflick install`: thêm `eval "$(tflick init zsh)"` vào `.zshrc` (backup trước khi sửa), tạo LaunchAgent `~/Library/LaunchAgents/dev.tflick.ui.plist` cho `tflick-ui`, hướng dẫn cấp quyền Accessibility.
   - `tflick uninstall` gỡ sạch.
   - `tflick doctor`: kiểm tra shell integration, socket, quyền AX, spec.
3. ✅ **bash + fish** (làm sớm ở P1, xem `p1-shell-integration.md`; fish chưa thử tay)
   - Port `post.bash` + `bash-preexec.sh`, `pre.fish` + `post.fish`; `tflick init bash|fish`.
4. **Caret chính xác qua Accessibility**
   - Port `macos-utils/src/caret_position.rs` (`get_caret_position`) cho Terminal.app.
   - Port `find_x_term_caret_tree` / `get_x_term_cursor_elem` (`fig_desktop/src/platform/macos.rs:226,697`) cho VSCode/Hyper (xterm.js).
   - Dùng cách này khi có, các trường hợp còn lại dùng cách tính của P2.
5. **Cấu hình** `~/.config/tflick/config.toml`: bật/tắt, phím chấp nhận (Tab/Enter), số dòng hiển thị, theme, offset theo terminal, danh sách lệnh bỏ qua.
6. **Hiệu năng:** đo thời gian từ khi gõ phím đến khi popup cập nhật (mục tiêu < 30ms với spec tĩnh); `tflick` khi rảnh không tốn CPU.
7. **Spec riêng của người dùng:** đọc thêm `~/.config/tflick/specs/*.js`, ưu tiên hơn spec mặc định.
8. **Cập nhật spec:** `tflick update-specs` kéo bản build mới.

## Cân nhắc sau (chưa lên lịch)
- tmux: lấy vị trí pane để định vị đúng.
- SSH remote: chuyển tiếp `ui.sock` qua `ssh -R`.
- Linux (gpui có hỗ trợ X11/Wayland; Accessibility thay bằng AT-SPI, hoặc chỉ dùng cell size).
- Input method kiểu `fig_input_method` cho các terminal mà AX lệch.

## Kiểm tra
Mỗi mục có checklist riêng trong PR. Kiểm tra chung: dùng hằng ngày 1 tuần trên iTerm2 + Ghostty + VSCode, không phải tắt tflick lần nào.

## Đã làm sớm
- ✅ **Icon trên thanh menu** (logo SHZ dạng template, `icons/shz-menubar.png` vẽ từ `icons/shz-mark.svg`; `macos/status_item.rs`): Cài đặt TFlick… (⌘,), Thoát. Menu gửi `OpenSettings` vào cùng kênh với socket.
- ✅ **Cửa sổ cài đặt** kiểu dashboard của Q (từ icon thanh menu hoặc `tflick settings`, `crates/tflick-ui/src/settings_view.rs`): trang Gợi ý, Giao diện, Tích hợp, Phím tắt. Lưu `~/.config/tflick/settings.json`, key giống Q. Mục của P4/P5 hiện mờ "Sắp có", không lưu giá trị giả. Khi `tflick` cần đọc cài đặt (P4), chuyển `Settings` sang `tflick-ipc`.

