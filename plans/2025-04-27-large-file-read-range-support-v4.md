# Large File Range Reading Support (v4)

## Objective
Implement support for reading extremely large text files by adding range parameters (start_byte and end_byte) to the file read tool, allowing users to read specific portions of large files without loading the entire file into memory. Binary files should not be supported and UTF-8 character boundaries must always be respected.

## Implementation Plan

1. **Update `FsReadService` interface to support range reading**
   - Dependencies: None
   - Files: 
     - `crates/forge_services/src/infra.rs`
   - Notes: Add a new method to the trait that accepts start and end positions
   - Status: Not Started

2. **Implement the range reading functionality in `ForgeFileReadService`**
   - Dependencies: Task 1
   - Files: 
     - `crates/forge_infra/src/fs_read.rs`
   - Notes: Implement the new trait method using Tokio's file API for efficient reading
   - Status: Not Started

3. **Add binary file detection using the infer crate**
   - Dependencies: None
   - Files:
     - `crates/forge_fs/src/lib.rs`
     - `crates/forge_fs/Cargo.toml`
   - Notes: Add the infer crate as a dependency and implement a utility function to detect if a file is binary and return an appropriate error
   - Status: Not Started

4. **Update ForgeFS to support range reading with binary file validation**
   - Dependencies: Tasks 2, 3
   - Files: 
     - `crates/forge_fs/src/lib.rs`
   - Notes: Add a new method for range-based file reading that rejects binary files
   - Status: Not Started

5. **Implement UTF-8 boundary detection and correction**
   - Dependencies: Tasks 2, 4
   - Files:
     - `crates/forge_fs/src/lib.rs`
   - Notes: Ensure that range reads always align with UTF-8 character boundaries by adjusting the actual read range
   - Status: Not Started

6. **Update the `FSReadInput` struct to include optional range parameters**
   - Dependencies: None
   - Files: 
     - `crates/forge_services/src/tools/fs/fs_read.rs`
   - Notes: Add optional start_byte and end_byte fields to the input struct
   - Status: Not Started

7. **Modify FSRead tool implementation to support range reading and reject binary files**
   - Dependencies: Tasks 1, 2, 3, 4, 5, 6
   - Files: 
     - `crates/forge_services/src/tools/fs/fs_read.rs`
   - Notes: Update the `call` method to use the range-based reading with UTF-8 boundary adjustment and ensure binary files are rejected
   - Status: Not Started

8. **Update the FSRead tool description**
   - Dependencies: Task 6
   - Files: 
     - `crates/forge_services/src/tools/fs/fs_read.rs`
   - Notes: Update docstring to include range parameters in the tool description and explicitly mention that binary files are not supported and UTF-8 boundaries are always respected

      Sample Response:
      ```
      ---
      path: /a/b/c.txt
      range: 100-200
      total: 1024
      ---
      Hello! This is the contents of file c.txt
      ```
   - Status: Not Started

9. **Implement file size detection logic**
   - Dependencies: None
   - Files:
     - `crates/forge_fs/src/lib.rs`
   - Notes: Add functionality to efficiently determine file size without reading the entire file
   - Status: Not Started

10. **Add content length information to range read responses**
    - Dependencies: Task 9
    - Files:
      - `crates/forge_services/src/tools/fs/fs_read.rs`
    - Notes: Include total file size and adjusted range information in the response to help users understand the context of the range
    - Status: Not Started

11. **Add unit tests for range-based file reading and binary file rejection**
    - Dependencies: Tasks 1-10
    - Files: 
      - `crates/forge_services/src/tools/fs/fs_read.rs`
      - `crates/forge_infra/src/fs_read.rs`
      - `crates/forge_fs/src/lib.rs`
    - Notes: Test different range scenarios, edge cases, binary file detection with infer, UTF-8 boundary handling, and error conditions
    - Status: Not Started

