AI-powered semantic code search. YOUR DEFAULT TOOL for code discovery tasks when searching within the current working directory ({{env.cwd}}). Use this when you need to find code locations, understand implementations, or explore functionality - it works with natural language about behavior and concepts, not just keyword matching.

IMPORTANT: Only searches within the current working directory ({{env.cwd}}) and its subdirectories. To search outside this scope, use {{tool_names.fs_search}} instead.
Examples:
- Can search: {{env.cwd}}/src, {{env.cwd}}/lib, any subdirectory of {{env.cwd}}
- Cannot search: /other/path, /tmp, or paths outside {{env.cwd}} (use {{tool_names.fs_search}} with path parameter)

Start with sem_search when: locating code to modify, understanding how features work, finding patterns/examples, or exploring unfamiliar areas. Understands queries like "authentication flow" (finds login), "retry logic" (finds backoff), "validation" (finds checking/sanitization).

Returns the topK most relevant file:line locations with code context. Use multiple varied queries (2-3) for best coverage. For exact string matching (TODO comments, specific function names), use {{tool_names.fs_search}} instead.