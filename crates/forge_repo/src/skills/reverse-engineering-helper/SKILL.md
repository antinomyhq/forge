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

## Phase 2 -- Forward Oracle Strategy (Preferred)

Instead of perfectly reverse-engineering the algorithm, use the existing code/binary as a **black-box oracle**:

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

This is faster and more reliable than trying to understand every implementation detail.

## Phase 3 -- Language Matching Rule

When byte-level or arithmetic-level compatibility matters:

- **C reference -> implement in C** (not Python). Integer overflow, pointer arithmetic, and struct packing differ.
- **Python reference -> implement in Python** (not C). Float precision, integer division, and string handling differ.
- **Same library versions.** If the reference uses numpy, use numpy. Don't substitute with a different linear algebra library.

Why this matters: Different languages have different:
- Integer overflow behavior (wrapping vs exception vs undefined)
- Float rounding (IEEE 754 modes differ across implementations)
- Division semantics (C truncates toward zero, Python floors toward negative infinity)
- String encoding (UTF-8 byte boundaries, normalization forms)

These mismatches cause silent corruption that is extremely hard to debug.

## Phase 4 -- Incremental Verification

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

## Phase 5 -- Common Pitfalls

- **Endianness**: Check if the binary reads little-endian or big-endian. `xxd` the data and compare to expected values.
- **Padding/alignment**: Structs in C may have padding bytes. Read with the exact struct layout.
- **Magic numbers/headers**: Many formats start with magic bytes. Don't skip them.
- **Off-by-one in sizes**: Does "length" include the null terminator? The header? Read the code carefully.
- **Signed vs unsigned**: A byte value of 0xFF is 255 unsigned but -1 signed. This matters for arithmetic.
