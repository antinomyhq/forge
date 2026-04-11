// Package api provides an HTTP client for the Claude (Anthropic) Messages API.
package api

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
)

const (
	defaultBaseURL = "https://api.anthropic.com/v1"
	defaultModel   = "claude-opus-4-5"
	anthropicVersion = "2023-06-01"
)

// Role represents a conversation participant role.
type Role string

const (
	RoleUser      Role = "user"
	RoleAssistant Role = "assistant"
)

// Message represents a single turn in a conversation.
type Message struct {
	Role    Role   `json:"role"`
	Content string `json:"content"`
}

// Request is the body sent to the Claude Messages API.
type Request struct {
	Model     string    `json:"model"`
	MaxTokens int       `json:"max_tokens"`
	Messages  []Message `json:"messages"`
	System    string    `json:"system,omitempty"`
}

// ContentBlock holds a single text block from the API response.
type ContentBlock struct {
	Type string `json:"type"`
	Text string `json:"text"`
}

// Response is the response received from the Claude Messages API.
type Response struct {
	ID      string         `json:"id"`
	Type    string         `json:"type"`
	Role    Role           `json:"role"`
	Content []ContentBlock `json:"content"`
	Model   string         `json:"model"`
	Usage   struct {
		InputTokens  int `json:"input_tokens"`
		OutputTokens int `json:"output_tokens"`
	} `json:"usage"`
}

// Text returns the concatenated text from all content blocks.
func (r *Response) Text() string {
	var out string
	for _, block := range r.Content {
		if block.Type == "text" {
			out += block.Text
		}
	}
	return out
}

// Client is the Claude API HTTP client.
type Client struct {
	apiKey     string
	baseURL    string
	model      string
	httpClient *http.Client
}

// Option is a functional option for Client.
type Option func(*Client)

// WithBaseURL overrides the default Anthropic API base URL.
func WithBaseURL(url string) Option {
	return func(c *Client) { c.baseURL = url }
}

// WithModel sets the Claude model to use for all requests.
func WithModel(model string) Option {
	return func(c *Client) { c.model = model }
}

// WithHTTPClient replaces the default http.Client.
func WithHTTPClient(hc *http.Client) Option {
	return func(c *Client) { c.httpClient = hc }
}

// NewClient creates a new Claude API client.
// If apiKey is empty the ANTHROPIC_API_KEY environment variable is used.
func NewClient(apiKey string, opts ...Option) *Client {
	if apiKey == "" {
		apiKey = os.Getenv("ANTHROPIC_API_KEY")
	}
	c := &Client{
		apiKey:     apiKey,
		baseURL:    defaultBaseURL,
		model:      defaultModel,
		httpClient: &http.Client{},
	}
	for _, o := range opts {
		o(c)
	}
	return c
}

// Chat sends a conversation to the Messages API and returns the reply.
func (c *Client) Chat(ctx context.Context, messages []Message, systemPrompt string) (*Response, error) {
	req := Request{
		Model:     c.model,
		MaxTokens: 4096,
		Messages:  messages,
		System:    systemPrompt,
	}

	body, err := json.Marshal(req)
	if err != nil {
		return nil, fmt.Errorf("api: marshal request: %w", err)
	}

	httpReq, err := http.NewRequestWithContext(ctx, http.MethodPost, c.baseURL+"/messages", bytes.NewReader(body))
	if err != nil {
		return nil, fmt.Errorf("api: build request: %w", err)
	}
	httpReq.Header.Set("Content-Type", "application/json")
	httpReq.Header.Set("x-api-key", c.apiKey)
	httpReq.Header.Set("anthropic-version", anthropicVersion)

	httpResp, err := c.httpClient.Do(httpReq)
	if err != nil {
		return nil, fmt.Errorf("api: send request: %w", err)
	}
	defer httpResp.Body.Close()

	respBody, err := io.ReadAll(httpResp.Body)
	if err != nil {
		return nil, fmt.Errorf("api: read response body: %w", err)
	}

	if httpResp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("api: unexpected status %d: %s", httpResp.StatusCode, string(respBody))
	}

	var resp Response
	if err := json.Unmarshal(respBody, &resp); err != nil {
		return nil, fmt.Errorf("api: unmarshal response: %w", err)
	}

	return &resp, nil
}