## Verification Criteria
- The file read tool correctly returns only the requested range of bytes from large text files
- The tool properly identifies and rejects binary files with a clear error message using the infer crate
- The tool always adjusts range boundaries to respect UTF-8 character boundaries
- The tool handles edge cases properly:
  - When start_byte is beyond the file size
  - When end_byte is beyond the file size
  - When start_byte is greater than end_byte
  - When start_byte and end_byte are equal
  - When start_byte is negative or otherwise invalid
  - When reading from an empty file
  - When the range spans across malformed UTF-8 sequences
  - When the file is locked by another process
  - When reading from special files (e.g., device files, named pipes)
  - When hitting OS-specific file size limits
- The tool returns the entire file when no range is specified (backward compatibility)
- The tool provides helpful error messages for invalid range parameters
- The response includes information about the actual range read after UTF-8 boundary adjustment
- File size information is correctly included in the response
- Performance remains acceptable for both small and extremely large text files
- All unit tests pass
- Clippy runs with no errors or warnings

## Potential Risks and Mitigations

1. **Performance issues with extremely large text files**  
   Mitigation: 
   - Ensure that the implementation doesn't read the entire file when a range is specified
   - Use Tokio's file operations that support seeking and partial reads
   - Verify with benchmarks on files of various sizes (MB to GB)
   - Consider implementing a buffered reading strategy for large ranges

2. **UTF-8 boundary adjustment overhead**  
   Mitigation: 
   - Optimize the UTF-8 boundary detection algorithm for performance
   - Implement caching for boundary positions when repeated reads are requested
   - Use efficient byte scanning techniques that minimize CPU and memory usage
   - Provide clear metadata about the boundary adjustments that were made

3. **Breaking changes to the existing API**  
   Mitigation: 
   - Make the range parameters optional with default values that maintain backward compatibility
   - Document the behavior changes thoroughly
   - Ensure all existing tests continue to pass with the new implementation

4. **Inaccurate binary file detection**  
   Mitigation: 
   - Use the infer crate which provides robust file type detection
   - Still implement fallback checks for edge cases the infer crate might miss
   - Add comprehensive tests with various file types to verify correct detection
   - Provide clear error messages when a file is detected as binary

5. **File locking and concurrent access issues**  
   Mitigation:
   - Implement proper error handling for locked files
   - Use non-exclusive file handles when possible
   - Add retry logic with exponential backoff for temporary access issues

6. **Memory consumption with large ranges**  
   Mitigation:
   - Implement chunk-based reading for very large ranges
   - Set reasonable defaults and maximum values for range sizes
   - Add memory usage monitoring and provide warnings for potentially problematic operations

7. **Platform-specific issues**  
   Mitigation:
   - Test on all supported platforms (Windows, macOS, Linux)
   - Handle platform-specific file path conventions
   - Respect platform-specific file size limitations

8. **Invalid UTF-8 sequences in text files**  
   Mitigation:
   - Implement robust error handling for malformed UTF-8
   - Provide clear error messages when invalid UTF-8 is encountered
   - Consider options for replacement or reporting of invalid sequences

9. **Dependency management issues with infer crate**  
   Mitigation:
   - Pin to a specific version of the infer crate to avoid breaking changes
   - Monitor for security updates and issues with the infer crate
   - Have a fallback mechanism in case the infer crate fails

10. **Confusion for users with the new metadata in responses**  
    Mitigation:
    - Provide clear documentation on how to interpret the metadata
    - Include examples of how to use the new range parameters
    - Ensure backward compatibility so users not using ranges aren't affected

## Alternative Approaches

1. **Streaming API**: Implement a streaming interface for file reading instead of range-based reading. This would allow progressive loading of large files but would require more significant changes to the tool interface.

2. **File Pagination Tool**: Create a separate tool specifically for paginated file reading, leaving the original file read tool unchanged. This would maintain perfect backward compatibility but introduce redundancy.

3. **Content-Based Partitioning**: Implement intelligent partitioning based on content (e.g., by line, by paragraph, by JSON object) rather than raw bytes. This would be more semantic but more complex to implement.

