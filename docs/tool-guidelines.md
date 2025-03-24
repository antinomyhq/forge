# Claude Tool Description Best Practices

## Key Principles

When defining tools for Claude to use, the quality of your tool descriptions dramatically affects performance. Here are the most important best practices extracted from Anthropic's documentation:

### 1. Provide Extremely Detailed Descriptions

This is by far the most important factor in tool performance. Your descriptions should explain every detail about the tool, including:

- **What the tool does** - Be explicit about the tool's primary function and purpose
- **When it should be used** - Specify the scenarios where this tool is appropriate
- **When it should NOT be used** - Clarify scenarios where the tool is not suitable
- **What each parameter means** - Explain how each parameter affects the tool's behavior
- **Limitations and caveats** - Note what information the tool does not return or handle
- **Additional context** - If the tool name is unclear, provide extra clarification

### 2. Use Comprehensive But Concise Descriptions

The more context you can give Claude about your tools, the better it will be at deciding when and how to use them. Aim for 3-4 sentences per tool description, or more if the tool is complex.

> **IMPORTANT**: Tool descriptions must never exceed 1024 characters. This is enforced by tests to ensure compatibility with LLM API constraints.

### 3. Prioritize Descriptions Over Examples

While you can include examples of how to use a tool in its description or in the accompanying prompt, this is less important than having a clear and comprehensive explanation of the tool's purpose and parameters. Only add examples after you've fully fleshed out the description.

### 4. Register All Tools in the Registry

Every tool must be registered in the `crates/forge_services/src/tools/registry.rs` file to be available for use. The `ToolRegistry::tools()` method returns all available tools configured with the given infrastructure.

## Example Comparison

### Good Tool Description Example:

```json
{
  "name": "get_stock_price",
  "description": "Retrieves the current stock price for a given ticker symbol. The ticker symbol must be a valid symbol for a publicly traded company on a major US stock exchange like NYSE or NASDAQ. The tool will return the latest trade price in USD. It should be used when the user asks about the current or most recent price of a specific stock. It will not provide any other information about the stock or company.",
  "input_schema": {
    "type": "object",
    "properties": {
      "ticker": {
        "type": "string",
        "description": "The stock ticker symbol, e.g. AAPL for Apple Inc."
      }
    },
    "required": ["ticker"]
  }
}
```

### Poor Tool Description Example:

```json
{
  "name": "get_stock_price",
  "description": "Gets the stock price for a ticker.",
  "input_schema": {
    "type": "object",
    "properties": {
      "ticker": {
        "type": "string"
      }
    },
    "required": ["ticker"]
  }
}
```

## Why the Good Description Works Better

The good description clearly explains:
- What the tool does (retrieves the current stock price)
- What data format it returns (latest trade price in USD)
- When to use it (when the user asks about current/recent price)
- What limitations it has (won't provide other company info)
- What the parameter means (ticker symbol for publicly traded companies)

The poor description is too brief and leaves Claude with many open questions about the tool's behavior and usage. It doesn't explain what kind of data is returned, what format it's in, when to use it, or what parameters are expected.

## Practical Tips

1. **Be specific about data formats** - Explain what format the data will be returned in
2. **Clarify parameter constraints** - Note any restrictions on parameter values
3. **Explain tool selection criteria** - Help Claude understand when to pick this tool over others
4. **Describe error handling** - Note how the tool behaves with invalid inputs
5. **Include domain-specific details** - Add contextual information related to the tool's domain
6. **Keep descriptions under 1024 characters** - Ensure compatibility with LLM API constraints
7. **Register your tool in the registry** - Add your tool to the registry.rs file for availability

Thorough yet concise tool descriptions lead to more accurate tool selection, fewer clarification questions, and better overall performance when using Claude with tools.

Source: [Anthropic Documentation on Tool Use](https://docs.anthropic.com/en/docs/build-with-claude/tool-use/overview#example-poor-tool-description)