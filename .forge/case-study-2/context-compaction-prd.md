# Product Requirements Document
# Context Compaction Feature

## 1. Introduction

**Document Purpose:** Define requirements for the Context Compaction feature that optimizes large AI conversations.
**Product Vision:** Create an intelligent system that compresses AI conversation history to reduce token usage while maintaining coherence.

## 2. Problem Statement

Long-running AI conversations lead to:
- Excessive token consumption
- Slower response times
- Hitting context window limits
- Poor reasoning due to context overload
- Higher operational costs

## 3. Product Overview

### 3.1 Description
An intelligent conversation management system that automatically identifies and compresses assistant messages and tool results while preserving critical conversation context.

### 3.2 Key Benefits
- Reduced token usage (30-50%)
- Faster model responses
- Extended conversation lifetimes
- Improved reasoning quality
- Lower operational costs

## 4. Feature Specifications

### 4.1 Core Capabilities

**Selective Compression:** Only compress assistant messages and tool results, preserving user inputs.
**Tool Call Coherence:** Compression should not break the assistant's on-going process of calling tools. 
**Incremental Approach:** Compress the earliest sequence of messages first, working forward over time.
**Intelligent Summarization:** Use AI to create concise summaries of previous exchanges.
**Recency Preservation:** Maintain most recent messages untouched via configurable window.
**Flexible Triggers:** Configure when compaction occurs based on tokens, messages, or turns.
**Retention Window:** Retain a min N most recent messages.

### 4.2 Configuration Parameters

| Parameter | Description | Default | Required |
|-----------|-------------|---------|----------|
| max_tokens | Maximum tokens for summary generation | 2000 | Yes |
| token_threshold | Token count that triggers compaction | 60000 | No |
| message_threshold | Message count that triggers compaction | None | No |
| turn_threshold | Turn count that triggers compaction | None | No |
| retention_window | Number of recent messages to preserve | 6 | Yes |
| model | Model ID to use for summarization | Same as agent | Yes |
| prompt | Custom prompt template for summarization | System default | No |
| summary_tag | Tag to extract content from in response | "summary" | No |

### 4.3 YAML Configuration Example
```yaml
compact:
  max_tokens: 2000
  token_threshold: 60000
  retention_window: 6
  model: google/gemini-2.0-flash-001
  prompt: "{{> system-prompt-context-summarizer.hbs }}"
  summary_tag: "summary"
```

## 5. User Experience

### 5.1 Compaction Process
1. System detects when thresholds are exceeded
2. Identifies earliest compressible message sequence
3. Generates concise summary of the sequence
4. Replaces sequence with summary
5. Process repeats as needed during conversation

### 5.2 Example Scenario

**Before Compaction:**
```
User: "Can you debug this code?"
Assistant: "Let me take a look." + (Tool call to read file)
Tool Result: (File contents)
Assistant: "I found the issue..." + (Detailed explanation)
Assistant: "Let me fix it." + (Tool call to patch file)
Tool Result: (Success message)
Assistant: "The fix is applied. Here's what I changed..."
User: "Can you explain why that fixed it?"
```

**After Compaction:**
```
User: "Can you debug this code?"
Assistant: "I debugged the code by identifying an issue with error handling, then fixed it by adding proper try/catch blocks and applied the patch successfully."
User: "Can you explain why that fixed it?"
```
