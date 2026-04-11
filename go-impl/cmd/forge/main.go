// Command forge is the CLI entry-point for the forgecode Go port.
package main

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"strings"

	"github.com/spf13/cobra"

	"github.com/pp5ee/forgecode-go/internal/agent"
	"github.com/pp5ee/forgecode-go/internal/api"
)

func main() {
	if err := rootCmd().Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func rootCmd() *cobra.Command {
	var (
		apiKey string
		model  string
		once   string
	)

	cmd := &cobra.Command{
		Use:   "forge",
		Short: "forgecode — AI programming assistant (Go port)",
		Long: `forge is a terminal-based AI programming assistant powered by Claude.

It maintains a multi-turn conversation and lets Claude read, write, and list
files on your behalf via a simple tool protocol.`,
		RunE: func(cmd *cobra.Command, args []string) error {
			client := api.NewClient(apiKey, api.WithModel(model))
			a := agent.New(client)
			ctx := context.Background()

			// Non-interactive: single prompt from --once flag.
			if once != "" {
				reply, err := a.Send(ctx, once)
				if err != nil {
					return fmt.Errorf("forge: %w", err)
				}
				fmt.Println(reply)
				return nil
			}

			// Interactive REPL.
			return runREPL(ctx, a)
		},
	}

	cmd.Flags().StringVar(&apiKey, "api-key", "", "Anthropic API key (default: $ANTHROPIC_API_KEY)")
	cmd.Flags().StringVar(&model, "model", "claude-opus-4-5", "Claude model to use")
	cmd.Flags().StringVar(&once, "once", "", "Send a single prompt and exit (non-interactive)")

	// Sub-commands
	cmd.AddCommand(versionCmd())

	return cmd
}

func runREPL(ctx context.Context, a *agent.Agent) error {
	fmt.Println("forgecode — type your request, or 'exit' / 'quit' to leave, 'reset' to start a new conversation.")
	scanner := bufio.NewScanner(os.Stdin)

	for {
		fmt.Print("> ")
		if !scanner.Scan() {
			break
		}
		input := strings.TrimSpace(scanner.Text())
		if input == "" {
			continue
		}
		switch strings.ToLower(input) {
		case "exit", "quit":
			fmt.Println("Goodbye!")
			return nil
		case "reset":
			a.Reset()
			fmt.Println("Conversation reset.")
			continue
		}

		reply, err := a.Send(ctx, input)
		if err != nil {
			fmt.Fprintf(os.Stderr, "Error: %v\n", err)
			continue
		}
		fmt.Println(reply)
	}

	if err := scanner.Err(); err != nil {
		return fmt.Errorf("forge: scanner: %w", err)
	}
	return nil
}

func versionCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Print the version of forge",
		Run: func(cmd *cobra.Command, args []string) {
			fmt.Println("forge version 0.1.0 (Go port)")
		},
	}
}
