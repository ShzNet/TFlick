# P2 — Cửa sổ popup gpui + định vị

**Mục tiêu:** daemon `tflick-ui` hiện một popup đẹp (blur, bo góc), nổi trên terminal, không lấy focus, **bám đúng vị trí con trỏ** khi gõ. Ở phase này dùng dữ liệu giả.
**Ước lượng:** 1–1,5 tuần · **Phụ thuộc:** spike không phụ thuộc gì; phần nối với `tflick` cần P1 (vị trí cursor).
**Đây là rủi ro lớn nhất của dự án, nên làm spike trước tiên.**

**Trạng thái (2026-10-31):** ✅ 2a spike đạt, giữ gpui. 🟡 2b + 2c đã có code chạy được (daemon, socket, `tflick-ipc`, định vị bằng AX, unit test cho phép tính vị trí), còn chờ thử trong terminal thật và cấp quyền Accessibility. ⬜ 2d chưa làm.

## Đã kiểm tra trong gpui 0.2.2
- `WindowKind::PopUp` → `NSWindowStyleMaskNonactivatingPanel` + `NSPopUpWindowLevel` (`gpui-0.2.2/src/platform/mac/window.rs:622,805`).
- `WindowOptions { focus: false, show, kind, titlebar: None, window_background: WindowBackgroundAppearance::Blurred, window_bounds, .. }` (`src/platform.rs:1089`).
- Chỉ có `Window::resize`, **không có API dời cửa sổ**. `Window` implement `HasWindowHandle` (`src/window.rs:4845`), qua đó lấy được `AppKitWindowHandle { ns_view }` → `[ns_view window]` → `setFrameOrigin:`.
- Ví dụ nên đọc: `examples/window_positioning.rs`, `examples/uniform_list.rs`, `examples/window_shadow.rs`.

## Tham khảo Q
- `refs/.../crates/fig_desktop/src/platform/macos.rs` — theo dõi app/cửa sổ đang focus, window level (dòng ~480–560), gửi vị trí caret (~690–760).
- `refs/.../crates/macos-utils/src/{accessibility.rs,caret_position.rs,window_server}`.
- `refs/.../packages/autocomplete/src/components/` — tham khảo bố cục và kích thước của popup Fig.

## Việc cần làm

### 2a. Spike (1–2 ngày): làm trước, quyết định có tiếp tục với gpui hay không
- Binary nhỏ: tạo 1 cửa sổ `PopUp`, nền blur, danh sách 8 dòng giả.
- Cứ 50ms lại dời cửa sổ theo vị trí chuột (`setFrameOrigin:` qua objc2), đổi nội dung, ẩn/hiện xen kẽ.
- Đo: có chớp không, CPU khi đứng yên (mục tiêu ~0%), RAM (mục tiêu < 40MB), trễ từ lúc nhận lệnh đến khi vẽ xong (mục tiêu < 16ms).
- Click vào popup: terminal phía sau **không** được mất focus.
- **Kết quả đo lần 1 (2026-10-31, `cargo run --release -p tflick-ui`, M-series, Rust 1.87):** build được; dời cửa sổ và vẽ lại ở 20Hz tốn ~3% CPU; mỗi lần update mất tối đa ~0,3ms; **RSS ~80MB, vượt mục tiêu 40MB**. Chớp hình và việc giữ focus phải kiểm tra bằng mắt.
- Build dùng feature `runtime_shaders` của gpui vì máy chưa có Metal Toolchain. Muốn shader biên dịch sẵn thì chạy `xcodebuild -downloadComponent MetalToolchain` rồi bỏ feature này.
- **Bài học từ spike (giữ lại cho code thật):**
  - **Phải dời NSWindow bên ngoài `update`/`update_in` của gpui.** Dời trong update làm callback resize đồng bộ của AppKit thất bại (Window đang bị mượn), nên `scale_factor` của gpui bị kẹt giá trị cũ khi chuyển giữa màn Retina 2x và màn 1x → nội dung vẽ to gấp đôi hoặc co một nửa. Đã gặp cả hai lỗi, sửa ở `ae568a8`.
  - `Blurred` của gpui là một NSVisualEffectView rất nhẹ và không chỉnh được. Dùng `Transparent` + `CGSSetWindowBackgroundBlurRadius` (API riêng, gpui cũng dùng) để chỉnh được bán kính blur.
  - Chữ trắng cố định bị chìm trên nền sáng → màu theo theme hệ thống (sáng/tối).
  - Giao diện người dùng đã duyệt: blur 30, tint 160 (`TFLICK_BLUR`, `TFLICK_TINT`), icon SVG đơn sắc theo loại, không nền, không viền.
