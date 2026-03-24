---
name: reverse-engineering-helper
description: >-
  Systematic reverse-engineering for tasks that require understanding
  existing code, binary formats, model weights, or data protocols to
  reimplement or interoperate. Use when the task provides compiled
  binaries, encoded data, model checkpoints, or reference
  implementations that must be matched exactly.
---

# Reverse Engineering Helper

Systematic approach for understanding and reimplementing existing code, formats, and protocols.

## When to Use

- Task provides a compiled binary you must interoperate with (decompressor, encoder, model)
- Task requires producing data that matches exact byte-level semantics of existing code
- Task provides model weights in a specific format you must load
- Task requires matching the output of a reference implementation exactly

## Phase 1 -- Discovery

Identify what you're working with:

```bash
# For binaries
file /app/binary_name
ldd /app/binary_name 2>/dev/null || echo "statically linked"
strings /app/binary_name | head -50
nm /app/binary_name 2>/dev/null | head -30

# For data files
file /app/data_file
xxd /app/data_file | head -20
wc -c /app/data_file

# For source code
wc -l /app/*.c /app/*.py 2>/dev/null
head -50 /app/source_file

# For model weights
python3 -c "import json; d=json.load(open('/app/weights.json')); print(type(d), len(d) if isinstance(d,dict) else 'N/A')"
```

Read ALL provided source code carefully. Annotate:
- Input format (what does it read from stdin/files/args?)
- Output format (what does it produce?)
- Algorithm (what transformation does it apply?)
- Edge cases (overflow, rounding, special values)

## Phase 2 -- Language Choice Gate (Run Before Prototyping)

Before writing the producer, choose the implementation language.

- If the provided consumer/reference is written in **C** and correctness depends on exact arithmetic or byte semantics, your producer must default to **C as well**.
- For a C interop task, start with C.
- Python is allowed for auxiliary analysis, search heuristics, and test harnesses, but not as the default producer for a C interop task.
- If you believe same-language implementation is impossible or too costly, prove that with a minimal same-language spike first; only then fall back.

This gate takes priority over artifact-first and oracle-first guidance.

## Phase 3 -- Forward Oracle Strategy (Preferred After Language Choice)

After choosing the implementation language, prefer using the existing code/binary as a **black-box oracle** to validate your implementation's output. For C interop tasks, the oracle should usually validate your C producer or C harness first, not a Python-first encoder. For deliverable-producing tasks, do not spend more than **2 consecutive analysis turns** without either generating a candidate artifact at the final path or running a forward-oracle experiment. **This does NOT mean brute-forcing the input space** — if the output is more than a few bytes, the search space is astronomically large and enumeration will never finish. Build a proper encoder/generator and test its output against the oracle.

1. **Build a test harness:**
   ```bash
   # Feed known input, capture expected output
   echo "test input" | /app/reference_binary > /tmp/expected_output
   ```

2. **Generate diverse test cases:**
   ```bash
   # Simple cases
   echo "" | /app/reference_binary > /tmp/expected_empty
   echo "a" | /app/reference_binary > /tmp/expected_single
   # Complex cases
   cat /app/real_data | /app/reference_binary > /tmp/expected_real
   ```

3. **Implement your solution and compare:**
   ```bash
   echo "test input" | /app/my_solution > /tmp/actual_output
   diff /tmp/expected_output /tmp/actual_output
   ```

4. **Iterate until outputs match on all test cases.**

This is faster and more reliable than trying to understand every implementation detail. If your current line of reasoning is not producing better candidates after 2 experiments, stop analyzing and switch to a new empirical strategy.

**Anti-pattern — subprocess-per-candidate brute-force**: Never write a loop that spawns a subprocess per candidate to search the input space. If the search space exceeds ~1000 items, the approach is wrong — build a structured encoder/generator and debug it when its output is wrong, rather than bypassing it with enumeration.

## Phase 3.5 -- Artifact-First Rule

As soon as you have any candidate that might work, write it to the final expected output path and test that exact artifact with the provided consumer. Do not keep promising candidates only in `/tmp` or in-memory while continuing analysis.

## Phase 4 -- Why Same-Language Matters

When byte-level or arithmetic-level compatibility matters:

- **C reference -> implement in C** (not Python). Integer overflow, pointer arithmetic, struct packing, promotion rules, and integer division behavior differ.
- **Python reference -> implement in Python** (not C). Float precision, integer division, and string handling differ.
- **Same library versions.** If the reference uses numpy, use numpy. Don't substitute with a different linear algebra library.

Why this matters: Different languages have different:
- Integer overflow behavior (wrapping vs exception vs undefined)
- Float rounding (IEEE 754 modes differ across implementations)
- Division semantics (C truncates toward zero, Python floors toward negative infinity)
- String encoding (UTF-8 byte boundaries, normalization forms)

These mismatches cause silent corruption that is extremely hard to debug.

## Phase 5 -- Incremental Verification

For multi-stage pipelines:

1. **Break the pipeline into stages** (parse -> transform -> encode -> output).
2. **Verify each stage independently:**
   ```bash
   # Print intermediate values
   echo "Stage 1 output:" && my_solution --stage1 input | xxd | head
   echo "Reference stage 1:" && reference --debug-stage1 input | xxd | head
   ```
3. **Diff at each stage.** Find the first point of divergence.
4. **Only proceed to the next stage when the current one matches exactly.**

## Phase 6 -- Common Pitfalls

- **Endianness**: Check if the binary reads little-endian or big-endian. `xxd` the data and compare to expected values.
- **Padding/alignment**: Structs in C may have padding bytes. Read with the exact struct layout.
- **Magic numbers/headers**: Many formats start with magic bytes. Don't skip them.
- **Off-by-one in sizes**: Does "length" include the null terminator? The header? Read the code carefully.
- **Signed vs unsigned**: A byte value of 0xFF is 255 unsigned but -1 signed. This matters for arithmetic.
- **Integer overflow emulation**: When reimplementing C code in Python/JS, C's 32-bit signed arithmetic wraps silently. Python integers are arbitrary-precision and NEVER overflow — this causes silent divergence on large inputs. Always emulate C overflow: use `ctypes.c_int32(value).value` or `((value + 2**31) % 2**32) - 2**31` for every multiplication and addition in the hot loop. **Test your emulation**: run both the C binary and your reimplementation on the same 1000+ byte input and compare byte-for-byte BEFORE scaling up.
- **EOF/padding semantics**: C code reading from stdin returns EOF (-1) after the last byte. Python `sys.stdin.buffer.read()` returns empty bytes. If the C code's behavior depends on reading past EOF (e.g., returning 0xFF or 0x00 for padding), replicate that exact behavior.
