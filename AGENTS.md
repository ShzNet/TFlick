# AGENTS.md

Hướng dẫn cho AI agent (Claude Code, Codex, Cursor…) làm việc trong repo này.

## Dự án
TFlick: terminal autocomplete kiểu Fig / Amazon Q, viết bằng Rust, ưu tiên macOS.
- `tflick`: bọc shell trong PTY, đọc edit buffer qua OSC 697 + lưới màn hình, parse lệnh, chạy spec bằng QuickJS, chặn phím khi popup đang mở.
- `tflick-ui`: daemon gpui, vẽ popup nổi cạnh con trỏ (`WindowKind::PopUp`, không lấy focus).
- Hai process nói chuyện bằng JSON lines qua unix socket (`crates/tflick-ipc`).

**Đọc `plans/README.md` trước**, rồi đọc file của phase đang làm (`plans/pN-*.md`). Kiến trúc, các quyết định (D1–D7) và rủi ro đều ở đó.

## Cấu trúc
```
crates/tflick/      pty, term, keys, parser, specs, engine (bin + lib)
crates/tflick-ui/   gpui popup, định vị, Accessibility
crates/tflick-ipc/  kiểu message dùng chung
shell/              tflick.zsh/.bash/.fish, nhúng bằng include_str!
specs/              spec JS đã compile (sinh ra, không commit)
plans/              plan theo phase
refs/               repo tham khảo, CHỈ ĐỌC, không sửa, không build
```

## Lệnh
```sh
cargo build                      # build workspace
cargo test                       # unit + golden test
cargo clippy --all-targets -- -D warnings
cargo fmt
cargo run -p tflick              # chạy shell bọc trong PTY
cargo run -p tflick -- complete -- "git checkout -"   # debug gợi ý, không cần UI
cargo run -p tflick-ui           # chạy daemon popup
scripts/build-specs.sh           # compile spec từ refs/autocomplete → specs/
```
Log: đặt `TFLICK_LOG=/tmp/tflick.log`.

## Nguồn tham khảo khi port
Port từ code có sẵn trong `refs/` thay vì tự nghĩ lại từ đầu. Khi port, ghi file nguồn vào doc comment, ví dụ `// port: packages/autocomplete-parser/src/parseArguments.ts`.

| Việc | Xem ở |
|---|---|
| PTY, vòng lặp chính, edit buffer | `refs/amazon-q-developer-cli-autocomplete/crates/figterm/src/main.rs` |
| OSC 697 | `.../crates/alacritty_terminal/src/ansi.rs:1175`, `term/mod.rs:755` |
| Shell hooks | `.../crates/fig_integrations/src/shell/scripts/` |
| Chặn phím, chèn text | `.../crates/figterm/src/interceptor.rs`, `message.rs:162` |
| Accessibility, caret | `.../crates/macos-utils/src/`, `.../crates/fig_desktop/src/platform/macos.rs` |
| Parser | `.../packages/shell-parser/src/`, `.../packages/autocomplete-parser/src/` |
| Gợi ý, chèn, generator | `.../packages/autocomplete/src/{suggestions,state,generators}/` |
| Kiểu spec | `refs/autocomplete-tools/types/index.d.ts` |
| gpui | `~/.cargo/registry/src/*/gpui-0.2.2/` (`examples/`, `src/platform/mac/window.rs`) |

## Quy ước
- Rust edition 2024. Dùng `anyhow` trong bin, `thiserror` chỉ khi caller thật sự cần match lỗi.
- Không thêm crate mới khi stdlib hoặc crate đã có làm được. Thêm crate mới thì ghi lý do vào commit.
- Mọi `unsafe` (objc2, AX, termios) phải có comment `// SAFETY:` và gom vào module `macos`/`sys`.
- Luồng PTY **không bao giờ được chặn**: chạy generator, gọi JS, gọi IPC đều phải ở thread khác hoặc có timeout. Shell bị treo là lỗi nghiêm trọng nhất.
- Lỗi trong spec hoặc JS: log rồi bỏ qua, tuyệt đối không panic.
- Logic thuần (parser, edit buffer, insertion) phải có unit test. Test parser ưu tiên port từ test TS trong `refs/`.
- Chỗ cố ý làm đơn giản và có giới hạn đã biết thì đánh dấu `// ponytail: <giới hạn>, <cách nâng cấp>`.
- Comment và doc trong code viết bằng tiếng Anh; plan và tài liệu cho người viết bằng tiếng Việt.

## Khi làm xong một việc
- `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` phải pass.
- Tick checklist trong `plans/pN-*.md`. Nếu đổi quyết định thì cập nhật bảng "Quyết định đã chốt" trong `plans/README.md`.
- Thay đổi liên quan đến PTY hoặc phím phải được thử tay trong terminal thật. Ghi lại đã thử ở terminal nào.
