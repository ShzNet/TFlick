# TFlick shell integration for fish. tflick loads it by itself; to load it yourself instead,
# add to ~/.config/fish/config.fish:  tflick init fish | source
# port: refs/amazon-q-developer-cli-autocomplete/crates/fig_integrations/src/shell/scripts/post.fish

# Only inside tflick (TFLICK_* leak into shells started elsewhere), and once.
if set --query TFLICK_SESSION_ID; and test "$TFLICK_TTY" = (command tty 2>/dev/null); and not set --query TFLICK_FISH_LOADED
    set --global TFLICK_FISH_LOADED 1

    function tflick_osc
        builtin printf "\033]697;$argv[1]\007" $argv[2..-1]
    end

    function tflick_copy_fn
        functions --erase $argv[2]
        functions --copy $argv[1] $argv[2]
    end

    function tflick_wrap_prompt
        set -l last_status $status
        tflick_osc StartPrompt
        builtin printf "%b" (string join "\n" $argv)
        tflick_osc EndPrompt
        return $last_status
    end

    function tflick_preexec --on-event fish_preexec
        tflick_osc PreExec
        # Restore the user's prompts.
        if functions --query tflick_user_mode_prompt
            tflick_copy_fn tflick_user_mode_prompt fish_mode_prompt
        end
        if functions --query tflick_user_right_prompt
            tflick_copy_fn tflick_user_right_prompt fish_right_prompt
        end
        if functions --query tflick_user_prompt
            tflick_copy_fn tflick_user_prompt fish_prompt
        end
        set --global tflick_has_set_prompt 0
    end

    function tflick_precmd --on-event fish_prompt
        set -l last_status $status

        tflick_osc "Dir=%s" "$PWD"
        tflick_osc "Shell=fish"
        tflick_osc "PID=%d" "$fish_pid"
        tflick_osc "ExitCode=%s" "$last_status"
        tflick_osc "FishSuggestionColor=%s" "$fish_color_autosuggestion"

        # ^C while typing a command: preexec never ran.
        if test $tflick_has_set_prompt = 1
            tflick_preexec
        end

        if functions --query fish_mode_prompt
            tflick_copy_fn fish_mode_prompt tflick_user_mode_prompt
            function fish_mode_prompt
                tflick_wrap_prompt (tflick_user_mode_prompt)
            end
        end

        if functions --query fish_right_prompt
            tflick_copy_fn fish_right_prompt tflick_user_right_prompt
            function fish_right_prompt
                tflick_wrap_prompt (tflick_user_right_prompt)
            end
        end

        tflick_copy_fn fish_prompt tflick_user_prompt
        function fish_prompt
            tflick_wrap_prompt (tflick_user_prompt)
            tflick_osc NewCmd=$TFLICK_SESSION_ID
        end

        set --global tflick_has_set_prompt 1
    end

    set --global tflick_has_set_prompt 0
end
