//! Loads the shell integration (`shell/`) without the user editing their rc files, the way kitty
//! and VSCode do. Shells other than zsh, bash and fish run without it (popup guessed from keys).
//!
//! - zsh: ZDOTDIR points at a directory whose `.zshenv` restores the user's ZDOTDIR, runs their
//!   `.zshenv`, and loads the script at the first prompt, after their `.zshrc` set up the prompt.
//!   zsh reads the remaining startup files from the restored ZDOTDIR.
//! - bash: `--rcfile` with a file that sources `~/.bashrc`, then the script.
//! - fish: `-C` sources the script after `config.fish`.

use std::path::{Path, PathBuf};
use std::{env, fs};

use anyhow::Result;
use portable_pty::CommandBuilder;

const ZSH: &str = include_str!("../../../shell/tflick.zsh");
/// bash-preexec `return`s at the top when already loaded, which would also skip our part (or,
/// eval'ed from .bashrc, return from .bashrc), so guard it from outside.
const BASH: &str = concat!(
    "if [[ -z \"${bash_preexec_imported:-}${__bp_imported:-}\" ]]; then\n",
    include_str!("../../../shell/bash-preexec.sh"),
    "fi\n",
    include_str!("../../../shell/tflick.bash")
);
const FISH: &str = include_str!("../../../shell/tflick.fish");

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

/// ponytail: like a plain `bash` (non-login), only ~/.bashrc; a config kept only in
/// ~/.bash_profile is not read, as without tflick. Emulate a login shell if users need it.
const BASHRC: &str = r#"# Written by tflick. Your ~/.bashrc, then the tflick integration.
[[ -f ~/.bashrc ]] && source ~/.bashrc
source "$TFLICK_INTEGRATION"
"#;

/// The integration script for `tflick init <shell>`.
pub fn script(shell: &str) -> Option<&'static str> {
    match shell {
        "zsh" => Some(ZSH),
        "bash" => Some(BASH),
        "fish" => Some(FISH),
        _ => None,
    }
}

/// Sets up `cmd` (the shell, no arguments yet) to load the integration. `TFLICK_NO_INJECT=1`
/// opts out (loading twice is harmless anyway). Returns the directory to remove once the shell
/// exits.
pub fn inject(cmd: &mut CommandBuilder, shell: &str, session: &str) -> Result<Option<PathBuf>> {
    let name = Path::new(shell)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let Some(script) = script(name) else {
        return Ok(None);
    };
    if env::var_os("TFLICK_NO_INJECT").is_some() {
        return Ok(None);
    }
    let dir = env::temp_dir().join(format!("tflick-{session}"));
    fs::create_dir_all(&dir)?;
    let script_path = dir.join(format!("tflick.{name}"));
    fs::write(&script_path, script)?;
    cmd.env("TFLICK_INTEGRATION", &script_path);
    match name {
        "zsh" => {
            fs::write(dir.join(".zshenv"), ZSHENV)?;
            if let Some(user) = env::var_os("ZDOTDIR") {
                cmd.env("TFLICK_USER_ZDOTDIR", user);
            }
            cmd.env("ZDOTDIR", &dir);
        }
        "bash" => {
            let rc = dir.join("bashrc");
            fs::write(&rc, BASHRC)?;
            cmd.arg("--rcfile");
            cmd.arg(rc);
        }
        _ => {
            // fish: the temp dir has no quotes or spaces in it.
            cmd.arg("-C");
            cmd.arg(format!("source '{}'", script_path.display()));
        }
    }
    Ok(Some(dir))
}
