// Package tools provides file-system utilities that the AI agent can invoke.
package tools

import (
	"fmt"
	"os"
	"path/filepath"
)

// ReadFile reads the entire content of the file at path and returns it as a string.
func ReadFile(path string) (string, error) {
	data, err := os.ReadFile(filepath.Clean(path))
	if err != nil {
		return "", fmt.Errorf("tools: read file %q: %w", path, err)
	}
	return string(data), nil
}

// WriteFile writes content to the file at path, creating parent directories as
// needed and truncating any existing file.
func WriteFile(path, content string) error {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("tools: create dirs for %q: %w", path, err)
	}
	if err := os.WriteFile(filepath.Clean(path), []byte(content), 0o644); err != nil {
		return fmt.Errorf("tools: write file %q: %w", path, err)
	}
	return nil
}

// AppendFile appends content to the file at path, creating it if necessary.
func AppendFile(path, content string) error {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("tools: create dirs for %q: %w", path, err)
	}
	f, err := os.OpenFile(filepath.Clean(path), os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return fmt.Errorf("tools: open file %q for append: %w", path, err)
	}
	defer f.Close()
	if _, err := f.WriteString(content); err != nil {
		return fmt.Errorf("tools: append to file %q: %w", path, err)
	}
	return nil
}

// FileExists reports whether a regular file exists at path.
func FileExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}

// ListDir returns the names of the entries in the directory at path.
func ListDir(path string) ([]string, error) {
	entries, err := os.ReadDir(path)
	if err != nil {
		return nil, fmt.Errorf("tools: list dir %q: %w", path, err)
	}
	names := make([]string, 0, len(entries))
	for _, e := range entries {
		names = append(names, e.Name())
	}
	return names, nil
}
