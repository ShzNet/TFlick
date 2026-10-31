//! "Launch at login": a LaunchAgent that starts this tflick-ui binary when the user logs in.

use std::path::PathBuf;
use std::{env, fs, io};

const LABEL: &str = "net.shz.tflick-ui";

fn plist_path() -> PathBuf {
    PathBuf::from(env::var_os("HOME").unwrap_or_default())
        .join(format!("Library/LaunchAgents/{LABEL}.plist"))
}

pub fn enabled() -> bool {
    plist_path().exists()
}

/// Points the agent at the running binary, so rebuild/reinstall before enabling it.
pub fn set(enabled: bool) -> io::Result<()> {
    let path = plist_path();
    if !enabled {
        return match fs::remove_file(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        };
    }
    let exe = env::current_exe()?;
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key><array><string>{}</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#,
        exe.display()
    );
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, plist)
}
