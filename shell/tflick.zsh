# TFlick shell integration for zsh. Add to the END of ~/.zshrc:
#   eval "$(tflick init zsh)"
# Reports prompt boundaries and shell state to tflick with OSC 697 escape codes.
# port: refs/amazon-q-developer-cli-autocomplete/crates/fig_integrations/src/shell/scripts/post.zsh

# Only inside tflick (TFLICK_* leak into shells started elsewhere, e.g. from `code .`), and once.
if [[ -n "${TFLICK_SESSION_ID:-}" && "${TFLICK_TTY:-}" == "$TTY" && -z "${TFLICK_ZSH_LOADED:-}" ]]; then
TFLICK_ZSH_LOADED=1

# shellcheck disable=SC2059
function tflick_osc { printf "\033]697;$1\007" "${@:2}"; }

TFLICK_HAS_SET_PROMPT=0

tflick_preexec() {
  # Restore the user's prompts before running the command.
  [[ -n "${PS1+x}" ]] && PS1="$TFLICK_USER_PS1"
  [[ -n "${PROMPT+x}" ]] && PROMPT="$TFLICK_USER_PROMPT"
  [[ -n "${prompt+x}" ]] && prompt="$TFLICK_USER_prompt"
  [[ -n "${PS2+x}" ]] && PS2="$TFLICK_USER_PS2"
  [[ -n "${PROMPT2+x}" ]] && PROMPT2="$TFLICK_USER_PROMPT2"
  [[ -n "${RPS1+x}" ]] && RPS1="$TFLICK_USER_RPS1"
  [[ -n "${RPROMPT+x}" ]] && RPROMPT="$TFLICK_USER_RPROMPT"
  TFLICK_HAS_SET_PROMPT=0
  tflick_osc PreExec
}

tflick_precmd() {
  local LAST_STATUS=$?

  tflick_reset_hooks

  tflick_osc "Dir=%s" "$PWD"
  tflick_osc "Shell=zsh"
  tflick_osc "PID=%d" "$$"
  tflick_osc "ExitCode=%s" "${LAST_STATUS}"
  tflick_osc "ZshAutosuggestionColor=%s" "${ZSH_AUTOSUGGEST_HIGHLIGHT_STYLE:-}"

  if [[ "$TFLICK_HAS_SET_PROMPT" -eq 1 ]]; then
    # ^C pressed while typing a command: preexec never ran, restore the prompts now.
    tflick_preexec
  fi

  local START_PROMPT=$'\033]697;StartPrompt\007'
  local END_PROMPT=$'\033]697;EndPrompt\007'
  local NEW_CMD=$'\033]697;NewCmd='"${TFLICK_SESSION_ID}"$'\007'

  # Save the user's prompts (themes may change them between commands).
  TFLICK_USER_PS1="$PS1"
  TFLICK_USER_PROMPT="$PROMPT"
  TFLICK_USER_prompt="$prompt"
  TFLICK_USER_PS2="$PS2"
  TFLICK_USER_PROMPT2="$PROMPT2"
  TFLICK_USER_RPS1="$RPS1"
  TFLICK_USER_RPROMPT="$RPROMPT"

  # %{...%} tells zsh the codes take no space on screen.
  if [[ -n "${PROMPT+x}" ]]; then
    PROMPT="%{$START_PROMPT%}$PROMPT%{$END_PROMPT$NEW_CMD%}"
  elif [[ -n "${prompt+x}" ]]; then
    prompt="%{$START_PROMPT%}$prompt%{$END_PROMPT$NEW_CMD%}"
  else
    PS1="%{$START_PROMPT%}$PS1%{$END_PROMPT$NEW_CMD%}"
  fi

  if [[ -n "${PROMPT2+x}" ]]; then
    PROMPT2="%{$START_PROMPT%}$PROMPT2%{$END_PROMPT%}"
  else
    PS2="%{$START_PROMPT%}$PS2%{$END_PROMPT%}"
  fi

  if [[ -n "${RPROMPT+x}" ]]; then
    RPROMPT="%{$START_PROMPT%}$RPROMPT%{$END_PROMPT%}"
  else
    RPS1="%{$START_PROMPT%}$RPS1%{$END_PROMPT%}"
  fi

  TFLICK_HAS_SET_PROMPT=1
}

# Run last in precmd (after themes set the prompt) and first in preexec.
tflick_reset_hooks() {
  # shellcheck disable=SC1087,SC2193
  if [[ "$precmd_functions[-1]" != tflick_precmd ]]; then
    # shellcheck disable=SC2206,SC2296
    precmd_functions=(${(@)precmd_functions:#tflick_precmd} tflick_precmd)
  fi
  # shellcheck disable=SC1087,SC2193
  if [[ "$preexec_functions[1]" != tflick_preexec ]]; then
    # shellcheck disable=SC2206,SC2296
    preexec_functions=(tflick_preexec ${(@)preexec_functions:#tflick_preexec})
  fi
}

tflick_reset_hooks

fi
