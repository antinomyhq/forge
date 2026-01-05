Reads file contents from the specified absolute path. Ideal for analyzing
code, configuration files, documentation, or textual data. Returns the
content as a string with line number prefixes by default. For files larger
than 2,000 lines, the tool automatically returns only the first 2,000 lines.
You should always rely on this default behavior and avoid specifying custom
ranges unless absolutely necessary. If needed, specify a range with the
start_line and end_line parameters, ensuring the total range does not exceed
2,000 lines. Specifying a range exceeding this limit will result in an
error. Binary files are automatically detected and rejected.
