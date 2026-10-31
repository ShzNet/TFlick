//! Cursor position reports: we ask the real terminal where its cursor is (DSR `ESC[6n`) and it
//! answers on stdin (CPR `ESC[row;colR`), which must be stripped before the shell sees it.

/// Asks the terminal for its cursor position; write this to the real terminal (stdout).
pub const CURSOR_REQUEST: &[u8] = b"\x1b[6n";

/// Removes up to `max` cursor position reports (`ESC[row;colR`, 1-based) from `bytes`.
/// Returns the remaining bytes and the reported positions as 0-based (column, row).
/// ponytail: a report split across two reads is passed through to the shell; never seen in
/// practice since terminals write the few bytes at once.
pub fn take_cursor_reports(bytes: &[u8], max: u32) -> (Vec<u8>, Vec<(u16, u16)>) {
    let mut rest = Vec::with_capacity(bytes.len());
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if (found.len() as u32) < max
            && let Some((len, pos)) = parse_cursor_report(&bytes[i..])
        {
            found.push(pos);
            i += len;
        } else {
            rest.push(bytes[i]);
            i += 1;
        }
    }
    (rest, found)
}

/// `ESC [ row ; col R` at the start of `bytes` -> (length, (col - 1, row - 1)).
fn parse_cursor_report(bytes: &[u8]) -> Option<(usize, (u16, u16))> {
    let body = bytes.strip_prefix(b"\x1b[")?;
    let end = body.iter().position(|&b| b == b'R')?;
    let (row, col) = std::str::from_utf8(&body[..end]).ok()?.split_once(';')?;
    let (row, col): (u16, u16) = (row.parse().ok()?, col.parse().ok()?);
    Some((2 + end + 1, (col.saturating_sub(1), row.saturating_sub(1))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cursor_reports() {
        let (rest, found) = take_cursor_reports(b"a\x1b[3;4Rb\x1b[10;1R\x1b[A", 5);
        assert_eq!(rest, b"ab\x1b[A");
        assert_eq!(found, vec![(3, 2), (0, 9)]);
        let (rest, found) = take_cursor_reports(b"\x1b[3;4R", 0);
        assert_eq!((rest.as_slice(), found.len()), (&b"\x1b[3;4R"[..], 0));
    }
}
