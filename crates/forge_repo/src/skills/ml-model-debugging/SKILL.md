---
name: ml-model-debugging
description: >-
  Reference knowledge for debugging ML model loading, inference, and
  numerical issues. Use when a task involves loading model weights from
  checkpoint files, implementing transformer forward passes, building
  tokenizers, or diagnosing wrong model outputs. Covers weight file
  formats, activation debugging, tensor parallelism patterns, and
  tokenizer implementation.
---

# ML Model Debugging

Actionable reference for diagnosing and fixing ML model loading and inference issues.

## When to Use

- Task requires loading model weights from a checkpoint file (.ckpt, .bin, .pt, .safetensors, GGUF)
- Model produces garbage/repeated output after loading
- Task involves implementing a transformer forward pass or tokenizer
- Task requires tensor parallelism or distributed inference
- Output doesn't match a reference model

## 1 -- Weight File Formats and Inspection

### TensorFlow Checkpoint (.ckpt)

Variables are stored **alphabetically by variable name**, NOT in creation order.

```bash
# Probe structure -- check if it's raw floats or has TF headers
xxd /app/model.ckpt | head -5
python3 -c "
import struct
with open('/app/model.ckpt','rb') as f:
    d = f.read(16)
    print('first 16 bytes:', d.hex())
    # TF checkpoints start with specific headers; raw dumps start with float values
    floats = struct.unpack('<4f', d)
    print('as floats:', floats)
"
```

Typical TF GPT-2 variable alphabetical order:
```
model/h0/attn/c_attn/b, model/h0/attn/c_attn/w,
model/h0/attn/c_proj/b, model/h0/attn/c_proj/w,
model/h0/ln_1/b, model/h0/ln_1/g,
model/h0/ln_2/b, model/h0/ln_2/g,
model/h0/mlp/c_fc/b, model/h0/mlp/c_fc/w,
model/h0/mlp/c_proj/b, model/h0/mlp/c_proj/w,
... (repeat for h1..h11) ...
model/ln_f/b, model/ln_f/g,
model/wpe, model/wte
```

Note: wte/wpe come LAST alphabetically, not first.

### PyTorch State Dict (.pt, .bin)

Keys match module hierarchy in definition order.

```bash
python3 -c "
import torch
d = torch.load('/app/model.pt', map_location='cpu')
for k in list(d.keys())[:20]:
    print(k, d[k].shape, d[k].dtype)
"
```

### Safetensors (.safetensors)

JSON header at file start with tensor names, offsets, dtypes.

```bash
python3 -c "
import struct, json
with open('/app/model.safetensors','rb') as f:
    hlen = struct.unpack('<Q', f.read(8))[0]
    header = json.loads(f.read(hlen))
    for k in sorted(header.keys())[:15]:
        print(k, header[k])
"
```

### Raw Float Dumps (llm.c style)

No header, just contiguous float32. Order depends on the exporter.

```bash
python3 -c "
import struct, os
sz = os.path.getsize('/app/model.bin')
print(f'{sz} bytes = {sz//4} floats = {sz//4/1e6:.1f}M params')
"
```

### Probing Weight Layout by Statistics

When the tensor order is unknown, probe statistical signatures at different offsets:

```bash
python3 -c "
import struct, math
def stats(path, offset, count=768):
    with open(path, 'rb') as f:
        f.seek(offset * 4)
        vals = struct.unpack(f'<{count}f', f.read(count * 4))
    mu = sum(vals) / len(vals)
    var = sum((v - mu)**2 for v in vals) / len(vals)
    print(f'  offset {offset:>10d}: mean={mu:.4f} std={math.sqrt(var):.4f} min={min(vals):.4f} max={max(vals):.4f}')

path = '/app/model.ckpt'
print('Probing weight regions:')
for off in [0, 50257*768, 50257*768+1024*768, 50257*768+1024*768+768]:
    stats(path, off)
"
```

**Expected signatures:**
- **Embeddings (wte/wpe):** mean ~0.0, std ~0.02, range [-0.15, 0.15]
- **Layer norm gamma:** mean ~1.0, std ~0.01, all values near 1.0
- **Layer norm bias:** mean ~0.0, std ~0.01, all values near 0.0
- **Attention/FFN weights:** mean ~0.0, std ~0.02-0.05
- **Attention/FFN biases:** mean ~0.0, std ~0.01

If offset 0 shows mean ~1.0, std ~0.01, you're looking at layer norm gamma, NOT embeddings.

