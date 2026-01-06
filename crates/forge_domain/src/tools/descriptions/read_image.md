**DEPRECATED**: This tool is deprecated. Use the `read` tool instead, which now automatically handles images, PDFs, and other file types.

Reads image files from the file system and returns them in base64-encoded format for vision-capable models. Supports common image formats: JPEG, PNG, WebP, and GIF. The path must be absolute and point to an existing file. Use this tool when you need to process, analyze, or display images with vision models. Do NOT use this for text files - use the `read` tool instead. Do NOT use for other binary files like PDFs, videos, or archives. The tool will fail if the file doesn't exist or if the format is unsupported. Returns the image content encoded in base64 format ready for vision model consumption.

**Migration**: Simply use the `read` tool with the image path - it will automatically detect and handle images correctly.
