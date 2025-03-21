# Forge Merger

A utility library crate for the Forge project that merges all non-ignored files in a directory into a single string or multiple shards based on token limits. Each file's content is preceded by its full path enclosed in separators.

## Features

- Recursively scans directories using `forge_walker`
- Skips binary files and only processes text files
- Each file's content is preceded by its full path enclosed in separators
- Customizable separator (default: `================`)
- Token-based sharding for large projects
- Multiple token estimation strategies
- Detailed shard information including token counts and file paths

## Usage

### Basic Usage

```rust
use forge_merger::Merger;
use anyhow::Result;
use tokio::fs;

async fn merge_files() -> Result<()> {
    // Create a new merger instance
    let merger = Merger::new("/path/to/directory");
    
    // Process all files and get merged content as a string
    let merged_content = merger.process().await?;
    
    // Optionally write the merged content to a file
    fs::write("/path/to/output.txt", merged_content).await?;
    
    Ok(())
}
```

### Using Custom Separator

```rust
use forge_merger::Merger;
use anyhow::Result;

async fn merge_with_custom_separator() -> Result<()> {
    // Create a merger with a custom separator
    let merger = Merger::new("/path/to/directory")
        .with_separator("-------------------");
    
    // Process all files and get merged content
    let merged_content = merger.process().await?;
    
    println!("Merged content length: {} characters", merged_content.len());
    
    Ok(())
}
```

### Token-Based Sharding

```rust
use forge_merger::Merger;
use anyhow::Result;
use tokio::fs;

async fn shard_by_tokens() -> Result<()> {
    // Create a merger with token limit
    let merger = Merger::new("/path/to/project")
        .with_token_limit(8000);  // Target 8K tokens per shard
    
    // Process with sharding
    let shards = merger.process_sharded().await?;
    
    // Output information about shards
    for (i, shard) in shards.iter().enumerate() {
        println!("Shard {}: ~{} characters", i + 1, shard.len());
        
        // Optionally save each shard to a separate file
        fs::write(format!("shard_{}.txt", i + 1), shard).await?;
    }
    
    Ok(())
}
```

### Using Different Token Estimation Strategies

```rust
use forge_merger::{Merger, TokenEstimator};
use anyhow::Result;

async fn custom_estimation() -> Result<()> {
    // Create a merger with character-based token estimation
    let merger = Merger::new("/path/to/project")
        .with_token_limit(8000)
        .with_token_estimator(TokenEstimator::CharacterBased(4.0));
    
    let shards = merger.process_sharded().await?;
    println!("Created {} shards with character-based estimation", shards.len());
    
    // Using a custom token estimator
    let custom_merger = Merger::new("/path/to/project")
        .with_token_limit(8000)
        .with_token_estimator(TokenEstimator::Custom(|content| {
            // Custom logic here - this example just counts punctuation as additional tokens
            let word_count = content.split_whitespace().count();
            let punct_count = content.chars().filter(|c| c.is_ascii_punctuation()).count();
            word_count + (punct_count / 2)
        }));
    
    let custom_shards = custom_merger.process_sharded().await?;
    println!("Created {} shards with custom estimation", custom_shards.len());
    
    Ok(())
}
```

### Getting Detailed Shard Information

```rust
use forge_merger::Merger;
use anyhow::Result;

async fn shards_with_info() -> Result<()> {
    let merger = Merger::new("/path/to/project")
        .with_token_limit(8000);
    
    // Get detailed shard information
    let shards = merger.process_sharded_with_info().await?;
    
    for (i, shard) in shards.iter().enumerate() {
        println!("Shard {}:", i + 1);
        println!("  - Estimated tokens: {}", shard.estimated_tokens);
        println!("  - Files: {}", shard.file_paths.len());
        
        // Print the paths of the first few files
        for (j, path) in shard.file_paths.iter().take(3).enumerate() {
            println!("    {}: {}", j + 1, path.display());
        }
        
        if shard.file_paths.len() > 3 {
            println!("    ... and {} more files", shard.file_paths.len() - 3);
        }
    }
    
    Ok(())
}
```

## Output Format

The merged content will have the following format:

```
================
File: /full/path/to/file1.txt
================
Content of file1.txt

================
File: /full/path/to/file2.txt
================
Content of file2.txt

... and so on
```

## Token Estimation

The default token estimation strategy is word-count based with a multiplier of 0.75, which provides a reasonable approximation for code. You can choose from these options:

1. **Word Count Based**: Estimates tokens as `word_count * multiplier` (default: 0.75)
2. **Character Based**: Estimates tokens as `character_count / chars_per_token` (default: 4.0)
3. **Custom**: Use a custom function to estimate tokens based on content

The word-count method (default) works well for most programming languages and provides a good balance between accuracy and performance.