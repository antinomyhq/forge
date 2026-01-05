Recursively searches directories for files by content (regex) and/or name
(glob pattern). Provides context-rich results with line numbers for content
matches. Two modes: content search (when regex provided) or file finder
(when regex omitted). Uses case-insensitive Rust regex syntax. Requires
absolute paths. Avoids binary files and excluded directories. Best for code
exploration, API usage discovery, configuration settings, or finding
patterns across projects. For large pages, returns the first 200
lines and stores the complete content in a temporary file for
subsequent access.
