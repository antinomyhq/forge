---
name: edge-case-tester
description: >-
  Adversarial and boundary-condition testing for tasks with specified
  parameter ranges, distributed/parallel execution, exhaustive output
  requirements, regex/parser/compression, numerical optimization, or
  performance constraints. Invoke AFTER the solution passes basic tests
  but BEFORE declaring complete. Focuses on the specific inputs that
  cause failures: degenerate parameters, boundary values, adversarial
  domain inputs, and oracle comparison.
---

# Edge Case Tester

Run adversarial tests AFTER basic correctness passes. Focus on inputs that actually cause failures.

## 1. Parameter Sweep (MOST COMMON FAILURE SOURCE)

If the task specifies parameter values or ranges:

```python
# Test ALL specified values, not just the trivial case
for ws in [1, 2, 4]:           # world_size=1 is degenerate — test 2+ for real
    for bias in [True, False]:  # Boolean params often have different code paths
        test_case(world_size=ws, bias=bias)
```

Key rules:
- **world_size=1 hides ALL sharding bugs.** If the task says "1,2,4", you MUST test 2 and 4.
- **2x2 matrices differ from 100x100.** Test the actual sizes the verifier uses.
- **Small inputs may be slower** than the reference (overhead). Test small AND large for performance tasks.
- Run the benchmark **3 times** for performance tasks — timing jitter is 30%+.

## 2. Domain-Specific Adversarial Inputs

### Chess / Game Engines
- En passant (including EP that would expose king to check)
- Castling through/out-of/into check
- Pawn promotion with immediate check
- Absolute pins (piece pinned to king)
- Stalemate positions
- Positions with many (>40) and few (1-2) legal moves

### Regex / Parsers
- Empty input, maximum-length input
- Special characters the regex might not escape
- Inputs that match at the very start/end
- Unicode edge cases (multi-byte UTF-8)

### Compression / Encoding
- Empty file (0 bytes), single-byte file
- All identical bytes (e.g., all zeros)
- Random data (incompressible)
- Data at exactly the size limit

### Numerical / Linear Algebra
- Identity matrix, zero matrix, singular matrices
- Very large values (near overflow), very small (near denormalized)
- Negative values, complex results from real inputs
- **Curve fitting**: Check that fitted parameters are physically plausible (center within window, width << window, amplitude > 0)

### Text Processing
- Empty string, single character
- Lines with only whitespace
- Files with no trailing newline

## 3. Oracle Comparison

When a reference implementation or binary exists:

```bash
for i in $(seq 1 20); do
    input=$(generate_random_input $i)
    expected=$(echo "$input" | reference_binary)
    actual=$(echo "$input" | my_solution)
    if [ "$expected" != "$actual" ]; then
        echo "MISMATCH on input $i"
        diff <(echo "$expected") <(echo "$actual")
    fi
done
```

Include BOTH random inputs AND hand-picked adversarial cases from the domain-specific list above.

## 4. Quick Checklist

Before declaring complete:
- [ ] Tested ALL specified parameter values (not just the trivial one)?
- [ ] For performance: faster than reference at EVERY input size?
- [ ] For distributed: works with world_size > 1?
- [ ] For exhaustive output: compared against oracle on randomized inputs?
- [ ] For numerical: are fitted/computed values physically plausible?
- [ ] Tested at least 3 adversarial inputs from the domain-specific list?
