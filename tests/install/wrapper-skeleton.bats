#!/usr/bin/env bats

setup() {
  export WRAPPER="$BATS_TEST_DIRNAME/../../dist/nothelix"
  export NOTHELIX_TEST_MODE=1   # prevents exec to hx-nothelix
  cp "$BATS_TEST_DIRNAME/fixtures/VERSION.example" "$BATS_TEST_DIRNAME/fixtures/VERSION" 2>/dev/null || true
}

@test "wrapper exists and is executable" {
  [ -x "$WRAPPER" ]
}

@test "--help prints usage and exits 0" {
  run "$WRAPPER" --help
  [ "$status" -eq 0 ]
  [[ "$output" == *"nothelix"* ]]
  [[ "$output" == *"upgrade"* ]]
  [[ "$output" == *"uninstall"* ]]
  [[ "$output" == *"doctor"* ]]
}

@test "-h is an alias for --help" {
  run "$WRAPPER" -h
  [ "$status" -eq 0 ]
  [[ "$output" == *"nothelix"* ]]
}

@test "no args would exec hx-nothelix with demo path (test mode prints the cmd)" {
  run "$WRAPPER"
  [ "$status" -eq 0 ]
  [[ "$output" == *"hx-nothelix"* ]]
  [[ "$output" == *"demo.jl"* ]]
}

@test "a file arg is forwarded verbatim" {
  run "$WRAPPER" /tmp/test.jl
  [ "$status" -eq 0 ]
  [[ "$output" == *"hx-nothelix /tmp/test.jl"* ]]
}

@test "multiple file args are forwarded" {
  run "$WRAPPER" /tmp/a.jl /tmp/b.jl
  [ "$status" -eq 0 ]
  [[ "$output" == *"hx-nothelix /tmp/a.jl /tmp/b.jl"* ]]
}

@test "unknown flags pass through" {
  run "$WRAPPER" +42 /tmp/notes.md
  [ "$status" -eq 0 ]
  [[ "$output" == *"hx-nothelix +42 /tmp/notes.md"* ]]
}

@test "version reads from the VERSION file" {
    FIXTURE="$BATS_TEST_DIRNAME/fixtures"
    NOTHELIX_SHARE="$FIXTURE" run "$WRAPPER" version
    [ "$status" -eq 0 ]
    [[ "$output" == *"nothelix v0.2.1"* ]]
    [[ "$output" == *"89734c7291a9"* ]]
    [[ "$output" == *"feature/inline-image-rendering"* ]]
}

@test "version fails gracefully if VERSION file is missing" {
    NOTHELIX_SHARE="/tmp/does-not-exist" run "$WRAPPER" version
    [ "$status" -eq 1 ]
    [[ "$output" == *"VERSION file not found"* ]]
    [[ "$output" == *"nothelix upgrade"* ]]
}
