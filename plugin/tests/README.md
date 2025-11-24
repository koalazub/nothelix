# Nothelix Diagnostic Tests

These tests help diagnose issues with the Nothelix plugin.

## Running Tests

In Helix, run these commands:

```
:run-diagnostic-tests    # Run all diagnostic tests
:run-async-test         # Run async execution test
:run-navigation-test    # Run navigation test
```

**Note**: Commands are defined in `~/.config/helix/helix.scm` and run the test files automatically.

## Test Suite

### 1. diagnostic-tests.scm

Comprehensive diagnostic test that checks:

- **Rust FFI bindings** - Are all the native functions available?
- **Steel builtins** - Do we have spawn-native-thread, hx.with-context, etc.?
- **Protocol detection** - Is graphics protocol detected correctly?
- **Kernel directory** - Does the kernel directory and files exist?
- **Kernel status** - Can we read kernel execution status?
- **Kernel output** - Can we read kernel output?
- **JSON parsing** - Does our JSON parser work?
- **Background threads** - Does spawn-native-thread work?
- **Context callbacks** - Does hx.with-context work?
- **Async execution flow** - Does the full async pipeline work?

**Expected Output:**
```
=== NOTHELIX DIAGNOSTIC TESTS ===

Test 1: Checking Rust FFI bindings...
  ✓ detect-graphics-protocol exists
  ✓ kernel-execute-start exists
  ✓ kernel-execution-status exists
  ... (more tests)

=== TEST SUMMARY ===
Passed: 25/25
✓ ALL TESTS PASSED
```

### 2. async-execution-test.scm

Focused test for async execution:

- Starts kernel execution
- Polls for completion status
- Reads output when done
- Tests background thread + hx.with-context callback

**Expected Behavior:**
- Should show polling status updates
- Should detect when execution completes
- Should show output result
- Should test if background callback fires

**Common Issues:**
- If polling never shows "done": Kernel isn't completing execution
- If background callback doesn't fire: `spawn-native-thread` or `hx.with-context` not available

### 3. navigation-test.scm

Tests cell navigation commands:

- Scans current file for @cell markers
- Tests next-cell function
- Tests previous-cell function

**Prerequisites:**
- Must have a converted .jl file open
- File should have @cell markers

**Expected Output:**
```
=== CELL NAVIGATION TEST ===

File: /path/to/notebook.jl
Total lines: 100

Scanning for cell markers...
  Line 5: @cell 0 nothing
  Line 15: @cell 1 42
  Line 25: @markdown 2

Found 3 cells

Testing next-cell function...
Current line: 5
After next-cell: 15
```

## Interpreting Results

### If diagnostic-tests shows failures:

**"spawn-native-thread exists" fails:**
- Your Helix doesn't have background thread support
- Async execution won't work
- Need synchronous execution fallback

**"hx.with-context exists" fails:**
- Your Helix doesn't have context callback support
- Background threads can't update UI
- Need synchronous execution fallback

**"Protocol is valid" fails:**
- Graphics protocol detection not working
- Check TERM/TERM_PROGRAM environment variables

**Kernel files don't exist:**
- Kernel never started
- Run `:execute-cell` first to start kernel

### If async-execution-test shows issues:

**Polling never reaches "done":**
```bash
# Check kernel manually
cat /tmp/helix-kernel-1/output.txt.done
# If file exists, kernel finished but status check is broken
```

**"Background callback did not fire":**
- `spawn-native-thread` or `hx.with-context` not working
- This explains why "Running..." isn't being replaced
- Need to implement synchronous version

**"hx.with-context error":**
- Function exists but isn't working correctly
- May need different Helix build

### If navigation-test shows issues:

**"No cells found":**
- File not converted yet (run `:convert-notebook`)
- Wrong file format
- Cell markers missing

**"Error calling next-cell":**
- Function implementation has bug
- Check error message for details

## Debugging Steps

1. **Run diagnostic-tests first** to identify which components are missing

2. **If async execution tests fail:**
   - Check if spawn-native-thread exists
   - Check if hx.with-context exists
   - If either missing: need synchronous fallback

3. **If navigation tests fail:**
   - Verify file has @cell markers
   - Check current line number
   - Try :next-cell command manually

4. **Check kernel status manually:**
   ```bash
   ls -la /tmp/helix-kernel-1/
   cat /tmp/helix-kernel-1/output.txt.done
   cat /tmp/helix-kernel-1/output.txt
   ```

## Expected Test Results

### Minimum Requirements (Synchronous Mode)

These must pass for basic functionality:

- ✅ All Rust FFI bindings exist
- ✅ Protocol detection works
- ✅ Kernel files can be read
- ✅ JSON parsing works
- ✅ helix.static functions work

### Full Async Support

These are needed for async execution:

- ✅ spawn-native-thread exists
- ✅ hx.with-context exists
- ✅ Background thread executes
- ✅ Context callback fires

### Navigation Support

These are needed for cell navigation:

- ✅ File has @cell markers
- ✅ next-cell works
- ✅ previous-cell works

## Next Steps Based on Results

**If async tests fail:**
1. Report which functions are missing
2. We'll implement synchronous fallback
3. Or update Helix to version with full Steel support

**If navigation fails:**
1. Check keybindings in ~/.config/helix/config.toml
2. Verify :next-cell works when run as command
3. Report specific error messages

**If all tests pass but still have issues:**
1. The problem is elsewhere
2. May need more specific debugging
3. Check Helix logs for errors
