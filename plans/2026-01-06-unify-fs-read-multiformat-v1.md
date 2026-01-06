# Unify fs_read Tool to Support Multiple File Formats

## Objective

Transform the `read` tool into a unified file reading interface that automatically detects and handles different file types using MIME type detection. Visual content (PDFs, images) is sent as base64-encoded data for LLM visual analysis, while text content (including Jupyter notebooks) is sent as formatted text. This eliminates the need for separate `read_image` tool and avoids complex text extraction - letting the LLM leverage its vision capabilities for PDFs and native JSON parsing for notebooks. The enhanced tool should seamlessly detect MIME types and return appropriately formatted responses while maintaining full backward compatibility with existing tool calls.

**Rationale**: Following OpenCode's proven approach - PDFs are treated as visual content (base64 data URLs) rather than extracting text, and Jupyter notebooks are read as plain JSON text rather than parsing cell structure. This is simpler to implement and more powerful since vision-capable LLMs can analyze PDF pages directly and understand ipynb JSON structure natively.

## Implementation Plan

- [ ] 1. Design unified file type detection system using MIME types in the service layer
  - Add MIME type detection logic in `crates/forge_services/src/tool_services/fs_read.rs` using a library like `mime_guess` or `infer` crate to classify files based on extensions and content magic numbers
  - Classify files into two categories: text-based (text/plain, text/*, application/json) and binary/visual (application/pdf, image/*, etc.)
  - For visual content (PDFs and images), send as base64-encoded data to LLM for visual analysis rather than extracting text - following OpenCode's approach where PDFs are converted to data URLs and sent as attachments
  - For text files including Jupyter notebooks (ipynb), read as plain text and let LLM parse the JSON structure
  - Reuse existing `ImageFormat` enum from `crates/forge_services/src/tool_services/image_read.rs:14-22` for supported image format validation
  - Reference: OpenCode's ReadTool uses `file.type` (MIME) to detect `application/pdf` and `image/*` types and sends them as base64 data URLs rather than extracting content

- [ ] 2. Extend FSRead struct to support optional file type hint parameter
  - Modify `FSRead` struct in `crates/forge_domain/src/tools/catalog.rs:73-92` to add optional `file_type` parameter that allows users to explicitly specify file type when auto-detection might fail
  - Keep this parameter optional with `#[serde(skip_serializing_if = "Option::is_none")]` to maintain backward compatibility
  - Add comprehensive documentation explaining when manual file type specification is needed
  - Ensure parameter doesn't break existing tool calls that don't specify it

- [ ] 3. Refactor ForgeFsRead service to dispatch to appropriate reader based on MIME type
  - Modify `read` method in `crates/forge_services/src/tool_services/fs_read.rs:57-120` to detect MIME type and branch on text vs binary/visual content
  - For text files (text/*, application/json): keep existing implementation with line numbering and range support
  - For visual content (application/pdf, image/*): read file as binary and convert to base64-encoded data URL for LLM visual analysis
  - For Jupyter notebooks (*.ipynb): treat as text/plain and read JSON content as text - LLM can parse the structure
  - Maintain single entry point with simple branching logic based on MIME type classification (text vs visual)
  - Reference: OpenCode's ReadTool branches on `file.type` - text files get line numbering, PDFs/images get base64 encoding

- [ ] 4. Implement image reading path within unified read service
  - Extract and integrate image reading logic from `ForgeImageRead` service in `crates/forge_services/src/tool_services/image_read.rs:46-90` into the unified read flow
  - Ensure base64 encoding follows same pattern using `Image::new_bytes()` from `crates/forge_domain/src/image.rs:12-16`
  - Validate image size limits using `env.max_image_size` as in `crates/forge_services/src/tool_services/image_read.rs:57-61`
  - Return `Content` type that wraps the `Image` object for proper multimodal response
  - Preserve all error messages and validation behavior from current `read_image` tool

- [ ] 5. Implement PDF handling as visual content (base64 encoding)
  - Treat PDFs as visual/binary content similar to images - read file bytes and convert to base64 data URL
  - Use format `data:application/pdf;base64,{base64_content}` following OpenCode's pattern
  - Validate PDF size against `env.max_image_size` limit (or create dedicated `env.max_pdf_size` if different limit needed)
  - Return as `Image` content type with MIME type `application/pdf` to enable LLM visual analysis of PDF pages
  - No need for PDF parsing libraries - LLM providers with vision capabilities can analyze PDF content directly from base64
  - This approach handles both text-based and scanned PDFs uniformly, letting the LLM extract information visually

- [ ] 6. Handle Jupyter notebooks as plain text JSON files
  - Treat Jupyter notebooks (*.ipynb) as text files - read raw JSON content with line numbers
  - No special parsing or formatting needed - LLM can understand ipynb JSON structure natively
  - Apply standard text file size limits from `env.max_file_size`
  - Let LLM parse cell types, execution counts, outputs, and embedded images from the JSON
  - Following OpenCode's approach: notebooks are not explicitly handled as special format, just read as text/plain
  - Validate JSON is readable but don't validate notebook schema - let LLM handle malformed notebooks with its own error messages

- [ ] 7. Update ReadOutput structure to support text and visual content types
  - Modify `ReadOutput` struct in `crates/forge_app/src/services.rs:39-45` to include MIME type information for content classification
  - Add `mime_type: Option<String>` field to indicate content type (e.g., "application/pdf", "image/png", "text/plain")
  - Ensure `Content` field can represent both text (with line info) and binary/visual content (base64 Image)
  - For visual content (PDFs, images): set start_line=0, end_line=0, total_lines=0 since line info is not applicable
  - For text content: preserve existing behavior with line ranges and hash calculation
  - Content hash should be computed on raw bytes for visual content, on text for text files

- [ ] 8. Update tool executor to handle unified read tool responses for text and visual content
  - Modify `ToolExecutor::call_internal` in `crates/forge_app/src/tool_executor.rs:126-137` to handle `ReadOutput` with both text and visual content
  - Ensure response formatting in `ToolOperation::into_tool_output` at `crates/forge_app/src/operation.rs:225-249` checks MIME type to determine output format
  - For visual content (PDFs with application/pdf, images with image/*): return `ToolOutput::image()` with base64 Image for multimodal LLM consumption
  - For text content (text/*, application/json, including ipynb): return `ToolOutput::text()` with line numbering and XML formatting as currently implemented
  - Preserve existing line numbering behavior for text files when `show_line_numbers` is true

- [ ] 9. Deprecate read_image tool while maintaining backward compatibility
  - Keep `ReadImage` struct in `crates/forge_domain/src/tools/catalog.rs:94-100` but mark as deprecated in code comments
  - Update tool executor to route `ToolCatalog::ReadImage` calls to unified read service with image file type
  - Update tool description in `crates/forge_domain/src/tools/descriptions/read_image.md` to indicate deprecation and suggest using `read` tool instead
  - Ensure all existing `read_image` tool calls continue to work identically to preserve backward compatibility
  - Plan for eventual removal in future major version but keep functional for now

- [ ] 10. Update fs_read tool description to document automatic multiformat support
  - Modify `crates/forge_domain/src/tools/descriptions/fs_read.md` to document that the tool now handles all file types automatically based on MIME type
  - Explain that visual files (PDFs, images) are sent as base64-encoded content for LLM visual analysis, while text files (including ipynb) are sent as formatted text
  - Document automatic MIME type detection based on file extensions and content
  - List supported image formats (JPEG, PNG, WebP, GIF) and note that PDFs are treated as visual content
  - Explain that Jupyter notebooks are read as plain JSON text and LLM can parse their structure
  - Document size limits: text files use max_file_size, visual content uses max_image_size
  - Remove reference to using separate `read_image` tool since this is now unified
  - Add template variable references for configuration limits

- [ ] 11. Add comprehensive unit tests for text and visual file type handling
  - Create tests for text file reading in `crates/forge_services/src/tool_services/fs_read.rs` to ensure existing functionality is preserved
  - Add tests for image reading covering all supported formats (JPEG, PNG, WebP, GIF) including base64 encoding validation
  - Add tests for PDF reading verifying base64 encoding with correct MIME type (application/pdf)
  - Add tests for Jupyter notebook reading as plain text JSON files with line numbering
  - Test MIME type detection with various file extensions and content types
  - Test size limit enforcement distinguishing text files (max_file_size) from visual content (max_image_size)
  - Test error handling for unsupported binary formats, malformed files, and missing files

- [ ] 12. Add integration tests for tool executor with text and visual content reads
  - Create integration tests in `crates/forge_app/src/tool_executor.rs` that exercise the complete flow from tool call to formatted output
  - Test reading text files (including ipynb) through executor verifying XML formatting with line numbers
  - Test reading images through executor verifying base64 Image output with correct MIME types
  - Test reading PDFs through executor verifying base64 Image output with application/pdf MIME type
  - Test backward compatibility by ensuring existing test fixtures with `read` and `read_image` tool calls still pass
  - Test the deprecated `read_image` tool routing to ensure identical behavior to legacy implementation
  - Verify metrics tracking and hash calculation work correctly for both text and visual content

- [ ] 13. Update operation and response formatting to handle text vs visual content
  - Modify `ToolOperation::FsRead` handling in `crates/forge_app/src/operation.rs:225-249` to branch based on MIME type
  - For text files (including ipynb JSON): keep existing XML element formatting with file_content and line number attributes
  - For visual content (images and PDFs): return Image content directly without XML wrapping to match multimodal API expectations
  - No special formatting for PDFs or notebooks - they're handled as visual (base64) or text (raw JSON) respectively
  - Ensure metrics logging in `crates/forge_app/src/operation.rs:242-246` captures MIME type and distinguishes text vs visual reads

- [ ] 14. Consider environment configuration for PDF size limits
  - Evaluate whether PDFs need separate size limit from images or can reuse `max_image_size` (both are visual content)
  - If separate limit needed, add `max_pdf_size` to environment settings in `crates/forge_app/src/services.rs`
  - Jupyter notebooks use existing `max_file_size` since they're treated as text files
  - Update environment variable parsing to support `FORGE_MAX_PDF_SIZE` if separate limit is deemed necessary
  - Consider that OpenCode uses same general limit for all binary content without distinguishing PDFs from images
  - Document size limit strategy in configuration documentation

- [ ] 15. Update summary and context handling for multiformat content
  - Verify `extract_tool_info` in `crates/forge_domain/src/compact/summary.rs:309-339` correctly handles unified read tool with different content types
  - Ensure both `Read` and `ReadImage` variants map to `SummaryTool::FileRead` for backward compatibility as shown in `crates/forge_domain/src/compact/summary.rs:313-314`
  - Update context serialization to handle new content types, potentially filtering large PDFs or notebooks like base64 images in `crates/forge_domain/src/context.rs:38-56`
  - Test that conversation summaries correctly represent file reads regardless of file type

- [ ] 16. Verify DTO layer handles PDFs and images as visual content correctly
  - Ensure Anthropic DTO in `crates/forge_app/src/dto/anthropic/request.rs:244-268` correctly serializes Image content with application/pdf MIME type for PDFs
  - Ensure OpenAI DTO in `crates/forge_app/src/dto/openai/request.rs:92-104` correctly serializes PDF data URLs alongside image data URLs
  - Test that both providers receive properly formatted multimodal messages when reading images and PDFs through unified tool
  - Verify text content from Jupyter notebooks is sent as plain text messages with JSON structure
  - Ensure no regression in how tool responses are formatted for each LLM provider - both should handle PDFs as visual content similar to images

- [ ] 17. Update documentation and examples
  - Update user-facing documentation to explain the unified read tool capabilities
  - Provide examples of reading different file types
  - Document migration path from `read_image` to unified `read` tool
  - Update any CLI help text or usage messages that reference file reading
  - Add troubleshooting section for common issues (unsupported formats, size limits, etc.)

## Verification Criteria

- All existing unit tests pass without modification, confirming backward compatibility
- New tests cover text files, images, PDFs, and Jupyter notebooks with >90% code coverage
- Tool executor integration tests pass for both text and visual content through complete read flow
- Existing `read` tool calls continue to work identically for text files with line numbering and range support
- Existing `read_image` tool calls continue to work identically, automatically routing through unified service
- Image files are correctly detected by MIME type, base64-encoded, and returned as multimodal Image content to LLM
- PDF files are correctly detected by MIME type (application/pdf), base64-encoded as Image content with proper MIME type for LLM visual analysis
- Jupyter notebooks are read as plain JSON text with line numbers, no special parsing needed
- MIME type detection correctly classifies files into text vs visual content categories
- Size limits are enforced: text files use max_file_size, visual content (PDFs/images) uses max_image_size
- Error messages are clear and helpful for unsupported binary formats, size limit violations, and missing files
- Tool description accurately documents MIME-based detection and behavior for text vs visual content
- Performance is acceptable with no significant degradation from current implementation
- Metrics and telemetry correctly track MIME type and content type (text vs visual) in read operations
- LLM providers (Anthropic, OpenAI) correctly receive PDFs as visual content alongside images
- Conversation context and summaries correctly represent reads of different content types

## Potential Risks and Mitigations

1. **Breaking changes in tool response format**
   Mitigation: Maintain exact same response structure for text files (existing default behavior). Only return different content types for newly supported file formats. Extensive integration testing with existing test fixtures ensures no regression.

2. **Performance degradation from file type detection**
   Mitigation: Use fast extension-based detection as primary method. Only fall back to content sniffing when absolutely necessary. Benchmark performance with existing test suite to ensure no measurable slowdown.

3. **Memory consumption for large PDFs or notebooks**
   Mitigation: Implement appropriate size limits (max_pdf_size, max_notebook_size) with clear error messages. Consider streaming or chunking for very large files if needed. Test with realistically sized files.

4. **Complexity of maintaining two code paths during deprecation**
   Mitigation: Route deprecated `read_image` tool through unified service immediately rather than maintaining parallel implementations. Single source of truth reduces maintenance burden.

5. **LLM vision capabilities for PDF analysis**
   Mitigation: Verify that target LLM providers (Claude, GPT-4) support PDF visual analysis via base64 data URLs with application/pdf MIME type. Document any limitations in PDF page rendering or size. This approach requires vision-capable models but avoids complex PDF parsing dependencies.

6. **Jupyter notebook handling as plain JSON text**
   Mitigation: Send raw ipynb JSON as text and let LLM parse it natively - no need for custom parsing. LLM understands notebook structure inherently. If JSON is malformed, file read will fail at text reading stage with standard error handling.

7. **Backward compatibility with existing automation and scripts**
   Mitigation: Maintain existing tool call signatures without breaking changes. Preserve response format for text files. Extensive testing of existing tool call patterns ensures compatibility.

8. **MIME type detection accuracy when extensions are missing or incorrect**
   Mitigation: Use robust MIME detection library that examines content magic numbers as fallback. Provide optional file_type parameter for explicit specification. Return clear error messages when detection fails or format is unsupported. Document best practices for file naming.

## Alternative Approaches

1. **Keep separate tools, add more specialized tools**: Continue with separate `read`, `read_image`, `read_pdf`, `read_notebook` tools rather than unifying
   - Pros: Simpler individual implementations, clearer tool purposes, easier to understand tool catalog
   - Cons: More tools to document and maintain, users must know which tool to use, inconsistent API patterns, increased LLM confusion about tool selection
   - Trade-off: Unification provides better user experience at cost of more complex single implementation

2. **Create new unified tool, deprecate all existing read tools**: Introduce `read_file` as completely new tool and deprecate both `read` and `read_image`
   - Pros: Clean break, can design API from scratch, no legacy constraints
   - Cons: Breaking change for all users, requires migration period, increases tool count during transition
   - Trade-off: Enhancing existing `read` tool provides continuity while adding new capabilities

3. **Use content-based detection only**: Rely entirely on magic number/content sniffing rather than file extensions
2. **Use MIME type detection with magic numbers**: Rely on content sniffing (magic numbers) rather than file extensions for classification
   - Pros: More robust against misnamed files, doesn't rely on user file naming conventions, can detect actual file type
   - Cons: Requires reading file headers for every file, slightly slower performance
   - Trade-off: **Selected approach** - Use MIME detection library (like `infer` or `mime_guess`) that checks both extension and content for accurate classification, following OpenCode's pattern of using MIME types
4. **Separate service per file type with factory pattern**: Create separate services for each file type (TextReadService, ImageReadService, etc.) with factory to select
3. **Extract text from PDFs instead of sending as visual**: Parse PDF content to extract text using libraries like pdf-extract or lopdf
   - Pros: Works with non-vision LLMs, potentially more accurate text extraction, smaller token usage
   - Cons: Complex PDF parsing, doesn't handle scanned/image PDFs, loses visual structure/formatting, library dependencies
   - Trade-off: **Rejected** - Base64 visual approach is simpler, handles all PDF types uniformly, and leverages LLM vision capabilities. OpenCode demonstrates this pattern works well in practice.
4. **Parse Jupyter notebooks to extract and format cells**: Parse ipynb JSON to extract cell content, types, outputs and format nicely
   - Pros: Cleaner presentation, could hide noise from notebook metadata, structured output
   - Cons: Complex parsing logic, notebook format variations, loses context LLM might need, maintenance burden
   - Trade-off: **Rejected** - Sending raw JSON is simpler and LLMs understand ipynb structure natively. OpenCode demonstrates plain text approach works well. LLM can parse any notebook format/version without our code changes.

5. **Plugin architecture for file type handlers**: Create extensible plugin system where new file types can be added without modifying core service
   - Pros: Highly extensible, clean architecture, easy to add new formats in future
   - Cons: Over-engineering for current needs, added complexity of plugin system, registration and discovery overhead
   - Trade-off: **Rejected** - Direct implementation with MIME type branching is more appropriate given limited set of content types (text vs visual) and current requirements
