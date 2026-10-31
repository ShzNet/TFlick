//! The unix socket every `tflick` connects to.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixListener;
use std::sync::mpsc::Sender;
use std::thread;

use tflick_ipc::ToUi;

/// Accepts any number of `tflick` connections; every parsed message goes to `tx`. A connection
/// closing becomes a `Hide` for its session.
pub fn listen(tx: Sender<ToUi>) {
    let path = tflick_ipc::socket_path();
    // A stale socket from a crashed run would make bind fail.
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).expect("bind tflick-ui socket");
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let tx = tx.clone();
            thread::spawn(move || {
                let mut session = None;
                for line in BufReader::new(conn).lines() {
                    let Ok(line) = line else { break };
                    match serde_json::from_str::<ToUi>(&line) {
                        Ok(msg) => {
                            if let ToUi::Show { session: s, .. } | ToUi::Hide { session: s } = &msg
                            {
                                session = Some(s.clone());
                            }
                            if tx.send(msg).is_err() {
                                return;
                            }
                        }
                        Err(e) => eprintln!("tflick-ui: bad message: {e}"),
                    }
                }
                // The shell's tflick exited (terminal tab or window closed, or a crash).
                if let Some(session) = session {
                    let _ = tx.send(ToUi::Hide { session });
                }
            });
        }
    });
}
