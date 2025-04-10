[![CI Status](https://img.shields.io/github/actions/workflow/status/antinomyhq/forge/ci.yml?style=for-the-badge)](https://github.com/antinomyhq/forge/actions)
[![GitHub Release](https://img.shields.io/github/v/release/antinomyhq/forge?style=for-the-badge)](https://github.com/antinomyhq/forge/releases)
[![Discord](https://img.shields.io/discord/1044859667798568962?style=for-the-badge&cacheSeconds=120&logo=discord)](https://discord.gg/kRZBPpkgwq)
[![CLA assistant](https://cla-assistant.io/readme/badge/antinomyhq/forge?style=for-the-badge)](https://cla-assistant.io/antinomyhq/forge)

# ⚒️ Forge: AI-Enhanced Terminal Development Environment

![Code-Forge Demo](https://assets.antinomy.ai/images/forge_demo_2x.gif)

Forge is a comprehensive coding agent that integrates AI capabilities with your development environment, offering sophisticated assistance while maintaining the efficiency of your existing workflow. ✨

## 🚀 Installation

Install Forge globally using npm:

```bash
npm install -g @antinomyhq/forge
```

Or run directly without installation using npx:

```bash
npx @antinomyhq/forge
```

This method works on **Windows**, **macOS**, and **Linux**, providing a consistent installation experience across all platforms.

## 🔌 Provider Configuration

Forge requires two configuration files in your project directory:

1. A `.env` file with your API credentials
2. A `forge.yaml` file specifying additional settings

Below are setup instructions for each supported provider:

### OpenRouter (Recommended)

```bash
# .env
OPENROUTER_API_KEY=<your_openrouter_api_key>
```

_No changes in `forge.yaml` is required_

### OpenAI

```bash
# .env
OPENAI_API_KEY=<your_openai_api_key>
```

```yaml
# forge.yaml
model: o3-mini-high
```

### Anthropic

```bash
# .env
ANTHROPIC_API_KEY=<your_anthropic_api_key>
```

```yaml
# forge.yaml
model: claude-3.7-sonnet
```

### Google Vertex AI

```bash
# .env
PROJECT_ID=<your_project_id>
LOCATION=<your_location>
OPENAI_API_KEY=<vertex_ai_key>
OPENAI_URL=https://${LOCATION}-aiplatform.googleapis.com/v1beta1/projects/${PROJECT_ID}/locations/${LOCATION}/endpoints/openapi
```

```yaml
# forge.yaml
model: publishers/anthropic/models/claude-3-7-sonnet
```

### OpenAI-Compatible Providers

```bash
# .env
OPENAI_API_KEY=<your_provider_api_key>
OPENAI_URL=<your_provider_url>
```

```yaml
# forge.yaml
model: <provider-specific-model>
```

### Amazon Bedrock

To use Amazon Bedrock models with Forge, you'll need to first set up the [Bedrock Access Gateway](https://github.com/aws-samples/bedrock-access-gateway):

1. **Set up Bedrock Access Gateway**:

   - Follow the deployment steps in the [Bedrock Access Gateway repo](https://github.com/aws-samples/bedrock-access-gateway)
   - Create your own API key in Secrets Manager
   - Deploy the CloudFormation stack
   - Note your API Base URL from the CloudFormation outputs

2. **Create these files in your project directory**:

   ```bash
   # .env
   OPENAI_API_KEY=<your_bedrock_gateway_api_key>
   OPENAI_URL=<your_bedrock_gateway_base_url>
   ```

   ```yaml
   # forge.yaml
   model: anthropic.claude-3-opus
   ```

## 📚 Documentation

For comprehensive documentation on all features and capabilities, please visit the [documentation site](https://github.com/antinomyhq/forge/tree/main/docs).

## 🤝 Community

Join our vibrant Discord community to connect with other Code-Forge users and contributors, get help with your projects, share ideas, and provide feedback! 🌟

[![Discord](https://img.shields.io/discord/1044859667798568962?style=for-the-badge&cacheSeconds=120&logo=discord)](https://discord.gg/kRZBPpkgwq)

## ⭐ Support Us

Your support drives Code-Forge's continued evolution! By starring our GitHub repository, you:

- Help others discover this powerful tool 🔍
- Motivate our development team 💪
- Enable us to prioritize new features 🛠️
- Strengthen our open-source community 🌱
