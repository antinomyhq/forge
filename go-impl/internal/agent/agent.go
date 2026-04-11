// Package agent implements the core conversation loop for the forgecode AI agent.
package agent

import (
	"context"
	"fmt"
	"strings"

	"github.com/pp5ee/forgecode-go/internal/api"
	"github.com/pp5ee/forgecode-go/internal/tools"
)

const systemPrompt = `You are forgecode, an AI programming assistant.
You help users write, read, and modify code through a simple tool interface.

You have access to the following tools. To invoke a tool, output a line in exactly this format:
  TOOL:<tool_name>:<argument>

Available tools:
  TOOL:read_file:<path>       — read the contents of a file
  TOOL:write_file:<path>|<content> — write content to a file (separator is first '|')
  TOOL:list_dir:<path>        — list entries in a directory

After invoking a tool you will receive the result in the next user message prefixed with TOOL_RESULT.
When you are done, reply normally without any TOOL: prefix.`

// Agent manages the stateful conversation with Claude.
type Agent struct {
	client   *api.Client
	history  []api.Message
}

// New creates a new Agent using the provided Claude API client.
func New(client *api.Client) *Agent {
	return &Agent{client: client}
}

// Send sends a user message, handles any tool calls in the assistant reply,
// and returns the final text response.
func (a *Agent) Send(ctx context.Context, userInput string) (string, error) {
	a.history = append(a.history, api.Message{Role: api.RoleUser, Content: userInput})

	for {
		resp, err := a.client.Chat(ctx, a.history, systemPrompt)
		if err != nil {
			return "", fmt.Errorf("agent: chat: %w", err)
		}

		assistantText := resp.Text()
		a.history = append(a.history, api.Message{Role: api.RoleAssistant, Content: assistantText})

		// Check whether the model wants to invoke a tool.
		toolResult, invoked, err := handleTool(assistantText)
		if err != nil {
			// Tool execution error — feed the error back so the model can recover.
			a.history = append(a.history, api.Message{
				Role:    api.RoleUser,
				Content: fmt.Sprintf("TOOL_RESULT:error:%s", err.Error()),
			})
			continue
		}
		if !invoked {
			// No tool call — the model's reply is the final answer.
			return assistantText, nil
		}

		// Feed the tool result back to the model.
		a.history = append(a.history, api.Message{
			Role:    api.RoleUser,
			Content: "TOOL_RESULT:" + toolResult,
		})
	}
}

// Reset clears the conversation history so a new conversation can begin.
func (a *Agent) Reset() {
	a.history = nil
}

// History returns a copy of the current conversation history.
func (a *Agent) History() []api.Message {
	h := make([]api.Message, len(a.history))
	copy(h, a.history)
	return h
}

// handleTool parses the first TOOL: line in text, executes the tool, and
// returns (result, true, nil) on success, ("", false, nil) when no tool call
// is present, or ("", true, err) when the tool was invoked but failed.
func handleTool(text string) (string, bool, error) {
	for _, line := range strings.Split(text, "\n") {
		line = strings.TrimSpace(line)
		if !strings.HasPrefix(line, "TOOL:") {
			continue
		}
		// TOOL:<name>:<arg>
		parts := strings.SplitN(line, ":", 3)
		if len(parts) < 3 {
			return "", true, fmt.Errorf("malformed tool call: %q", line)
		}
		toolName, arg := parts[1], parts[2]

		switch toolName {
		case "read_file":
			content, err := tools.ReadFile(arg)
			if err != nil {
				return "", true, err
			}
			return fmt.Sprintf("read_file:%s", content), true, nil

		case "write_file":
			idx := strings.Index(arg, "|")
			if idx < 0 {
				return "", true, fmt.Errorf("write_file: missing '|' separator in %q", arg)
			}
			path, content := arg[:idx], arg[idx+1:]
			if err := tools.WriteFile(path, content); err != nil {
				return "", true, err
			}
			return fmt.Sprintf("write_file:ok:%s", path), true, nil

		case "list_dir":
			entries, err := tools.ListDir(arg)
			if err != nil {
				return "", true, err
			}
			return fmt.Sprintf("list_dir:%s", strings.Join(entries, "\n")), true, nil

		default:
			return "", true, fmt.Errorf("unknown tool: %q", toolName)
		}
	}
	return "", false, nil
}
