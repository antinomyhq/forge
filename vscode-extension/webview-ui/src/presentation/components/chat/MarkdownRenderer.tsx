import React from 'react';
import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeHighlight from 'rehype-highlight';
import { cn } from '@/lib/utils';

interface MarkdownRendererProps {
  content: string;
  className?: string;
}

/**
 * MarkdownRenderer renders markdown content with syntax highlighting and VSCode theme integration.
 * Supports GFM (GitHub Flavored Markdown) including tables, task lists, and strikethrough.
 */
export const MarkdownRenderer: React.FC<MarkdownRendererProps> = ({ content, className }) => {
  return (
    <div className={cn('markdown-content', className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeHighlight]}
        components={{
          // Headings
          h1: ({ node, ...props }) => (
            <h1
              className="text-2xl font-bold mb-4 mt-6 first:mt-0"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),
          h2: ({ node, ...props }) => (
            <h2
              className="text-xl font-bold mb-3 mt-5 first:mt-0"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),
          h3: ({ node, ...props }) => (
            <h3
              className="text-lg font-semibold mb-2 mt-4 first:mt-0"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),
          h4: ({ node, ...props }) => (
            <h4
              className="text-base font-semibold mb-2 mt-3 first:mt-0"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),
          h5: ({ node, ...props }) => (
            <h5
              className="text-sm font-semibold mb-2 mt-3 first:mt-0"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),
          h6: ({ node, ...props }) => (
            <h6
              className="text-sm font-semibold mb-2 mt-3 first:mt-0"
              style={{ color: 'var(--vscode-descriptionForeground)' }}
              {...props}
            />
          ),

          // Paragraphs
          p: ({ node, ...props }) => (
            <p
              className="mb-3 leading-relaxed"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),

          // Lists
          ul: ({ node, ...props }) => (
            <ul
              className="list-disc list-inside mb-3 space-y-1"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),
          ol: ({ node, ...props }) => (
            <ol
              className="list-decimal list-inside mb-3 space-y-1"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),
          li: ({ node, ...props }) => (
            <li
              className="leading-relaxed"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),

          // Code blocks
          code: ({ node, inline, className, children, ...props }: any) => {
            if (inline) {
              return (
                <code
                  className="px-1.5 py-0.5 rounded text-sm font-mono"
                  style={{
                    backgroundColor: 'var(--vscode-textCodeBlock-background)',
                    color: 'var(--vscode-textPreformat-foreground)',
                    border: '1px solid var(--vscode-panel-border)',
                  }}
                  {...props}
                >
                  {children}
                </code>
              );
            }

            return (
              <code
                className={cn('block font-mono text-sm', className)}
                style={{ color: 'var(--vscode-editor-foreground)' }}
                {...props}
              >
                {children}
              </code>
            );
          },
          pre: ({ node, ...props }) => (
            <pre
              className="p-4 rounded-md overflow-x-auto mb-3 font-mono text-sm"
              style={{
                backgroundColor: 'var(--vscode-textCodeBlock-background)',
                border: '1px solid var(--vscode-panel-border)',
              }}
              {...props}
            />
          ),

          // Blockquotes
          blockquote: ({ node, ...props }) => (
            <blockquote
              className="border-l-4 pl-4 py-2 my-3 italic"
              style={{
                borderColor: 'var(--vscode-textBlockQuote-border)',
                backgroundColor: 'var(--vscode-textBlockQuote-background)',
                color: 'var(--vscode-editor-foreground)',
              }}
              {...props}
            />
          ),

          // Links
          a: ({ node, ...props }) => (
            <a
              className="underline hover:no-underline"
              style={{ color: 'var(--vscode-textLink-foreground)' }}
              target="_blank"
              rel="noopener noreferrer"
              {...props}
            />
          ),

          // Tables
          table: ({ node, ...props }) => (
            <div className="overflow-x-auto mb-3">
              <table
                className="min-w-full border-collapse"
                style={{
                  borderColor: 'var(--vscode-panel-border)',
                }}
                {...props}
              />
            </div>
          ),
          thead: ({ node, ...props }) => (
            <thead
              style={{
                backgroundColor: 'var(--vscode-editor-background)',
                borderBottom: '2px solid var(--vscode-panel-border)',
              }}
              {...props}
            />
          ),
          tbody: ({ node, ...props }) => <tbody {...props} />,
          tr: ({ node, ...props }) => (
            <tr
              style={{
                borderBottom: '1px solid var(--vscode-panel-border)',
              }}
              {...props}
            />
          ),
          th: ({ node, ...props }) => (
            <th
              className="px-4 py-2 text-left font-semibold"
              style={{
                color: 'var(--vscode-editor-foreground)',
              }}
              {...props}
            />
          ),
          td: ({ node, ...props }) => (
            <td
              className="px-4 py-2"
              style={{
                color: 'var(--vscode-editor-foreground)',
              }}
              {...props}
            />
          ),

          // Horizontal rule
          hr: ({ node, ...props }) => (
            <hr
              className="my-4"
              style={{
                borderColor: 'var(--vscode-panel-border)',
              }}
              {...props}
            />
          ),

          // Strong/Bold
          strong: ({ node, ...props }) => (
            <strong
              className="font-bold"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),

          // Emphasis/Italic
          em: ({ node, ...props }) => (
            <em
              className="italic"
              style={{ color: 'var(--vscode-editor-foreground)' }}
              {...props}
            />
          ),

          // Strikethrough (from remark-gfm)
          del: ({ node, ...props }) => (
            <del
              className="line-through"
              style={{ color: 'var(--vscode-descriptionForeground)' }}
              {...props}
            />
          ),
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
};
