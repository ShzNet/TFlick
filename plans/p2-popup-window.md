# P2 — Cửa sổ popup gpui + định vị

**Mục tiêu:** daemon `tflick-ui` hiện một popup đẹp (blur, bo góc), nổi trên terminal, không lấy focus, **bám đúng vị trí con trỏ** khi gõ. Ở phase này dùng dữ liệu giả.
**Ước lượng:** 1–1,5 tuần · **Phụ thuộc:** spike không phụ thuộc gì; phần nối với `tflick` cần P1 (vị trí cursor).
**Đây là rủi ro lớn nhất của dự án, nên làm spike trước tiên.**

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
- **Nếu không đạt:** quay lại cân nhắc AppKit thuần (`objc2-app-kit` NSPanel + NSVisualEffectView), rồi cập nhật D2 trong `README.md`.

### 2b. Daemon `tflick-ui`
- Crate `crates/tflick-ui`, ẩn icon Dock (`NSApplicationActivationPolicyAccessory`).
- Socket server `$TMPDIR/tflick-$UID/ui.sock`, mỗi `tflick` mở một kết nối. Socket để quyền 0600.
- Crate `tflick-ipc` (1 file, serde):
  ```rust
  enum ToUi   { Show { session: String, items: Vec<Item>, selected: usize, cursor: (u16,u16), grid: (u16,u16), cell_px: Option<(u16,u16)> }, Hide { session: String } }
  enum FromUi { Accept { session: String, index: usize } }
  struct Item { name: String, description: Option<String>, kind: Kind, matched: Vec<u32> }
  ```
- Session gửi `Show` gần nhất là session được hiển thị. `Hide` chỉ có tác dụng khi đúng session đang hiển thị.
- `tflick` tự spawn `tflick-ui` nếu không kết nối được; sau này launchd sẽ lo việc này (P6).

### 2c. Định vị: cách mặc định
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
6. Cần quyền Accessibility: kiểm tra bằng `AXIsProcessTrustedWithOptions` (prompt = true) khi khởi động, nếu chưa có thì in hướng dẫn.

### 2d. Ẩn popup
- Terminal mất focus (theo dõi notification `NSWorkspaceDidActivateApplication`), cửa sổ di chuyển hoặc resize, hoặc nhận `Hide`.

## Kiểm tra
- [ ] Spike đạt các mục tiêu đo ở 2a.
- [ ] `tflick` gửi danh sách giả mỗi khi edit buffer đổi: popup nằm ngay dưới con trỏ, lệch dưới nửa ô chữ, ở iTerm2, Terminal.app, Ghostty, WezTerm, kitty.
- [ ] Gõ ở dòng cuối màn hình: popup lật lên trên.
- [ ] Hai màn hình, màn hình Retina và thường: toạ độ vẫn đúng (chú ý gốc toạ độ của AX ở góc trên trái, của Cocoa ở góc dưới trái).
- [ ] Chuyển sang app khác: popup ẩn trong vòng 100ms.
- [ ] Theme sáng/tối theo hệ thống.

## Ngoài phạm vi
Caret thật qua AX (Terminal.app, xterm.js): để P6.
