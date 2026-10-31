# P4 — Gợi ý thật + chọn + chèn text

**Mục tiêu:** dùng được hằng ngày ở mức cơ bản: gõ `git ch` → popup hiện `checkout`, `cherry-pick`… → ↓ chọn → Tab/Enter chèn vào dòng lệnh.
**Ước lượng:** 1 tuần · **Phụ thuộc:** P1, P2, P3

## Tham khảo Q
- `refs/.../packages/autocomplete/src/suggestions/index.ts` (519 dòng), `helpers.ts` — lọc theo `search_term`, khử trùng lặp, `hidden`, `priority`.
- `.../state/insertion.ts` (326 dòng) — tính text cần chèn: thêm dấu cách hoặc `/` ở cuối, `insertValue` với `{cursor}`, escape ký tự đặc biệt.
- `refs/.../crates/figterm/src/message.rs:162` — figterm chèn bằng cách so `insertion_buffer` với buffer hiện tại, gửi `\x08` (backspace) để xoá phần thừa rồi ghi text vào PTY.
- `refs/.../crates/figterm/src/interceptor.rs` — chặn phím khi popup đang hiện. Lưu ý mũi tên ở application mode (`ESC O A` thay vì `ESC [ A`).

## Việc cần làm
1. **Engine** (`crates/tflick/src/engine.rs`)
   - Khi edit buffer đổi: parse (P3) → sinh gợi ý → lọc theo `search_term` (prefix trước; fuzzy để P6) → tối đa 50 mục → gửi `Show`, hoặc `Hide` nếu rỗng.
   - Debounce khoảng 20ms. Không gửi lại nếu danh sách và vị trí không đổi.
   - Không hiện popup khi: buffer rỗng, vừa chạy lệnh, hoặc cursor không ở cuối token đang gõ (Fig cũng như vậy).
2. **Chặn phím** (`crates/tflick/src/keys.rs`)
   - Chỉ chặn khi popup đang hiện: `Tab`, `Enter` (chỉ khi người dùng đã chủ động di chuyển lựa chọn, để Enter vẫn chạy được lệnh như bình thường), `↑/↓` (cả `ESC [ A` và `ESC O A`), `Esc`, `Ctrl-N/P`.
   - Esc đóng popup cho đến khi buffer đổi tiếp. Các phím khác chuyển nguyên cho shell.
   - Cẩn thận khi tách phím Esc khỏi chuỗi escape: dùng timeout ngắn (~10ms) như vim.
3. **Chèn text**
   - Port `insertion.ts`: ra `(delete_count, text, cursor_back)`.
   - Ghi vào PTY: `\x08` × delete_count + text + `ESC [ D` × cursor_back (cho `{cursor}`).
   - Sau khi chèn: là subcommand/folder thì giữ popup để gợi ý tiếp; là kết quả cuối thì ẩn.
4. **Giao diện popup** (`tflick-ui`)
   - Mỗi dòng gồm: icon theo `Kind` (subcommand, option, arg, file, folder, special), tên với các ký tự khớp tô đậm, mô tả mờ bên phải (cắt bớt nếu dài).
   - Dòng đang chọn tô nền; tự cuộn khi ra khỏi vùng nhìn thấy (`uniform_list`).
   - Tối đa ~8 dòng hiển thị, rộng 300–500px tuỳ nội dung; theme sáng/tối; font monospace cho tên.
   - Click chuột vào dòng → `FromUi::Accept`.

## Kiểm tra
- [ ] `git ch` + Tab → `git checkout ` (có dấu cách cuối).
- [ ] `cd Doc` + Tab → `cd Documents/` và popup hiện tiếp nội dung bên trong.
- [ ] `git commit -m` chọn xong thì cursor đứng sau `-m `.
- [ ] Popup đang mở, nhấn Enter khi chưa di chuyển lựa chọn → lệnh chạy bình thường.
- [ ] Esc đóng popup; gõ tiếp thì popup hiện lại.
- [ ] Trong `vim`/`less`, ↑↓ không bao giờ bị chặn.
- [ ] Gõ nhanh (dán một đoạn dài): không mất ký tự, popup không nhảy loạn.
