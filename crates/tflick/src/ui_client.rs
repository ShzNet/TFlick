//! Connection to the `tflick-ui` daemon.

use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use tflick_ipc::ToUi;

#[derive(Default)]
pub struct UiClient {
    conn: Option<UnixStream>,
}

impl UiClient {
    /// Never blocks the PTY for long: short write timeout, and a missing or dead UI is ignored
    /// (reconnected on the next message).
    pub fn send(&mut self, msg: &ToUi) {
        if self.conn.is_none() {
            self.conn = UnixStream::connect(tflick_ipc::socket_path()).ok();
            if let Some(c) = &self.conn {
                let _ = c.set_write_timeout(Some(Duration::from_millis(20)));
            }
        }
        let Some(conn) = &mut self.conn else { return };
        let mut line = serde_json::to_vec(msg).expect("ToUi serializes");
        line.push(b'\n');
        if conn.write_all(&line).is_err() {
            self.conn = None;
        }
    }
}
