---
name: edge-case-tester
description: >-
  Adversarial and boundary-condition test generation after basic
  correctness is achieved. Use after the solution passes basic tests
  but before declaring complete, especially for algorithmic tasks,
  distributed systems, regex/parser tasks, or optimization problems
  with specified input ranges.
---

# Edge Case Tester

Generate and run adversarial tests after the solution passes basic correctness checks.

## When to Use

Invoke this skill after:
- Basic tests pass (the "happy path" works)
- When chained from verification-specialist's Step 4 (Edge-Case Gate)
- Especially for: algorithms, distributed systems, parsers, regex engines, optimization, numerical code

## 1. Input Range Sweep

If the task specifies input sizes or ranges (e.g., "matrices up to 10x10", "strings up to 1000 chars"):

- **Minimum**: Test the smallest valid input (1x1, empty string, single element)
- **Minimum+1**: Test one step above minimum (2x2, single char) -- off-by-one bugs hide here
- **Small values**: For optimization tasks, test small inputs where naive algorithms may be faster than your optimized solution. Overhead matters when N is tiny.
- **Medium**: Geometric mean of the range
- **Maximum**: The largest specified size
- **Beyond maximum**: If feasible, test slightly above to ensure graceful handling

For performance-critical tasks, run the benchmark at EVERY specified size and compare against the reference. A solution that's faster for large inputs but slower for small ones will fail on the small-input tests.

## 2. Parameter Combination Testing

If the task has multiple parameters (world_size, num_workers, batch_size, bias=True/False):

- **Never test only the trivial case.** world_size=1 is degenerate and hides ALL sharding/distribution bugs.
- **Test the maximum specified value.** world_size=4 exposes alignment and remainder issues.
- **Test at least one intermediate value.** world_size=2 catches basic split bugs.
- **Test all boolean combinations.** bias=True and bias=False often have different code paths.
- **Cross-product when feasible.** If 3 params with 3 values each = 27 combos, test at least the corners.

Write a loop:
```python
for ws in [1, 2, 4]:
    for bias in [True, False]:
        test_case(world_size=ws, bias=bias)
```

## 3. Domain-Specific Adversarial Inputs

### Chess / Game Engines
- Positions with absolute pins (piece pinned to king)
- Discovered checks and double checks
- En passant (including en passant that would expose king to check)
- Castling through/out-of/into check
- Pawn promotion (especially to queen with immediate check)
- Stalemate positions
- Positions with many legal moves (>40) and few legal moves (1-2)

### Regex / Parsers
- Empty input
- Maximum-length input
- Inputs with special characters the regex might not escape
- Catastrophic backtracking patterns
- Unicode edge cases (if applicable)
- Inputs that match at the very start/end

### Compression / Encoding
- Empty file (0 bytes)
- Single-byte file
- File of all identical bytes (e.g., all zeros)
- Random data (incompressible)
- Data at exactly the size limit
- Data one byte over the size limit

### Numerical / Linear Algebra
- Identity matrix, zero matrix
- Singular/degenerate matrices
- Very large values (near overflow)
- Very small values (near underflow/denormalized)
- Negative values, complex results from real inputs
- Symmetric vs asymmetric matrices

### Text Processing
- Empty string, single character
- Lines with only whitespace
- Lines at maximum length
- Unicode characters (multi-byte UTF-8)
- Files with no trailing newline

## 4. Multi-Input Generalization

When the task says "must work on any input" or provides a single example:

1. **Never test only the provided example.** The verifier will test on different, freshly generated inputs.
2. **Create at least 2 additional test inputs yourself** that differ structurally from the example (different sizes, different properties, compiled with different flags).
3. **For binary/file format tasks**: compile/generate a second test file with `gcc -o test2 test2.c` and run your solution on it. The verifier does exactly this.
4. **For game/position tasks**: test on mid-game positions (move 15+), not just the opening. Complex positions expose bugs that simple ones hide.
5. If you only pass on the provided example, you are likely overfitting to it.

## 5. Oracle Comparison

When a reference implementation or binary exists:

1. Generate 20+ random inputs spanning the full input space.
2. Run both your solution and the reference on each input.
3. Diff the outputs.
4. Any mismatch = bug. Investigate before declaring complete.

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

## 6. Output Checklist

After running edge case tests, answer:
- [ ] Does the solution handle the minimum input size?
- [ ] Does the solution handle the maximum input size?
- [ ] Does the solution handle ALL specified parameter values?
- [ ] For optimization: is the solution faster than the reference at EVERY input size?
- [ ] For distributed: does it work with world_size > 1?
- [ ] Have I tested at least 3 adversarial inputs from the domain-specific list?
