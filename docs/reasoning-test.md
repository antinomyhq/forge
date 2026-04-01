# Reasoning Test Guide

This guide explains how to manually verify that reasoning parameters are correctly serialized and sent to the provider API.

## Prerequisites

Build the application in debug mode before running any tests:

```bash
cargo build
```

Optionally, inspect available CLI flags:

```bash
target/debug/forge --help
```

## Steps

### 1. Run Forge with debug request capture

Set the following environment variables and run the binary with a simple prompt. The `FORGE_DEBUG_REQUESTS` variable writes the outgoing HTTP request body to the specified path inside the `.forge/` directory.

```bash
FORGE_DEBUG_REQUESTS="forge.request.json" \
FORGE_SESSION__PROVIDER_ID=<provider_id> \
FORGE_SESSION__MODEL_ID=<model_id> \
target/debug/forge -p "Hello!"
```

Replace `<provider_id>` and `<model_id>` with the provider and model you want to test (e.g. `anthropic` / `claude-opus-4-5`, `open_router` / `openai/o3`, etc.).

### 2. Inspect the captured request

After the command completes, a file is written to `.forge/forge.request.json`. Open it and verify that the correct reasoning parameters are present in the request body.

#### OpenAI (Responses API)

OpenAI o-series and GPT-5 models accept a `reasoning` object:

```json
{
  "reasoning": {
    "effort": "medium",
    "summary": "auto"
  }
}
```

- `effort`: `"low"` | `"medium"` | `"high"` — controls how many tokens the model spends on reasoning.
- `summary`: `"auto"` | `"concise"` | `"detailed"` — controls the reasoning summary returned in the response. When `exclude=true` is set in Forge, this maps to `"concise"`.

Note: OpenAI o-series models do not return reasoning tokens in the response body.

#### Anthropic

**Newer models (Opus 4.6, Sonnet 4.6)** use the `output_config.effort` parameter:

```json
{
  "output_config": {
    "effort": "medium"
  }
}
```

**Older models (Opus 4.5 and earlier)** use extended thinking with `budget_tokens`:

```json
{
  "thinking": {
    "type": "enabled",
    "budget_tokens": 8000
  }
}
```

- `effort`: `"max"` | `"high"` (default) | `"medium"` | `"low"` — behavioral signal for thinking depth. `"max"` is only available on Opus 4.6; using it on other models returns an error.
- `budget_tokens`: integer — maximum number of thinking tokens; must be > 1024 and strictly less than the overall `max_tokens` to leave room for the final response.

#### OpenRouter

OpenRouter normalizes reasoning across providers using a unified `reasoning` object:

```json
{
  "reasoning": {
    "effort": "high",
    "max_tokens": 2000,
    "exclude": false
  }
}
```

- `effort`: `"xhigh"` | `"high"` | `"medium"` | `"low"` | `"minimal"` | `"none"` — for OpenAI o-series and Grok models. Approximate token allocation: `xhigh` ≈ 95%, `high` ≈ 80%, `medium` ≈ 50%, `low` ≈ 20%, `minimal` ≈ 10% of `max_tokens`.
- `max_tokens`: integer (≥ 1024, ≤ 128 000) — for Anthropic and Gemini models; passed directly as `budget_tokens`. For Gemini 3 models it maps to `thinkingLevel` internally.
- `exclude`: boolean — when `true`, reasoning runs internally but is omitted from the response (`reasoning` field is absent).
- `enabled`: boolean — shorthand to activate reasoning at `"medium"` effort with no exclusions.

Reasoning tokens appear in `choices[0].message.reasoning` (plain text) and in the structured `choices[0].message.reasoning_details` array.

---

The `ReasoningConfig` fields in Forge that drive all of the above are:

- `enabled` — activates reasoning at medium effort (supported by OpenRouter, Anthropic, and Forge)
- `effort` — explicit effort level: `low`, `medium`, or `high` (supported by OpenRouter and Forge)
- `max_tokens` — token budget for thinking; must be > 1024 (supported by OpenRouter, Anthropic, and Forge)
- `exclude` — hides the reasoning trace from the response (supported by OpenRouter and Forge)

## References

- [OpenAI Reasoning](https://developers.openai.com/api/docs/guides/reasoning)
- [Anthropic Extended Thinking](https://platform.claude.com/docs/en/build-with-claude/effort)
- [OpenRouter Reasoning Tokens](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens)
