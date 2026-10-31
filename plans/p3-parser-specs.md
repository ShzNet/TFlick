# P3 — Parser + nạp spec

**Mục tiêu:** từ edit buffer + cursor, xác định đang ở spec nào, subcommand nào, đang gõ option hay argument, và danh sách gợi ý tĩnh (subcommand, option, argument có `suggestions`, template `filepaths`/`folders`).
**Ước lượng:** 1–1,5 tuần · **Phụ thuộc:** không (là logic thuần, test độc lập được)

## Khối lượng cần port (TS → Rust)
| File | Dòng | Ghi chú |
|---|---|---|
| `refs/.../packages/shell-parser/src/parser.ts` | 730 | tokenizer/AST bash: quote, escape, `|`, `&&`, `;`, `$( )`, biến |
| `.../shell-parser/src/command.ts` | 236 | lấy lệnh đang được gõ tại vị trí cursor, mở alias |
| `.../autocomplete-parser/src/parseArguments.ts` | 1.144 | đi qua spec: subcommand, option, args, `requiresSeparator`, `isRepeatable`, `parserDirectives`, `loadSpec` lồng nhau |
| `.../autocomplete-parser/src/loadSpec.ts`, `tryResolveSpecToSubcommand.ts`, `loadHelpers.ts` | ~530 | tìm spec theo tên lệnh, spec có nhiều phiên bản (`getVersionCommand`) |
| Test: `shell-parser/tests/*.test.ts` + `fixtures/`, `autocomplete-parser/tests/*.test.ts` | — | port thành golden test |

Kiểu dữ liệu spec: `refs/autocomplete-tools/types/index.d.ts` (`Fig.Spec`, `Fig.Subcommand`, `Fig.Option`, `Fig.Arg`, `Fig.Suggestion`, `Fig.Generator`).

## Việc cần làm
1. **Build spec**
   - `scripts/build-specs.sh`: chạy `pnpm install` + `pnpm build` trong `refs/autocomplete` (esbuild bundle cả helper vào), rồi copy `build/*.js` sang `specs/`.
   - Có thể ghim một commit cụ thể của `withfig/autocomplete` cho ổn định.
2. **QuickJS (`rquickjs` 0.14)**
   - Mỗi `tflick` có một `Runtime` + `Context`, đặt giới hạn bộ nhớ và gas/interrupt handler (mặc định 50ms) để spec lỗi không treo được shell.
   - Nạp spec như một ES module, lấy `default` export.
   - Chuyển spec sang struct Rust: các trường dữ liệu (`name`, `description`, `args`, `options`, `subcommands`, `suggestions`, `template`, `isOptional`, `isVariadic`…) deserialize bằng serde. Các trường là **hàm** (`generators.custom`, `postProcess`, `script` dạng hàm, `loadSpec` dạng hàm, `generateSpec`) giữ lại dưới dạng `rquickjs::Persistent<Function>` để P5 dùng.
   - Cache spec đã nạp theo tên (`HashMap`), nạp lười khi gặp lệnh lần đầu.
3. **shell-parser** → `crates/tflick/src/parser/shell.rs`
4. **autocomplete-parser** → `crates/tflick/src/parser/args.rs`
   - Đầu vào: tokens + spec. Đầu ra: `ParseResult { completion_obj, current_arg, search_term, suggestion_flags, annotations }`, giống `getResultFromState`.
5. **Template tĩnh:** `filepaths`/`folders` đọc thư mục bằng `std::fs::read_dir` theo cwd + phần đường dẫn đã gõ (port `templateSuggestionsGenerator.ts`, 87 dòng).
6. **CLI debug:** `tflick complete -- "git checkout -"` in danh sách gợi ý ra stdout. Dùng để test nhanh, không cần UI.

## Kiểm tra
- [ ] Port được và pass ≥ 90% test case của `shell-parser` và `parseArguments` (case nào bỏ qua thì ghi lý do).
- [ ] `tflick complete -- "git "` → có `checkout`, `commit`…; `"git commit -"` → có `-m`, `--amend`.
- [ ] `"cd "` → thư mục trong cwd; `"cat src/"` → file trong `src/`.
- [ ] `"echo foo | grep -"` → option của `grep`.
- [ ] Nạp toàn bộ ~1.480 spec không spec nào panic (spec nào lỗi thì log ra và bỏ qua).
- [ ] Nạp spec `git` lần đầu < 30ms, các lần sau < 1ms.
