---
name: ml-model-debugging
description: >-
  MUST invoke on ANY task involving machine learning, deep learning,
  or neural networks. This includes: loading model weights or
  checkpoints, implementing forward passes, building tokenizers,
  tensor/pipeline parallelism, embedding models, text retrieval,
  similarity search, fine-tuning, inference, model conversion,
  PyTorch, TensorFlow, HuggingFace transformers, GGUF, safetensors,
  MTEB, BERT, GPT, or any model architecture. Invoke at task start
  to get domain-specific guidance BEFORE writing code — not after
  failures. Contains weight format references, activation debugging,
  parallelism patterns, tokenizer implementation details, and
  numerical pitfalls.
---

# ML Model Debugging

Actionable reference for ML tasks. Invoke at task start — read the relevant section, then implement.

## When to Use

Invoke this skill at the START of the task if ANY of these apply:
- Loading model weights (.ckpt, .bin, .pt, .safetensors, GGUF, .npz)
- Implementing a transformer forward pass or any neural network layer
- Building or using a tokenizer (BPE, SentencePiece, etc.)
- Tensor parallelism, pipeline parallelism, or distributed inference
- Embedding models, text similarity, retrieval, or ranking (MTEB, BGE, sentence-transformers)
- Fine-tuning, training, or inference with PyTorch/TensorFlow/JAX
- Model conversion between formats
- Computing cosine similarity, nearest neighbors, or re-ranking

## 0 -- Embedding & Retrieval Tasks (READ FIRST for similarity/retrieval tasks)

These tasks fail silently when you miss model-specific encoding requirements:

### Critical: Query Instruction Prefixes

Many embedding models require different prefixes for queries vs documents:
- **BGE models** (`BAAI/bge-*`): Queries MUST be prefixed with `"Represent this sentence: "` or `"Represent this sentence for searching relevant passages: "`. Documents get NO prefix. Missing this changes rankings completely.
- **E5 models** (`intfloat/e5-*`): Queries use `"query: "` prefix, documents use `"passage: "` prefix.
- **Instructor models**: Use task-specific instructions as prefixes.
- **Sentence-transformers**: Most don't need prefixes, but check the model card.

### Similarity Computation

```python
# ALWAYS normalize embeddings before cosine similarity
embeddings = embeddings / np.linalg.norm(embeddings, axis=1, keepdims=True)
similarity = embeddings @ query_embedding.T

# Common bug: using dot product without normalization gives wrong rankings
# Common bug: comparing raw model outputs without pooling (use [CLS] or mean pooling)
```

### Pooling Strategy

- Check the model card. Most BERT-based models use **mean pooling** over token embeddings (excluding padding).
- Some models use **[CLS] token** output directly.
- Using the wrong pooling strategy silently produces garbage rankings.

## 1 -- Weight File Formats and Inspection

### TensorFlow Checkpoint (.ckpt)

Variables are stored **alphabetically by variable name**, NOT in creation order.

```bash
# Probe structure
xxd /app/model.ckpt | head -5
python3 -c "
import struct
with open('/app/model.ckpt','rb') as f:
    d = f.read(16)
    print('first 16 bytes:', d.hex())
    floats = struct.unpack('<4f', d)
    print('as floats:', floats)
"
```

Typical TF GPT-2 variable alphabetical order:
```
model/h0/attn/c_attn/b, model/h0/attn/c_attn/w, ...
model/h0/ln_1/b, model/h0/ln_1/g, ...
model/ln_f/b, model/ln_f/g,
model/wpe, model/wte  (LAST alphabetically, not first)
```

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

### Probing Weight Layout by Statistics

When the tensor order is unknown, probe statistical signatures:

```python
# Expected signatures:
# Embeddings (wte/wpe): mean ~0.0, std ~0.02, range [-0.15, 0.15]
# Layer norm gamma: mean ~1.0, std ~0.01 (all values near 1.0)
# Layer norm bias: mean ~0.0, std ~0.01
# Attention/FFN weights: mean ~0.0, std ~0.02-0.05
```

If offset 0 shows mean ~1.0, std ~0.01, you're reading layer norm gamma, NOT embeddings.

## 2 -- Debugging Wrong Model Output

When the model compiles, runs, but produces garbage:

1. **Check weight loading order**: Print first few values of layer norm gamma. If not ~1.0, weights are loaded wrong.
2. **Print activations after each layer**: Values exploding (>1e6) = wrong weight order or missing layer norm. Values collapsing = broken attention/softmax. All zeros = wrong offset. NaN = missing 1/sqrt(d) scaling.
3. **Test with minimal input**: Single token, position 0. Check if argmax output is reasonable.
4. **Compare against reference**: Use Python with same weights to compute one forward step. Compare intermediates.

## 3 -- Tensor Parallelism Patterns

### Column Parallel Linear
Split weight columns across ranks. Each rank computes output slice, then all_gather.

### Row Parallel Linear
Split weight rows across ranks. Each rank has input slice, then all_reduce.

### Common bugs
- **Forgetting all-reduce/all-gather at world_size > 1**: Works at ws=1 (identity), breaks at ws=2+
- **Wrong split dimension**: Column = dim=-1, Row = dim=-2
- **Bias handling**: Only one rank should add bias, or divide by world_size before all-reduce
- **Gradient correctness**: Forward pass can work while gradients are wrong. Always test `torch.allclose` on gradients too.

Always test with world_size=1 AND world_size>=2.

## 4 -- Tokenizer Implementation (BPE)

### GPT-2 BPE
1. Byte-to-unicode mapping: 256 bytes → unicode. First 188 printable → themselves; rest → U+0100+.
2. Pre-tokenization regex: `/'s|'t|'re|'ve|'m|'ll|'d| ?\p{L}+| ?\p{N}+| ?[^\s\p{L}\p{N}]+|\s+/`
3. Merge ranks from vocab.bpe: lower line = higher priority.
4. Token IDs: First 256 = byte-unicode, then merges in file order, 50256 = `<|endoftext|>`.

### Common tokenizer bugs
- Using raw byte as index instead of byte-to-unicode rank
- Buffer too small for long merge lines (some exceed 256 bytes)
- Missing contraction handling ('s, 't, 're)
- `cs[n++] = 256 + n` — undefined behavior in C

## 5 -- Numerical Pitfalls

- **Attention scaling**: Divide QK^T by sqrt(d_head), not sqrt(d_model)
- **GELU approximation**: `0.5 * x * (1 + tanh(sqrt(2/pi) * (x + 0.044715 * x^3)))` — exact constant matters
- **Layer norm epsilon**: Usually 1e-5, not 1e-8
- **Float32 softmax overflow**: Subtract max before exp: `exp(x - max(x))`
- **Weight tying**: GPT-2 ties wte with output projection. Don't allocate separately.
- **HuggingFace API**: Use `model.encode()` or `tokenizer()` — never call `model._internal_method`. Check the model card for required input format.
