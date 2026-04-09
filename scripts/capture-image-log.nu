#!/usr/bin/env nu
#
# capture-image-log.nu — reproduce the inline-image smearing bug and
# capture a fresh debug log from Helix's raw_content pipeline.
#
# Usage:
#   nu scripts/capture-image-log.nu
#   nu scripts/capture-image-log.nu examples/simple.ipynb
#
# Key design choices (learned the hard way):
#
#   - Helix uses `fern`, not `env_logger`. `RUST_LOG` is ignored. Log
#     verbosity is controlled by `-v` / `-vv` / `-vvv` on the command
#     line (0=Warn, 1=Info, 2=Debug, 3+=Trace). We pass `-vv` so the
#     `log::debug!` lines we care about (`raw_content.add_or_replace`,
#     `raw_content.remap`, `raw_content.clear`) actually land.
#
#   - Helix writes logs to `~/.cache/helix/helix.log` (see
#     `helix_loader::default_log_file`). fern APPENDS by default, so
#     the file accumulates across runs and mixes output from old
#     binaries with output from new ones. We truncate it before every
#     run so the tail we print is guaranteed to come from THIS run.
#
#   - The nushell `out+err>` redirect is useless here because fern
#     writes to a file handle, not stdout/stderr. We just let Helix
#     do its thing and then read the log file back at the end.

def main [
    file: path = examples/simple.ipynb   # .ipynb or .jl to open
] {
    let hx = ($env.HOME | path join ".local/bin/hx")
    let log = ($env.HOME | path join ".cache/helix/helix.log")

    if not ($hx | path exists) {
        print -e $"error: ($hx) does not exist — run `just install` or rebuild the helix fork first"
        exit 1
    }

    print $"helix binary : ($hx)"
    print $"version      : (^$hx --version | str trim)"
    print $"log file     : ($log)"
    print $"target file  : ($file)"
    print ""

    # Kill any lingering julia-lsp so we don't race the monkey patch.
    ^pkill -f LanguageServer | complete | ignore

    # Truncate the log before the run so the tail we read at the end
    # is guaranteed to be from THIS invocation, not accumulated from
    # previous runs by fern's append-only behaviour.
    if ($log | path exists) {
        "" | save -f $log
    } else {
        mkdir ($log | path dirname)
        "" | save -f $log
    }

    print "Launching Helix with -vv (Debug verbosity). Reproduce the smearing"
    print "in the TUI (open a cell with a plot, execute it), then exit with :q!."
    print "The raw_content lifecycle will print here when Helix closes."
    print ""

    # -vv gives us Debug level. That's what our `log::debug!` calls
    # need to be visible.
    ^$hx -vv $file

    print ""
    print "=== raw_content lifecycle ==="
    if ($log | path exists) {
        let lines = (open $log | lines)
        let raw = ($lines | where ($it | str contains "raw_content"))
        let raw_count = ($raw | length)
        let by_kind = (
            $raw
            | each { |l|
                if ($l | str contains "add_or_replace") { "add_or_replace" }
                else if ($l | str contains "raw_content.add:") { "add" }
                else if ($l | str contains "raw_content.set") { "set" }
                else if ($l | str contains "raw_content.clear") { "clear" }
                else if ($l | str contains "raw_content.remap") { "remap" }
                else { "other" }
            }
            | uniq -c
        )
        print $"total raw_content lines: ($raw_count)"
        print "by kind:"
        print $by_kind
        print ""
        print "=== tail (last 40 raw_content lines) ==="
        $raw | last 40 | each { print $in }
    } else {
        print -e $"error: log file ($log) was not written"
        exit 1
    }

    print ""
    print $"full log: ($log)"
    print "useful follow-ups:"
    print $"  grep 'raw_content' ($log) | less"
    print $"  grep 'flush' ($log) | less"
}
