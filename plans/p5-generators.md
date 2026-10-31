# P5 — Generator động

**Mục tiêu:** gợi ý lấy từ trạng thái thật của hệ thống: `git checkout <branch>`, `npm run <script>`, `docker exec <container>`, `kill <pid>`, `ssh <host>`…
**Ước lượng:** 1 tuần · **Phụ thuộc:** P3, P4

## Các loại generator (`Fig.Generator`)
| Trường | Ý nghĩa | Xử lý |
|---|---|---|
| `script: string[] \| string \| (tokens) => ...` | lệnh cần chạy | chạy process con, cwd = cwd của shell (P1) |
| `postProcess(out, tokens)` | chuyển stdout thành danh sách Suggestion | gọi hàm JS trong QuickJS |
| `splitOn` | tách stdout theo ký tự | làm thẳng bằng Rust |
| `custom(tokens, executeCommand, ctx)` | hàm async tự chạy lệnh | QuickJS + expose `executeCommand` từ Rust |
| `template` | `filepaths`/`folders`/`history`/`help` | đã có từ P3; `history` đọc `$HISTFILE` |
| `trigger`, `getQueryTerm` | khi nào chạy lại, lấy phần chuỗi dùng để lọc | port `generators/helpers.ts` |
| `cache: { ttl, strategy, cacheByDirectory }` | cache kết quả | port `generators/cache.ts` |

Tham khảo: `refs/.../packages/autocomplete/src/generators/*.ts` (~380 dòng), `.../state/generators.ts` (200 dòng).

## Việc cần làm
1. **Chạy lệnh**
   - `std::process::Command` với `sh -c` hoặc argv, cwd của shell, env của shell (lấy env lúc spawn là đủ).
   - Timeout mặc định 5s (spec có thể đặt riêng). Kill khi hết giờ hoặc khi buffer đổi khiến kết quả không còn cần.
   - Chạy trên thread pool nhỏ (2–4 thread) để không chặn luồng PTY.
2. **QuickJS async**
   - `custom` trả Promise: Rust expose `executeCommand({command, args, cwd})`, trả về Promise được resolve khi process xong. Dùng `rquickjs` với feature `futures`, hoặc tự chạy job queue.
   - Mọi lời gọi JS đều có interrupt handler (P3) và bắt exception (log rồi trả rỗng).
3. **Luồng cập nhật**
   - Gợi ý tĩnh hiện ngay. Kết quả generator về sau thì merge vào rồi gửi `Show` lại, kèm cờ `loading` để UI hiện spinner nhỏ.
   - Kết quả về muộn (buffer đã đổi): bỏ.
4. **Cache:** key = (spec, generator index, tokens liên quan, cwd nếu `cacheByDirectory`); TTL theo spec, mặc định không cache. Dùng `HashMap` + `Instant`.
5. **Helper không hỗ trợ:** generator `ai` từ `@fig/autocomplete-generators` gọi API của Fig → trả rỗng, log một lần.

## Kiểm tra
- [ ] `git checkout ` → danh sách branch của repo hiện tại; đổi sang repo khác thì danh sách đổi.
- [ ] `npm run ` trong thư mục có `package.json` → các script.
- [ ] `kill ` → danh sách process.
- [ ] Generator chạy `sleep 10` không làm chậm việc gõ; quá 5s thì bị kill.
- [ ] Spec có `postProcess` ném exception → không crash, popup chỉ thiếu phần đó.
- [ ] Gõ `git checkout f` nhanh: kết quả cũ không chen vào sau.
