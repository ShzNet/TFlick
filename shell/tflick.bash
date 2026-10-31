# TFlick shell integration for bash. tflick loads it by itself; to load it yourself instead,
# add to the END of ~/.bashrc:  eval "$(tflick init bash)"
# Needs bash-preexec, which `tflick init bash` prints just before this (skipped when the user's
# rc files already loaded it, e.g. for atuin).
# port: refs/amazon-q-developer-cli-autocomplete/crates/fig_integrations/src/shell/scripts/post.bash

# Only inside tflick (TFLICK_* leak into shells started elsewhere), and once.
if [[ -n "${BASH:-}" && -n "${TFLICK_SESSION_ID:-}" && "${TFLICK_TTY:-}" == "$(tty 2>/dev/null)" && -z "${TFLICK_BASH_LOADED:-}" ]]; then
TFLICK_BASH_LOADED=1

TFLICK_LAST_PS1="$PS1"
TFLICK_LAST_PS2="$PS2"

# shellcheck disable=SC2059
function tflick_osc { printf "\033]697;$1\007" "${@:2}"; }

function __tflick_preexec() {
  tflick_osc PreExec
  # Restore the user's prompts, unless they changed them since we set ours.
  if [[ -n "${TFLICK_USER_PS1+x}" && "${PS1}" = "${TFLICK_LAST_PS1}" ]]; then
    TFLICK_LAST_PS1="${TFLICK_USER_PS1}"
    PS1="${TFLICK_USER_PS1}"
  fi
  if [[ -n "${TFLICK_USER_PS2+x}" && "${PS2}" = "${TFLICK_LAST_PS2}" ]]; then
    TFLICK_LAST_PS2="${TFLICK_USER_PS2}"
    PS2="${TFLICK_USER_PS2}"
  fi
  _tflick_done_preexec="yes"
}

function __tflick_preexec_preserve_status() {
  __tflick_ret_value="$?"
  __tflick_preexec "$@"
  __bp_set_ret_value "${__tflick_ret_value}" "${__bp_last_argument_prev_command:-}"
}

function __tflick_pre_prompt() {
  __tflick_ret_value="$?"

  tflick_osc "Dir=%s" "${PWD}"
  tflick_osc "Shell=bash"
  tflick_osc "PID=%d" "$$"
  tflick_osc "ExitCode=%s" "$__tflick_ret_value"

  # ^C while typing a command: preexec never ran.
  [[ -z "${_tflick_done_preexec:-}" ]] && __tflick_preexec ""
  _tflick_done_preexec=""

  __bp_set_ret_value "${__tflick_ret_value}" "${__bp_last_argument_prev_command:-}"
}

function __tflick_post_prompt() {
  __tflick_ret_value="$?"

  __tflick_reset_hooks

  # The user (or a theme) set new prompts: remember them.
  if [[ -z "${TFLICK_USER_PS1+x}" || "${PS1}" != "${TFLICK_LAST_PS1}" ]]; then
    TFLICK_USER_PS1="${PS1}"
  fi
  if [[ -z "${TFLICK_USER_PS2+x}" || "${PS2}" != "${TFLICK_LAST_PS2}" ]]; then
    TFLICK_USER_PS2="${PS2}"
  fi

  # \[...\] tells bash the codes take no space on screen.
  local START_PROMPT="\[$(tflick_osc StartPrompt)\]"
  local END_PROMPT="\[$(tflick_osc EndPrompt)\]"
  local NEW_CMD="\[$(tflick_osc "NewCmd=${TFLICK_SESSION_ID}")\]"

  # Reset $? first in case the prompt shows it.
  __bp_set_ret_value "${__tflick_ret_value}" "${__bp_last_argument_prev_command:-}"
  PS1="${START_PROMPT}${TFLICK_USER_PS1}${END_PROMPT}${NEW_CMD}"
  PS2="${START_PROMPT}${TFLICK_USER_PS2}${END_PROMPT}"

  TFLICK_LAST_PS1="${PS1}"
  TFLICK_LAST_PS2="${PS2}"
}

# PROMPT_COMMAND = [existing..., __tflick_post_prompt, __bp_interactive_mode], and our
# pre-prompt first in precmd_functions, preexec first in preexec_functions.
__tflick_reset_hooks() {
  local existing_prompt_command
  # shellcheck disable=SC2128
  existing_prompt_command="${PROMPT_COMMAND}"
  existing_prompt_command="${existing_prompt_command//__tflick_post_prompt[;$'\n']}"
  existing_prompt_command="${existing_prompt_command//__tflick_post_prompt}"
  existing_prompt_command="${existing_prompt_command//__bp_interactive_mode[;$'\n']}"
  existing_prompt_command="${existing_prompt_command//__bp_interactive_mode}"
  __bp_sanitize_string existing_prompt_command "$existing_prompt_command"

  # shellcheck disable=SC2178
  PROMPT_COMMAND=""
  if [[ -n "${existing_prompt_command:-}" ]]; then
    # shellcheck disable=SC2179
    PROMPT_COMMAND+=${existing_prompt_command}$'\n'
  fi
  # shellcheck disable=SC2179
  PROMPT_COMMAND+=$'__tflick_post_prompt\n'
  # shellcheck disable=SC2179
  PROMPT_COMMAND+='__bp_interactive_mode'

  if [[ ${precmd_functions[0]} != __tflick_pre_prompt ]]; then
    for index in "${!precmd_functions[@]}"; do
      if [[ ${precmd_functions[$index]} == __tflick_pre_prompt ]]; then
        unset -v 'precmd_functions[$index]'
      fi
    done
    precmd_functions=(__tflick_pre_prompt "${precmd_functions[@]}")
  fi

  if [[ ${preexec_functions[0]} != __tflick_preexec_preserve_status ]]; then
    for index in "${!preexec_functions[@]}"; do
      if [[ ${preexec_functions[$index]} == __tflick_preexec_preserve_status ]]; then
        unset -v 'preexec_functions[$index]'
      fi
    done
    preexec_functions=(__tflick_preexec_preserve_status "${preexec_functions[@]}")
  fi
}

# bash-preexec installed itself when sourced (just before this); installing it a second time
# would make its own install string clear the DEBUG trap again. Q's post.bash does that.
__tflick_reset_hooks

fi
