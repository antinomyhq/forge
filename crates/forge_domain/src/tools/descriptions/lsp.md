The LSP tool allows you to interact with Language Server Protocol servers to understand code.
It supports operations like Go To Definition, Find References, Hover, Document Symbols, etc.

**WHEN TO USE LSP:**
- **Precise Navigation**: When you need to find the *exact* definition of a function, class, or variable (unlike text search which finds all occurrences).
- **Understanding Relationships**: When you need to find all references to a symbol, or see the call hierarchy (who calls this function?).
- **Code Structure**: When you want to see a high-level outline of a file's symbols (classes, methods, variables) using `document_symbol`.
- **Debugging**: When you need to check for syntax errors or type errors in a specific file using `get_diagnostics`.
- **API Exploration**: When you want to see available methods or documentation for a symbol using `hover`.

**WHEN NOT TO USE:**
- **Broad Search**: When you are looking for a concept or pattern across the entire codebase (use `sem_search` or `fs_search`).
- **Text Matching**: When you are looking for a specific string literal or comment (use `fs_search`).
- **File Reading**: When you just need to read the raw content of a file (use `read`).

Usage:
- operation: The LSP operation to perform. Supported operations:
  - `go_to_definition`: Jump to the definition of a symbol.
  - `find_references`: Find all references to a symbol.
  - `hover`: Show documentation/type information for a symbol.
  - `document_symbol`: List all symbols in the current file.
  - `workspace_symbol`: Search for symbols across the workspace.
  - `go_to_implementation`: Jump to the implementation of a symbol.
  - `prepare_call_hierarchy`: Prepare for call hierarchy.
  - `incoming_calls`: Show incoming calls.
  - `outgoing_calls`: Show outgoing calls.
  - `get_diagnostics`: Get diagnostics (errors, warnings) for the file.
- file_path: The absolute path to the file to operate on.
- line: The line number (1-based). Required for position-based operations.
- character: The character offset (1-based). Required for position-based operations.
