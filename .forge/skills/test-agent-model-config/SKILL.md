---
name: test-agent-model-config
description: Test and verify that per-agent model configuration (agent_forge, agent_muse, agent_sage) works correctly via environment variables. Use when asked to test or verify that agent-specific model/provider overrides are applied correctly and that debug request files confirm the right model is being sent to the provider.
---

# Test Agent Model Config

Verify that `agent_forge`, `agent_muse`, and `agent_sage` env var overrides correctly route
requests to the right provider/model, and that preset reasoning effort values are forwarded
correctly in the POST body.

## Key env vars

| Purpose                               | Env var                          | Example                      |
| ------------------------------------- | -------------------------------- | ---------------------------- |
| Override forge agent model            | `FORGE_AGENT_FORGE__MODEL_ID`    | `claude-3-haiku-20240307`    |
| Override forge agent provider         | `FORGE_AGENT_FORGE__PROVIDER_ID` | `anthropic`                  |
| Override forge agent preset           | `FORGE_AGENT_FORGE__PRESET_ID`   | `my_preset`                  |
| Override muse agent model             | `FORGE_AGENT_MUSE__MODEL_ID`     | `claude-3-haiku-20240307`    |
| Override muse agent preset            | `FORGE_AGENT_MUSE__PRESET_ID`    | `my_preset`                  |
| Override sage agent model             | `FORGE_AGENT_SAGE__MODEL_ID`     | `minimax/minimax-m2`         |
| Override sage agent provider          | `FORGE_AGENT_SAGE__PROVIDER_ID`  | `open_router`                |
| Override sage agent preset            | `FORGE_AGENT_SAGE__PRESET_ID`    | `my_preset`                  |
| Write each LLM request body to a file | `FORGE_DEBUG_REQUESTS`           | `/tmp/forge-debug-req.json`  |

`FORGE_DEBUG_REQUESTS` is a **file path** (not a directory). Each POST overwrites the same file
with the latest request body JSON.

## Workflow

### 1. Build the binary

```bash
cargo build -p forge_main
```

### 2. Run the test script

```bash
bash .forge/skills/test-agent-model-config/test-agent-model-config.sh
```

The script runs all checks sequentially and prints a pass/fail summary. No manual sleep or
background processes needed — forge is invoked with `-p` (single-prompt mode) and exits on its
own.

### 3. What the script tests

**Sanity checks** (no LLM call, instant):
- `FORGE_SESSION__MODEL_ID=test-sentinel` → `config get model` prints `test-sentinel`
- Agent overrides do not corrupt session model parsing

**Model routing** (one LLM call per agent):
- `FORGE_AGENT_FORGE__MODEL_ID` → `model` field in Anthropic POST body
- `FORGE_AGENT_MUSE__MODEL_ID`  → `model` field in Anthropic POST body
- `FORGE_AGENT_SAGE__MODEL_ID`  → `model` field in Anthropic POST body

**Preset reasoning effort — Anthropic** (one call per agent):
- Preset defines `reasoning = { enabled = true, effort = "low" }`
- Expected POST body field: `output_config.effort = "low"`
- `FORGE_AGENT_FORGE__PRESET_ID`, `FORGE_AGENT_MUSE__PRESET_ID`, `FORGE_AGENT_SAGE__PRESET_ID`

**Preset reasoning effort — sage via OpenRouter `minimax/minimax-m2`** (three calls):
- `FORGE_AGENT_SAGE__PROVIDER_ID=open_router` + `FORGE_AGENT_SAGE__MODEL_ID=minimax/minimax-m2`
- Three presets tested: `effort = low | medium | high`
- Expected POST body field: `reasoning.effort` (OpenRouter passes it through verbatim)

### 4. What to check in the debug file

```python
import json
d = json.load(open("/tmp/forge-req-test.json"))

# Model routing
print(d["model"])                               # e.g. "claude-3-haiku-20240307"

# Anthropic reasoning effort (output_config path)
print(d.get("output_config", {}).get("effort")) # e.g. "low"

# OpenRouter reasoning effort (reasoning object path)
print(d.get("reasoning", {}).get("effort"))     # e.g. "high"
```

### 5. Provider-specific POST body shapes

| Provider   | Config                     | POST body field            |
| ---------- | -------------------------- | -------------------------- |
| `anthropic`  | `effort = low/medium/high/max` | `output_config.effort`  |
| `anthropic`  | `enabled + max_tokens`     | `thinking.budget_tokens`   |
| `open_router`| `effort = low/medium/high` | `reasoning.effort`         |

### 6. Preset TOML — presets cannot be set via env vars

`presets` is a `HashMap<String, Preset>` in `ForgeConfig`. The `config` crate env source
cannot synthesise new map keys, so presets must live in `~/forge/.forge.toml`:

```toml
[presets.minimax_high]
reasoning = { enabled = true, effort = "high" }
```

Reference the preset from an agent via env var:

```bash
FORGE_AGENT_SAGE__PRESET_ID=minimax_high
```

The script appends its test presets to `~/forge/.forge.toml` at runtime and removes them on
exit via `trap`.

## Notes

- `FORGE_DEBUG_REQUESTS` writes the raw POST body — it is **overwritten** on every request, so
  only the last request is captured per invocation.
- Provider identity is embedded in the **URL**, not the request body. Use `FORGE_LOG=debug` to
  see which provider URL was hit.
- The OpenRouter provider ID string is `open_router` (with underscore).
- Valid effort values for OpenRouter: `none | minimal | low | medium | high | xhigh`
- Valid effort values for Anthropic: `low | medium | high | max`
