@AGENTS.md

## Riêng cho Claude Code
- Trả lời người dùng bằng tiếng Việt.
- Trước khi sửa code của phase nào, đọc `plans/README.md` và `plans/pN-*.md` tương ứng.
- `refs/` rất lớn (~220MB): tìm bằng `grep`/`rg` theo đường dẫn cụ thể trong bảng tham khảo của AGENTS.md, không quét cả thư mục.
- Không tự chạy `tflick` lồng trong phiên Claude Code (cần TTY thật). Muốn kiểm tra PTY thì nhờ người dùng chạy bằng `! cargo run -p tflick`.
