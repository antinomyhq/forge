---
name: debug-distributed-training
description: >-
  Diagnose and verify PyTorch distributed training implementations,
  including tensor parallelism, pipeline parallelism, model sharding,
  rank-local partitioning, all_reduce/all_gather/send/recv flows, and
  gradient parity across ranks and microbatches. Invoke when tasks use
  torch.distributed, multiprocessing spawn, stage-to-stage communication,
  or when correctness differs between single-rank and multi-rank execution.
---

# Debug Distributed Training

Use this workflow before finalizing distributed ML code. The goal is to catch rank-dependent bugs early and verify gradient correctness, not just syntax.

## 1) Recreate Production-Like Runtime First

Distributed bugs are often masked by missing dependencies in the authoring sandbox.

1. Check import/runtime availability for the exact stack used by the task (`torch`, optional `transformers`, `numpy`).
2. If runtime is missing, set up a minimal local environment before implementation-level verification.
3. Do not treat `py_compile` as sufficient for distributed tasks.

## 2) Enforce Rank Coverage and Parameter Coverage

Always test all required parameter combinations from the prompt.

- If rank count range is specified, run all values.
- If booleans/config flags exist (e.g. bias on/off), run both branches.
- Single-rank runs are degenerate; validate correctness on multi-rank execution.

## 3) Build a Deterministic Reference Comparison Harness

For each distributed step, compare against a non-distributed reference run using identical initialization and microbatches.

- Fix seeds (`torch.manual_seed(...)`) on all ranks.
- Clone model state before distributed step.
- Compute reference gradients with a single-process baseline.
- Compare rank-local gradients to expected slices/tensors.

## 4) Verify Forward, Then Backward, Then Optimizer Effects

Check in this strict order:

1. **Forward parity**: per-microbatch activations/logits match reference tolerance.
2. **Backward parity**: gradients for all stage-owned parameters match reference.
3. **Head/loss scaling parity**: final projection and loss scaling match expected microbatch normalization.

Most silent failures come from backward mismatch even when forward looks correct.

## 5) Communication Invariants by Parallelism Type

### Tensor Parallel

- **Column-parallel linear**: split output features by rank; gather outputs in correct feature order.
- **Row-parallel linear**: split input features by rank; reduce partial outputs exactly once.
- Validate both weight gradients and bias gradients for each rank-local shard.

### Pipeline Parallel

- Partition layers consistently and contiguously across ranks.
- Verify send/recv tensor shapes and dtype/device for stage boundaries.
- Ensure pipeline schedule semantics are implemented exactly as requested.
- On final stage, verify loss reduction/normalization across microbatches.

## 6) Debugging Checklist for Mismatch Failures

If tests report mismatch at a specific rank/module/microbatch:

1. Re-run only that configuration (rank count + microbatch index).
2. Print max absolute diff and relative diff for the failing tensor.
3. Confirm split boundaries and concatenation ordering.
4. Confirm gradient scaling by microbatch count.
5. Confirm no duplicate or missing collective call in backward path.

## 7) Completion Gate

Before declaring completion:

- Distributed test harness passes for all required world sizes.
- Forward and backward parity checks pass at specified tolerance.
- No temporary artifacts remain in final workspace.
- Final verification run is executed after cleanup.
