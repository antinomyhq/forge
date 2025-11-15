# Forge Custom Providers Configuration

This directory contains configuration files and examples for setting up custom AI providers in Forge.

## Quick Start

1. **Choose an example** that matches your setup:
   - `provider-example-vllm.json` - For VLLM local instances
   - `provider-example-ollama.json` - For Ollama local instances
   - `provider-template.json` - Comprehensive template with all options

2. **Copy and customize**:
   ```bash
   cp provider-example-vllm.json ~/.forge/provider.json
   # Edit ~/.forge/provider.json with your specific configuration
   ```

3. **Configure in Forge**:
   ```bash
   forge provider add
   # Select your custom provider and enter the required credentials
   ```

## Configuration Options

### Required Fields
- `id`: Unique provider identifier (appears in Forge's menu)
- `api_key_vars`: Environment variable name for API key storage
- `url_param_vars`: Array of environment variables used in URLs
- `response_type`: "OpenAI" or "Anthropic" 
- `url`: Chat completions endpoint URL template
- `models`: Either URL template or hardcoded model array

### Model Definition Options

**Option 1: Dynamic Model Fetching**
```json
"models": "{{VLLM_LOCAL_URL}}/v1/models"
```
- Forge automatically fetches available models from the API
- Best for APIs with changing model lists

**Option 2: Hardcoded Models**
```json
"models": [
  {
    "id": "llama2:7b",
    "name": "Llama 2 7B (Local)",
    "description": "Local Llama 2 model",
    "context_length": 4096,
    "tools_supported": true,
    "supports_parallel_tool_calls": false,
    "supports_reasoning": false
  }
]
```
- Manually defined model list
- Best for stable environments with known models

### Model Fields
- `id`: API model identifier (required)
- `name`: Display name in Forge (required)
- `description`: Brief model description (optional)
- `context_length`: Maximum tokens (optional, default: 4096)
- `tools_supported`: Function calling support (optional, default: false)
- `supports_parallel_tool_calls`: Multiple tool calls (optional, default: false)
- `supports_reasoning`: Reasoning/chain-of-thought (optional, default: false)

## URL Templates

Use `{{VARIABLE_NAME}}` syntax for environment variable substitution:

```json
"url": "{{OLLAMA_URL}}/v1/chat/completions"
```

When configured with `OLLAMA_URL=http://127.0.0.1:11434`, this becomes:
`http://127.0.0.1:11434/v1/chat/completions`

## Common Examples

### VLLM Local Instance
```json
{
  "id": "vllm_local",
  "api_key_vars": "VLLM_LOCAL_API_KEY",
  "url_param_vars": ["VLLM_LOCAL_URL"],
  "response_type": "OpenAI",
  "url": "{{VLLM_LOCAL_URL}}/v1/chat/completions",
  "models": "{{VLLM_LOCAL_URL}}/v1/models"
}
```

### Ollama Local Instance
```json
{
  "id": "ollama_local", 
  "api_key_vars": "OLLAMA_API_KEY",
  "url_param_vars": ["OLLAMA_URL"],
  "response_type": "OpenAI",
  "url": "{{OLLAMA_URL}}/v1/chat/completions",
  "models": [
    {
      "id": "llama2:7b",
      "name": "Llama 2 7B (Ollama)",
      "context_length": 4096,
      "tools_supported": true
    }
  ]
}
```

### Custom API Provider
```json
{
  "id": "my_api",
  "api_key_vars": "MY_API_KEY",
  "url_param_vars": ["MY_API_URL"],
  "response_type": "OpenAI", 
  "url": "{{MY_API_URL}}/v1/chat/completions",
  "models": "{{MY_API_URL}}/v1/models"
}
```

## Tips

- Use descriptive provider names: `vllm_local`, `ollama_work`, `custom_openai`
- Include full paths in URLs: `/v1/chat/completions`
- Test endpoints with `curl` before adding to Forge
- Include non-standard ports: `http://127.0.0.1:8888`
- Use dynamic model fetching for cloud APIs, hardcoded for local setups

## Troubleshooting

If your provider shows `[unavailable]`:
1. Check that the API endpoint is accessible
2. Verify environment variables are set correctly
3. Ensure API keys are valid
4. Test the endpoint manually: `curl {{URL}}`

For more detailed examples, see `provider-template.json`.