## 2 -- Debugging Wrong Model Output

When the model compiles, runs, but produces garbage (repeated tokens, nonsensical text):

### Step 1: Check weight loading order

```bash
# Print first few values at each weight pointer
# If layer norm gamma isn't ~1.0, weights are loaded in wrong order
printf("ln1_g[0..4]: %f %f %f %f\n", g1[0][0], g1[0][1], g1[0][2], g1[0][3]);
```

### Step 2: Print activations after each layer

Add temporary debug output:
```c
// After layer 0
fprintf(stderr, "layer0 x[0..4]: %f %f %f %f\n", x[0], x[1], x[2], x[3]);
```

**What to look for:**
- Values exploding (>1e6): weight order wrong or missing layer norm
- Values collapsing to identical: attention mask or softmax broken
- All zeros: wrong offset, reading past end of file
- NaN/Inf: numerical overflow in attention scores (missing scaling by 1/sqrt(d))

### Step 3: Test with minimal input

```bash
# Single token, position 0 -- simplest possible forward pass
./model 0  # Feed token ID 0 at position 0
# Check: is the argmax output a reasonable token?
```

### Step 4: Compare against reference (if available)

```bash
# Use Python with the same weights to compute one forward step
python3 -c "
import struct, math
# Read same weights, compute same forward pass on same input
# Compare intermediate values at each stage
"
```

## 3 -- Tensor Parallelism Patterns

### Column Parallel Linear

Split weight matrix columns across ranks. Each rank computes a slice of the output.

```
W = [W0 | W1 | ... | Wn-1]  (split along columns)
rank_i: y_i = x @ W_i       (each rank gets full input x)
y = all_gather(y_i)          (gather slices to form full output)
```

### Row Parallel Linear

Split weight matrix rows across ranks. Each rank has a slice of the input.

```
W = [W0; W1; ...; Wn-1]     (split along rows)
rank_i: y_i = x_i @ W_i     (each rank has input slice x_i)
y = all_reduce(y_i)          (sum partial results)
```

### Common bugs

- **Forgetting all-reduce/all-gather at world_size > 1:** Works fine at world_size=1 (identity op), breaks at 2+
- **Wrong split dimension:** Column parallel splits dim=-1, row parallel splits dim=-2
- **Bias handling:** Only one rank should add the bias (usually rank 0), or divide bias by world_size before all-reduce
- **Embedding parallel:** Split vocabulary across ranks; each rank handles a vocab slice, then all-reduce

### Testing tensor parallelism

Always test with world_size=1 AND world_size=2 minimum:
```python
for ws in [1, 2, 4]:
    output = parallel_linear(input, weight, world_size=ws)
    assert torch.allclose(output, reference_output, atol=1e-5)
```

## 4 -- Tokenizer Implementation (BPE)

### GPT-2 BPE

1. **Byte-to-unicode mapping:** 256 byte values mapped to unicode characters.
   First 188 printable bytes map to themselves; remaining 68 map to U+0100+.
2. **Pre-tokenization regex:** Split input into words before BPE:
   ```
   /'s|'t|'re|'ve|'m|'ll|'d| ?\p{L}+| ?\p{N}+| ?[^\s\p{L}\p{N}]+|\s+/
   ```
3. **Merge ranks:** vocab.bpe lists merges in priority order. Lower line number = higher priority.
4. **BPE encoding:** For each pre-tokenized word, iteratively merge the highest-priority adjacent pair.
5. **Token IDs:** First 256 = byte-unicode chars, then 256+ = merges in file order, 50256 = `<|endoftext|>`.

### Common tokenizer bugs

- Using raw byte as array index instead of byte-to-unicode rank
- Buffer too small for long BPE merge lines (some exceed 256 bytes)
- Undefined behavior in `cs[n++] = 256 + n` (C sequencing)
- Missing contraction handling ('s, 't, 're, etc.)

## 5 -- Numerical Pitfalls

- **Attention scaling:** Divide QK^T by sqrt(d_head), not sqrt(d_model)
- **GELU approximation:** `0.5 * x * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))` -- get the constant right
- **Layer norm epsilon:** Usually 1e-5, not 1e-8. Wrong epsilon causes divergence
- **Float32 softmax overflow:** Subtract max before exp: `exp(x - max(x))`
- **Weight tying:** GPT-2 ties wte (input embeddings) with the output projection. Don't allocate separately.
