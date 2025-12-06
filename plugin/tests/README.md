# Nothelix Test Suite

Comprehensive behavioral tests for the Nothelix Jupyter notebook plugin.

## Running Tests

From Helix command mode:

```
:run-all-tests          # Run all test suites
:run-cell-tests         # Test cell code parsing (Rust FFI)
:run-kernel-tests       # Test kernel lifecycle & persistence
:run-execution-tests    # Test end-to-end execution flow
```

**Note**: After rebuilding Helix with the new `get-cell-code-from-jl` FFI function, reload the plugin:
```
:reload
```

Then run the tests.

## What Gets Tested

### Cell Extraction Tests (13 assertions)

- ✅ `get-cell-code-from-jl`: Extract code from `.jl` files by `@cell N` marker
- ✅ `notebook-get-cell-code`: Extract code from `.ipynb` JSON files
- ✅ `get-cell-at-line`: Find which cell a line belongs to
- ✅ Code structure preservation
- ✅ Error handling

### Kernel Persistence Tests (10 assertions)

- ✅ Kernel reuse for same notebook
- ✅ Kernel isolation between notebooks
- ✅ Variable persistence across cells
- ✅ Kernel state tracking
- ✅ Cleanup on stop

### Execution Flow Tests (15+ assertions)

- ✅ Sequential execution maintains state
- ✅ Error handling doesn't crash kernel
- ✅ Output capture (stdout, stderr, return values)
- ✅ Multiple concurrent kernels

## Expected Output

When all tests pass:
```
╔════════════════════════════════════════════════════════╗
║  Results: 38 passed, 0 failed                          ║
╚════════════════════════════════════════════════════════╝
✓ All tests passed!
```

## Debugging Failed Tests

1. Check `/tmp/nothelix.log` for Rust FFI logs
2. Check `/tmp/helix-kernel-*/kernel.log` for Julia kernel logs
3. Run individual test suites to isolate failures
4. Use `:kernel-shutdown-all` to clean up stale kernels
