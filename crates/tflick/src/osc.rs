//! Splits shell output at OSC 697 sequences (`ESC ] 697 ; payload BEL` or `... ESC \`), the
//! shell integration's channel (see `shell/tflick.zsh`). Everything else passes through in order.

const PREFIX: &[u8] = b"\x1b]697;";

/// An unterminated OSC 697 longer than this is not ours: pass it through as plain output.
const MAX_PAYLOAD: usize = 4096;

#[derive(Debug, PartialEq)]
pub enum Piece<'a> {
    Bytes(&'a [u8]),
    Osc(String),
}

/// Keeps a sequence cut in half by a read boundary until the rest arrives.
#[derive(Default)]
pub struct OscSplitter {
    pending: Vec<u8>,
}

impl OscSplitter {
    /// Calls `f` with the plain bytes and the OSC 697 payloads of `bytes`, in order.
    pub fn split(&mut self, bytes: &[u8], mut f: impl FnMut(Piece)) {
        let joined;
        let data = if self.pending.is_empty() {
            bytes
        } else {
            joined = [std::mem::take(&mut self.pending).as_slice(), bytes].concat();
            &joined
        };
        let mut start = 0;
        let mut i = 0;
        while let Some(off) = data[i..].iter().position(|&b| b == 0x1b) {
            let esc = i + off;
            let rest = &data[esc..];
            let osc = if rest.starts_with(PREFIX) {
                Some(terminator(&rest[PREFIX.len()..]))
            } else if PREFIX.starts_with(rest) {
                Some(None) // a prefix cut by the read boundary
            } else {
                None
            };
            match osc {
                Some(Some((len, end_len))) => {
                    emit(&mut f, &data[start..esc]);
                    let payload = &rest[PREFIX.len()..PREFIX.len() + len];
                    f(Piece::Osc(String::from_utf8_lossy(payload).into_owned()));
                    i = esc + PREFIX.len() + len + end_len;
                    start = i;
                }
                Some(None) if rest.len() <= PREFIX.len() + MAX_PAYLOAD => {
                    emit(&mut f, &data[start..esc]);
                    self.pending = rest.to_vec();
                    return;
                }
                _ => i = esc + 1,
            }
        }
        emit(&mut f, &data[start..]);
    }
}

fn emit(f: &mut impl FnMut(Piece), bytes: &[u8]) {
    if !bytes.is_empty() {
        f(Piece::Bytes(bytes));
    }
}

/// Length of the payload and of its terminator (BEL or ESC \), if complete.
fn terminator(body: &[u8]) -> Option<(usize, usize)> {
    body.iter().enumerate().find_map(|(i, &b)| match b {
        0x07 => Some((i, 1)),
        0x1b if body.get(i + 1) == Some(&b'\\') => Some((i, 2)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(s: &mut OscSplitter, bytes: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        s.split(bytes, |p| {
            out.push(match p {
                Piece::Bytes(b) => format!("B:{}", String::from_utf8_lossy(b)),
                Piece::Osc(o) => format!("O:{o}"),
            })
        });
        out
    }

    #[test]
    fn splits_osc_from_output_in_order() {
        let mut s = OscSplitter::default();
        assert_eq!(
            pieces(
                &mut s,
                b"a\x1b]697;Dir=/tmp\x07b\x1b]697;PreExec\x1b\\\x1b[0mc"
            ),
            ["B:a", "O:Dir=/tmp", "B:b", "O:PreExec", "B:\x1b[0mc"]
        );
    }

    #[test]
    fn joins_a_sequence_cut_by_a_read() {
        let mut s = OscSplitter::default();
        assert_eq!(pieces(&mut s, b"a\x1b]69"), ["B:a"]);
        assert_eq!(pieces(&mut s, b"7;NewCmd=x"), Vec::<String>::new());
        assert_eq!(pieces(&mut s, b"\x07b"), ["O:NewCmd=x", "B:b"]);
    }

    #[test]
    fn other_escapes_pass_through() {
        let mut s = OscSplitter::default();
        assert_eq!(
            pieces(&mut s, b"\x1b]0;title\x07\x1b[1m"),
            ["B:\x1b]0;title\x07\x1b[1m"]
        );
    }
}
