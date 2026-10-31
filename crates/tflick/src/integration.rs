//! Loads the shell integration without the user editing their rc files.
//!
//! zsh: point ZDOTDIR at a directory whose `.zshenv` restores the user's ZDOTDIR, runs their
//! `.zshenv`, and loads `shell/tflick.zsh` at the first prompt, after their `.zshrc` set up the
//! prompt. zsh reads the remaining startup files from the restored ZDOTDIR, so everything else
//! is untouched. Same idea as kitty's and VSCode's shell integration.

use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::Result;
use portable_pty::CommandBuilder;

pub const ZSH_SCRIPT: &str = include_str!("../../../shell/tflick.zsh");

const ZSHENV: &str = r#"# Written by tflick. Restores your ZDOTDIR, then loads the tflick integration at the first prompt.
if [[ -n "${TFLICK_USER_ZDOTDIR+x}" ]]; then
  ZDOTDIR="$TFLICK_USER_ZDOTDIR"
else
  unset ZDOTDIR
fi
unset TFLICK_USER_ZDOTDIR
[[ -f "${ZDOTDIR:-$HOME}/.zshenv" ]] && source "${ZDOTDIR:-$HOME}/.zshenv"
if [[ -o interactive ]]; then
  tflick_bootstrap() {
    precmd_functions=(${precmd_functions:#tflick_bootstrap})
    source "$TFLICK_INTEGRATION"
    (( $+functions[tflick_precmd] )) && tflick_precmd
  }
  precmd_functions+=(tflick_bootstrap)
fi
"#;

/// Sets up `cmd` to load the integration when `shell` is zsh. `TFLICK_NO_INJECT=1` opts out
/// (e.g. when `.zshrc` has `eval "$(tflick init zsh)"`; loading twice is harmless anyway).
/// Returns the directory to remove once the shell exits.
pub fn inject(cmd: &mut CommandBuilder, shell: &str, session: &str) -> Result<Option<PathBuf>> {
    let is_zsh = Path::new(shell).file_name().is_some_and(|n| n == "zsh");
    if !is_zsh || env::var_os("TFLICK_NO_INJECT").is_some() {
        return Ok(None);
    }
    let dir = env::temp_dir().join(format!("tflick-{session}"));
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(".zshenv"), ZSHENV)?;
    let script = dir.join("tflick.zsh");
    fs::write(&script, ZSH_SCRIPT)?;
    if let Some(user) = env::var_os("ZDOTDIR") {
        cmd.env("TFLICK_USER_ZDOTDIR", user);
    }
    cmd.env("ZDOTDIR", &dir);
    cmd.env("TFLICK_INTEGRATION", &script);
    Ok(Some(dir))
}
