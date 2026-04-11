#!/usr/bin/env bats

@test ".helix-fork-rev exists at repo root" {
  [ -f "$BATS_TEST_DIRNAME/../../.helix-fork-rev" ]
}

@test ".helix-fork-rev contains a 40-char git SHA" {
  run cat "$BATS_TEST_DIRNAME/../../.helix-fork-rev"
  [ "$status" -eq 0 ]
  # Exactly one line, 40 hex chars
  [[ "$output" =~ ^[0-9a-f]{40}$ ]]
}

@test ".helix-fork-rev has no trailing whitespace or newlines beyond one" {
  run wc -l "$BATS_TEST_DIRNAME/../../.helix-fork-rev"
  [ "$status" -eq 0 ]
  # wc -l reports 1 for "sha\n" (one newline); reject >1 lines
  [[ "$output" =~ ^[[:space:]]*1[[:space:]] ]]
}
