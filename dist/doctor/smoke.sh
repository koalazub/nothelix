#!/bin/bash
# doctor/smoke.sh — kernel smoke test for `nothelix doctor --smoke`.
#
# Sourced by dist/nothelix. Defines run_kernel_smoke_test().
#
# Spawns a real Julia kernel from the installed kernel-scripts dir,
# executes `1 + 1`, verifies the response, tears it down.

run_kernel_smoke_test() {
    local start_time
    start_time=$(date +%s)

    if ! command -v julia >/dev/null 2>&1; then
        _doctor_fail "kernel smoke: julia not found on PATH"
        return
    fi

    local kernel_scripts="$NOTHELIX_SHARE/kernel-scripts"
    if [ ! -f "$kernel_scripts/runner.jl" ]; then
        _doctor_fail "kernel smoke: $kernel_scripts/runner.jl missing — run 'nothelix upgrade'"
        return
    fi

    local tmp_dir
    tmp_dir=$(mktemp -d -t "nothelix-doctor-smoke.XXXXXX")

    # Copy kernel scripts into the temp dir so runner.jl can include
    # its siblings relative to @__DIR__.
    cp "$kernel_scripts/"*.jl "$tmp_dir/"

    # Spawn the kernel in the background
    (
        cd "$tmp_dir" && julia --startup-file=no --quiet runner.jl "$tmp_dir" \
            > "$tmp_dir/kernel.stdout" 2> "$tmp_dir/kernel.stderr"
    ) &
    local kernel_pid=$!

    # shellcheck disable=SC2064
    trap "kill $kernel_pid 2>/dev/null || true; rm -rf '$tmp_dir'" RETURN

    # Wait for ready file (up to 30s)
    local waited=0
    while [ ! -f "$tmp_dir/ready" ] && [ $waited -lt 30 ]; do
        sleep 1
        waited=$((waited + 1))
    done
    if [ ! -f "$tmp_dir/ready" ]; then
        _doctor_fail "kernel smoke: kernel did not become ready within 30s (stderr: $(head -5 "$tmp_dir/kernel.stderr" 2>/dev/null))"
        return
    fi

    local cold_start=$(($(date +%s) - start_time))
    local exec_start
    exec_start=$(date +%s)

    # Write an input command (runner.jl dispatches on "type", not "command")
    cat > "$tmp_dir/input.json" <<'EOF'
{"type": "execute_cell", "cell_index": 0, "code": "1 + 1"}
EOF

    # Wait for output.json.done (up to 10s)
    waited=0
    while [ ! -f "$tmp_dir/output.json.done" ] && [ $waited -lt 10 ]; do
        sleep 1
        waited=$((waited + 1))
    done
    if [ ! -f "$tmp_dir/output.json.done" ]; then
        _doctor_fail "kernel smoke: kernel did not respond within 10s"
        return
    fi

    local exec_time=$(($(date +%s) - exec_start))

    # Verify the response mentions "2" somewhere in output_repr
    if grep -q '"output_repr"[[:space:]]*:[[:space:]]*"2"' "$tmp_dir/output.json" 2>/dev/null; then
        _doctor_pass "kernel smoke test (cold start ${cold_start}s, execute ${exec_time}s, 1+1=2)"
    else
        _doctor_fail "kernel smoke: response did not contain output_repr=2 (got: $(head -c 200 "$tmp_dir/output.json"))"
    fi
}