- **Kết luận:** người dùng xác nhận không chớp, chuyển màn hình ổn → giữ gpui (D2). RAM ~80MB vượt mục tiêu nhưng chấp nhận tạm thời.
- **Nếu không đạt:** quay lại cân nhắc AppKit thuần (`objc2-app-kit` NSPanel + NSVisualEffectView), rồi cập nhật D2 trong `README.md`.

### 2b. Daemon `tflick-ui` 🟡
- ✅ Crate `crates/tflick-ui`, ẩn icon Dock (`NSApplicationActivationPolicyAccessory`).
- ✅ Socket server, mỗi `tflick` mở một kết nối. Thực tế dùng `$TMPDIR/tflick-ui.sock` vì `$TMPDIR` trên macOS đã riêng cho từng user (0700), chưa chmod 0600 riêng.
- ✅ Crate `tflick-ipc` (1 file, serde). Khác plan: `cell_px` là `(f32, f32)`, `Item` chưa có `matched`, chưa có `FromUi` (làm cùng P4):
  ```rust
  enum ToUi   { Show { session: String, items: Vec<Item>, selected: usize, cursor: (u16,u16), grid: (u16,u16), cell_px: Option<(u16,u16)> }, Hide { session: String } }
  enum FromUi { Accept { session: String, index: usize } }
  struct Item { name: String, description: Option<String>, kind: Kind, matched: Vec<u32> }
  ```
- ✅ Session gửi `Show` gần nhất là session được hiển thị. `Hide` chỉ có tác dụng khi đúng session đang hiển thị.
- ⬜ `tflick` tự spawn `tflick-ui` nếu không kết nối được (hiện tại: không có UI thì `tflick` lặng lẽ bỏ qua); sau này launchd sẽ lo việc này (P6).

### 2c. Định vị: cách mặc định 🟡
Đã làm: `macos::locate_terminal` (AX) + `caret_in_window` + `popup_origin` (logic thuần, 4 unit test). **Terminal xterm.js (VSCode…):** khung cửa sổ là cả cửa sổ editor nên vô dụng; dùng khung của `xterm-helper-textarea` (phần tử đang focus, nằm đúng ô con trỏ) làm caret, sau khi bật `AXManualAccessibility` (gửi lại mỗi lần chưa tìm thấy, vì Electron dựng cây AX bất đồng bộ). `examples/ax_probe.rs` để dò cây AX khi cần. Khác plan: **chưa có config offset theo terminal**; thay vào đó tự suy ra lề: coi lưới chữ nằm giữa theo chiều ngang, sát đáy với lề bằng lề hai bên, phần dư phía trên là titlebar + tab bar. `cell_px` báo theo device pixel thì tự chia cho scale. Chưa lọc theo danh sách terminal được hỗ trợ.
Phía `tflick` (tạm, trước P1): vị trí con trỏ lấy **từ chính terminal**: sau mỗi đoạn output hỏi `ESC[6n`, terminal trả `ESC[row;colR` trên stdin và `tflick` lọc nó ra trước khi tới shell (chỉ lọc khi đang chờ trả lời, để Shift+F3 vẫn tới shell). Mô hình `alacritty_terminal::Term` bị lệch với màn hình thật (nội dung có trước khi `tflick` chạy, reflow khi resize, `clear`) nên chỉ còn là dự phòng khi terminal không trả lời, và để biết alternate screen; coi là "đang gõ lệnh" khi có phím in được, tắt khi Enter/Ctrl-C/Ctrl-D/Esc; không bao giờ hiện khi ở alternate screen (vim, less). Danh sách gợi ý vẫn là dữ liệu giả.
1. App đang focus: `NSWorkspace.frontmostApplication` → pid, bundle id. Chỉ hiện popup khi đó là terminal nằm trong danh sách hỗ trợ (iTerm2, Terminal.app, Ghostty, WezTerm, kitty, Alacritty, VSCode).
2. Khung cửa sổ: `AXUIElementCreateApplication(pid)` → `AXFocusedWindow` → `AXPosition` + `AXSize`.
3. Kích thước ô chữ:
   - Ưu tiên `ws_xpixel/ws_ypixel` của `TIOCGWINSZ` (tflick đọc trên tty thật rồi gửi qua `cell_px`); iTerm2, kitty, WezTerm và Ghostty có điền giá trị này.
   - Nếu không có thì lấy (khung − offset) / (cols, rows).
