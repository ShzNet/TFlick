# P0 — PTY passthrough

**Mục tiêu:** `tflick` chạy `$SHELL` trong PTY. Người dùng không thấy khác biệt gì so với chạy shell trực tiếp.
**Ước lượng:** 1–2 ngày · **Phụ thuộc:** không

## Tham khảo
- `refs/amazon-q-developer-cli-autocomplete/crates/figterm/src/main.rs` — `launch_shell`, `build_shell_command`, `figterm_main` (dòng ~400–980)
- `.../crates/figterm/src/pty/unix.rs`, `.../term/unix.rs` (raw mode, winsize)

## Việc cần làm
1. **Workspace**
   - `Cargo.toml` workspace (edition 2024, resolver 3), tạo crate `crates/tflick`.
   - `rustup update` nếu toolchain chưa đủ mới cho edition 2024 (gpui sẽ cần sau này).
2. **Chạy shell**
   - Dùng `portable-pty`: `native_pty_system().openpty(size)` với size lấy từ `crossterm::terminal::size()`.
   - Lệnh chạy là `$SHELL -l` khi được gọi làm login shell; mặc định chỉ `$SHELL`. Truyền nguyên env, thêm `TFLICK_SESSION_ID=<uuid>` và `TFLICK=1`.
   - Chống lồng: nếu `TFLICK=1` đã có trong env thì `exec` thẳng shell, không bọc lần nữa.
3. **Chuyển I/O**
   - stdin (raw mode qua `crossterm::terminal::enable_raw_mode`) → PTY master writer.
   - PTY master reader → stdout, flush ngay sau mỗi lần đọc.
   - Dùng 2 thread + `std::sync::mpsc`; chưa cần tokio.
4. **Resize:** bắt SIGWINCH (`signal-hook`), gọi `master.resize(size)` mới.
5. **Thoát:** shell con thoát → khôi phục termios (cả khi panic, dùng guard `Drop`) rồi `exit` với exit code của shell con.
6. **Chạy thử:** thêm `exec tflick` vào cuối `.zshrc` có guard. Mặc định chỉ bật khi đặt `TFLICK_AUTOSTART=1`, để lỡ có lỗi vẫn mở được terminal.

## Kiểm tra (Definition of Done)
- [ ] `vim`, `htop`, `less`, `ssh`, `tmux` chạy bình thường bên trong, kể cả khi resize cửa sổ.
- [ ] Ctrl-C, Ctrl-Z, Ctrl-D hoạt động đúng; `exit 3` thì `echo $?` ở shell ngoài ra 3.
- [ ] `cat /dev/urandom | head -c 50M | base64` không treo hay rơi byte (so độ trễ với khi không bọc).
- [ ] Kill -9 `tflick` xong, `reset` là terminal về bình thường (chấp nhận được).

## Ngoài phạm vi
Parse output, OSC, IPC: làm ở P1/P2.
