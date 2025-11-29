import { cva, type VariantProps } from "class-variance-authority"

/// Custom message variant for tool call messages
export const messageVariants = cva(
  "rounded-lg border p-4 transition-colors",
  {
    variants: {
      role: {
        user: "bg-primary/10 border-primary/20",
        assistant: "bg-muted/50 border-border",
        system: "bg-accent/10 border-accent/20",
        tool: "bg-secondary/10 border-secondary/20",
      },
      status: {
        pending: "opacity-60",
        streaming: "opacity-80 animate-pulse",
        complete: "opacity-100",
        error: "border-destructive bg-destructive/10",
      },
    },
    defaultVariants: {
      role: "assistant",
      status: "complete",
    },
  }
)

export type MessageVariantProps = VariantProps<typeof messageVariants>

/// Tool call card variants
export const toolCallVariants = cva(
  "rounded-md border p-3 font-mono text-sm",
  {
    variants: {
      status: {
        running: "border-blue-500/50 bg-blue-500/10 animate-pulse",
        completed: "border-border bg-muted/50",
        failed: "border-destructive bg-destructive/10",
        pending: "border-muted-foreground/20 bg-muted/50",
      },
    },
    defaultVariants: {
      status: "pending",
    },
  }
)

export type ToolCallVariantProps = VariantProps<typeof toolCallVariants>

/// File reference chip variants
export const fileReferenceVariants = cva(
  "inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-xs font-semibold transition-colors",
  {
    variants: {
      variant: {
        default: "bg-primary/10 text-primary hover:bg-primary/20",
        secondary: "bg-secondary/10 text-secondary-foreground hover:bg-secondary/20",
        outline: "border border-input bg-background hover:bg-accent hover:text-accent-foreground",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

export type FileReferenceVariantProps = VariantProps<typeof fileReferenceVariants>

/// Code block variants for syntax highlighting integration
export const codeBlockVariants = cva(
  "rounded-md border p-4 font-mono text-sm overflow-x-auto",
  {
    variants: {
      theme: {
        vscode: "bg-[var(--vscode-textCodeBlock-background)] border-[var(--vscode-panel-border)]",
        default: "bg-muted/50 border-border",
      },
    },
    defaultVariants: {
      theme: "vscode",
    },
  }
)

export type CodeBlockVariantProps = VariantProps<typeof codeBlockVariants>