4. Offset theo terminal: `~/.config/tflick/config.toml`
   ```toml
   [terminal."com.googlecode.iterm2"]
   top = 28     # titlebar + tab bar
   left = 8     # padding
   ```
   Có sẵn giá trị mặc định cho các terminal phổ biến, người dùng chỉnh thêm nếu cần. `ponytail:` đây là chỗ cần hiệu chỉnh theo thực tế.
5. Toạ độ popup = gốc + (col, row + 1) × cell. Nếu tràn xuống đáy màn hình thì lật popup lên phía trên dòng hiện tại; tràn sang phải thì dịch sang trái.
6. ✅ Cần quyền Accessibility: kiểm tra bằng `AXIsProcessTrustedWithOptions` (prompt = true) khi khởi động, nếu chưa có thì in hướng dẫn.

### 2d. Ẩn popup / đi theo cửa sổ 🟡
Đã làm (chờ thử tay), poll AX mỗi 16ms khi popup đang hiện:
- Ẩn khi focus rời đi: app có focus (hỏi trực tiếp AX system-wide `AXFocusedApplication`; `NSWorkspace.frontmostApplication` là cache, bị trễ, lần thử đầu không ẩn được), cửa sổ focus khác cửa sổ lúc hiện (so bằng CFEqual), hoặc mất caret xterm.js (bấm từ terminal VSCode sang editor).
- Ẩn khi kết nối của `tflick` đóng (đóng tab/cửa sổ, crash).
- Kéo/resize cửa sổ: **kéo thanh tiêu đề thì popup chạy theo chuột** (vị trí popup lúc nhấn + quãng chuột đã đi; `NSEvent.mouseLocation` đọc ngay, không trễ như AX), thả chuột và cửa sổ đứng yên ~100ms thì chỉnh lại theo AX. Resize hoặc dời bằng phím (Rectangle) thì ẩn tới khi đứng yên. Lần 1 đuổi theo khung AX mỗi 50ms: giật vì poll trễ hơn tay kéo. Lần 2–3 ẩn tới khi đứng yên: hết giật nhưng popup biến mất suốt lúc kéo, vì AX cập nhật vị trí cửa sổ đang kéo theo từng đợt, có khoảng nghỉ >50ms.
- Caret xterm.js đổi chỗ trong cùng cửa sổ (đang gõ) thì dời popup ngay, không ẩn.
- Terminal mất focus (theo dõi notification `NSWorkspaceDidActivateApplication`), cửa sổ di chuyển hoặc resize, hoặc nhận `Hide`.

## Kiểm tra
- [x] Spike đạt các mục tiêu đo ở 2a (trừ RAM ~80MB > 40MB, chấp nhận tạm).
- [ ] `tflick` gửi danh sách giả mỗi khi edit buffer đổi: popup nằm ngay dưới con trỏ, lệch dưới nửa ô chữ, ở iTerm2, Terminal.app, Ghostty, WezTerm, kitty.
- [ ] Gõ ở dòng cuối màn hình: popup lật lên trên.
- [x] Spike: chuyển giữa màn Retina và màn 1x không còn vẽ sai tỉ lệ.
- [ ] Hai màn hình, màn hình Retina và thường: toạ độ vẫn đúng (chú ý gốc toạ độ của AX ở góc trên trái, của Cocoa ở góc dưới trái).
- [ ] Chuyển sang app khác: popup ẩn trong vòng 100ms (đã code, chờ thử tay).
- [ ] Kéo cửa sổ terminal: popup đi theo (đã code, chờ thử tay).
- [ ] Đóng tab/cửa sổ terminal: popup ẩn (đã code, chờ thử tay).
- [x] Theme sáng/tối theo hệ thống.

## Ngoài phạm vi
Caret thật qua AX (Terminal.app, xterm.js): để P6.
