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

Replace `<provider_id>` and `<model_id>` with the provider and model you want to test. To see available providers:

```bash
target/debug/forge provider list
```

### 2. Inspect the captured request

After the command completes, a file is written to `.forge/forge.request.json`. Open it and verify that the correct reasoning parameters are present in the request body.

#### OpenAI — Chat Completions API

The Chat Completions API (`POST /v1/chat/completions`) uses a top-level `reasoning_effort` string field:

```json
{
  "model": "gpt-5.1",
  "reasoning_effort": "medium",
  "messages": [...]
}
```

- `reasoning_effort`: `"none"` | `"minimal"` | `"low"` | `"medium"` | `"high"` | `"xhigh"` — constrains how many tokens the model spends on reasoning. Reducing it produces faster responses at lower cost.

Model-specific defaults and constraints:

- `gpt-5.1` defaults to `"none"` (no reasoning); supports `"none"`, `"low"`, `"medium"`, `"high"`.
- Models before `gpt-5.1` default to `"medium"` and do not support `"none"`.
- `gpt-5-pro` defaults to and only supports `"high"`.
- `"xhigh"` is supported for all models after `gpt-5.1-codex-max`.

Note: OpenAI does not return reasoning tokens in the response body.

#### OpenAI — Responses API

The Responses API uses a nested `reasoning` object instead:

```json
{
  "reasoning": {
    "effort": "medium",
    "summary": "auto"
  }
}
```

- `effort`: `"low"` | `"medium"` | `"high"` — controls reasoning depth.
- `summary`: `"auto"` | `"concise"` | `"detailed"` — controls the reasoning summary in the response. When `exclude=true` is set in Forge, this maps to `"concise"`.

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

## Test Matrix

Use these combinations to exercise reasoning across providers and models. For each row, set the listed `ReasoningConfig` fields in your agent definition (or via the forge config), then verify the JSON field appears in `.forge/forge.request.json`.

| Provider      | Model                        | `ReasoningConfig` fields             | JSON field in request                      | Valid effort / token values                              |
| ------------- | ---------------------------- | ------------------------------------ | ------------------------------------------ | -------------------------------------------------------- |
| `open_router` | `openai/o4-mini`             | `effort: high`                       | `reasoning.effort`                         | `none` · `minimal` · `low` · `medium` · `high` · `xhigh` |
| `open_router` | `anthropic/claude-opus-4-5`  | `max_tokens: 4000`                   | `reasoning.max_tokens`                     | integer ≥ 1024                                           |
| `anthropic`   | `claude-opus-4-6`            | `effort: medium`                     | `output_config.effort`                     | `low` · `medium` · `high` · `max`¹                       |
| `anthropic`   | `claude-3-7-sonnet-20250219` | `enabled: true` + `max_tokens: 8000` | `thinking.type` + `thinking.budget_tokens` | integer ≥ 1024                                           |

**Notes:**

1. `max` effort is only available on `claude-opus-4-6`; using it on any other model returns an API error.
2. The `openai` provider strips the `reasoning` field before sending. Use `open_router` with an OpenAI model (e.g. `openai/o4-mini`) to test OpenAI reasoning.

## References

- [OpenAI Reasoning guide](https://developers.openai.com/api/docs/guides/reasoning)
- [OpenAI Chat Completions API reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)
- [Anthropic Extended Thinking](https://platform.claude.com/docs/en/build-with-claude/effort)
- [OpenRouter Reasoning Tokens](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens)