4. **Fixed-size chunking**: Instead of arbitrary byte ranges, implement a chunking system where files are divided into fixed-size chunks that can be requested by index. This would simplify the API but reduce flexibility.

5. **Smart text-only file reading**: Implement a detection mechanism that automatically determines the optimal portion of a text file to return based on the context of the request, using language-aware boundaries like paragraphs or code blocks.

6. **Custom binary detection instead of infer**: Implement our own binary detection logic instead of relying on an external crate. This would reduce dependencies but require more maintenance and could be less accurate.

## Implementation Details

### Range Parameter Design

For the FSReadInput struct, add the following optional parameters:

```rust
/// Optional start position in bytes (0-based)
pub start_byte: Option<u64>,

/// Optional end position in bytes (exclusive)
pub end_byte: Option<u64>,
```

### Binary File Detection

To detect binary files, we'll use the `infer` crate:

1. Add the infer crate to dependencies in Cargo.toml:
   ```toml
   [dependencies]
   infer = "0.15.0"  # Use the latest version
   ```

2. Implement a utility function that:
   - Reads a small sample of the file (e.g., first 8KB)
   - Uses `infer::is_image()`, `infer::is_video()`, `infer::is_audio()`, `infer::is_archive()`, etc. to detect binary formats
   - For files not detected by infer, falls back to checking for null bytes or high concentration of non-printable characters
   - Returns a boolean indicating whether the file is likely binary

3. When a file is detected as binary, return an error message like:
   "Binary files are not supported. File detected as [file type]. Please use another tool or method to process this file."

### UTF-8 Boundary Detection and Adjustment

To ensure range reads respect UTF-8 character boundaries:

1. For the start position:
   - If the byte at start_byte is a UTF-8 continuation byte (10xxxxxx), scan backward to find the leading byte
   - Adjust start_byte to the position of the leading byte

2. For the end position:
   - If the byte at end_byte-1 is a leading byte of a multi-byte sequence, check if the complete character is included
   - If not, scan forward to include the complete character or backward to exclude the partial character

3. Report the adjusted positions in the response metadata

### Response Format

The response will include:

- The requested file content (as text)
- Metadata about the read operation:
  - Total file size
  - Original requested range
  - Actual range read after UTF-8 boundary adjustment
  - Information about boundary adjustments made

### Sample Response

The fs_read tool will return JSON with the following structure:

```json
{
  "content": "This is the file content within the requested range...",
  "metadata": {
    "file_size": 1024000,
    "requested_range": {
      "start_byte": 500,
      "end_byte": 1500
    },
    "actual_range": {
      "start_byte": 498,
      "end_byte": 1503
    },
    "boundary_adjustments": {
      "start_adjusted": true,
      "start_adjustment_reason": "UTF-8 character boundary alignment",
      "end_adjusted": true,
      "end_adjustment_reason": "UTF-8 character boundary alignment"
    },
    "is_partial": true,
    "percent_of_file": 0.1
  }
}
```

For error cases:

```json
{
  "error": "Binary files are not supported. File detected as image/png. Please use another tool or method to process this file."
}
```

Or for invalid ranges:

```json
{
  "error": "Invalid range specified: start_byte (5000) is greater than end_byte (4000)."
}
```

### Efficient Implementation Approach

To minimize memory usage and improve performance:

1. Use `tokio::fs::File::open()` to get a file handle
2. Read a small sample and perform binary file detection using the infer crate
3. Use `file.metadata()` to get the file size without reading content
4. Validate range parameters against file size
5. Use `file.seek()` to position near start_byte
6. Perform UTF-8 boundary detection and adjust start position if needed
7. Use `file.take(adjusted_end_byte - adjusted_start_byte)` to create a limited reader
8. Read from the limited reader into a buffer
9. Verify that the buffer contains valid UTF-8 and make any final adjustments
10. Return the buffer content with detailed metadata
