The LSP tool allows you to interact with Language Server Protocol servers to understand code.
It supports operations like Go To Definition, Find References, Hover, Document Symbols, etc.

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
