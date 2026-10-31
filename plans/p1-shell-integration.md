# P1 — Shell integration + edit buffer

**Mục tiêu:** `tflick` luôn biết người dùng đang ở prompt hay đang chạy lệnh, cwd hiện tại, chuỗi lệnh đang gõ và vị trí cursor trong chuỗi đó.
**Ước lượng:** 3–5 ngày · **Phụ thuộc:** P0

## Cách Q làm (đã đọc code)
- Shell script phát **OSC 697** (`ESC ] 697 ; <payload> BEL`):
  - Ở `precmd`: `Dir=`, `Shell=`, `ShellPath=`, `PID=`, `ExitCode=`, `TTY=`, `User=`, `ZshAutosuggestionColor=`, `OSCUnlock=`.
  - Bọc prompt: `PS1="%{StartPrompt%}$PS1%{EndPrompt NewCmd=<session>%}"`, làm tương tự cho PS2–4, RPS1/2.
  - Ở `preexec`: trả lại prompt gốc của người dùng, phát `OSCLock=` rồi `PreExec`.
  - Nguồn: `refs/.../crates/fig_integrations/src/shell/scripts/{pre.sh,post.zsh,post.bash,bash-preexec.sh,post.fish}`.
- figterm parse các mã đó trong fork alacritty (`refs/.../crates/alacritty_terminal/src/ansi.rs:1175`). Khi gặp `EndPrompt` thì lưu vị trí cursor lúc đó thành `cmd_cursor`.
- **Edit buffer = text trên lưới màn hình** tính từ `cmd_cursor` đến cuối màn hình. Phần đuôi được trim; `cursor_idx` là vị trí cursor hiện tại của terminal tính tương đối (`get_current_buffer`, `.../alacritty_terminal/src/term/mod.rs:755`). Các ô có màu trùng màu autosuggestion bị loại (gợi ý xám của zsh-autosuggestions).
- Chỉ gửi edit buffer khi đang ở prompt, không ở giữa `PreExec` và `NewCmd` (`can_send_edit_buffer`, `figterm/src/main.rs:300`).

## Việc cần làm
1. **Shell scripts (`shell/`)**
   - `tflick.zsh`: port `post.zsh` (bỏ `pre-cmd --alias` và `DoneSourcing`), đổi tên hàm sang tiền tố `tflick_`, vẫn giữ mã OSC 697 (không có lý do để đổi).
   - Lệnh `tflick init zsh` in script ra stdout (`include_str!`), người dùng thêm `eval "$(tflick init zsh)"` vào cuối `.zshrc`.
   - bash/fish để sang P6.
2. **Quét OSC 697**
   - Chạy `vte::Parser` song song trên luồng output, chỉ quan tâm `osc_dispatch` có tham số `697`.
   - Cắt output thành từng đoạn tại ranh giới mỗi OSC 697. Mỗi đoạn được đưa vào `Term` trước, rồi mới xử lý OSC, để vị trí cursor đọc được là đúng lúc OSC xuất hiện.
   - Vẫn chuyển nguyên byte ra terminal thật; terminal sẽ bỏ qua OSC lạ. Nếu terminal nào in rác ra màn hình thì lọc bỏ.
3. **Lưới màn hình**
   - `alacritty_terminal::Term` (0.26) kích thước bằng PTY, resize theo SIGWINCH.
   - `ShellState { in_prompt, cmd_start: Option<Point>, cwd, shell, pid, exit_code, autosuggest_color }`.
4. **Edit buffer**
   - `fn edit_buffer(term, state) -> Option<(String, usize)>`: port logic `get_current_buffer` + `get_text_region`. Nối các dòng bị wrap, trim cuối, bỏ ô màu autosuggestion.
   - Trả `None` khi `cmd_start` đã cuộn ra khỏi màn hình hoặc không ở prompt.
5. **Kênh debug:** `TFLICK_LOG=/tmp/tflick.log` ghi `buffer|cursor` mỗi khi giá trị thay đổi. Dùng `eprintln!` vào file, chưa cần `tracing`.

## Kiểm tra
- [ ] Gõ `git commit -m "abc"`, di chuyển cursor về giữa chuỗi: log ra đúng buffer và đúng index.
- [ ] Prompt nhiều dòng (starship, p10k), có RPROMPT: buffer không lẫn ký tự của prompt.
- [ ] Có zsh-autosuggestions: buffer không chứa phần gợi ý xám.
- [ ] Lệnh dài bị wrap sang dòng dưới: buffer liền mạch.
- [ ] Đang chạy `vim` hay `sleep 10`: không có edit buffer.
- [ ] `cd /tmp` xong thì `cwd` cập nhật thành `/tmp`.
- [ ] Unit test cho hàm edit buffer: dựng `Term` từ chuỗi byte cố định, không cần PTY thật.

## Rủi ro
- `alacritty_terminal` upstream có thể không cho truy cập cell/màu đủ dễ → copy phần cần thiết từ fork của Q.
- Prompt dùng nhiều lệnh di chuyển cursor (p10k instant prompt) → test sớm.
