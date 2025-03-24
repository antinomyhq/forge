use nom::bytes::complete::{tag, take_until};
use nom::character::complete::line_ending;
use nom::combinator::{map, verify};
use nom::error::ErrorKind;
use nom::multi::many0;
use nom::sequence::delimited;
use nom::{Err as NomErr, IResult, Parser};
use thiserror::Error;

use super::marker::{DIVIDER, REPLACE, SEARCH};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error in block {position}: {kind}")]
    Block { position: usize, kind: Kind },
    #[error("No search/replace blocks found in content")]
    NoBlocks,
    #[error("Parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Error)]
pub enum Kind {
    #[error("Missing newline after SEARCH marker")]
    SearchNewline,
    #[error("Missing separator between search and replace content")]
    Separator,
    #[error("Missing newline after separator")]
    SeparatorNewline,
    #[error("Missing REPLACE marker")]
    ReplaceMarker,
    #[error("Incomplete block")]
    Incomplete,
    #[error("Invalid marker position - must start at beginning of line")]
    InvalidMarkerPosition,
}

#[derive(Debug, PartialEq)]
pub struct PatchBlock {
    pub search: String,
    pub replace: String,
}

/// Verify input starts with a newline or is at start of input
fn ensure_line_start(input: &str) -> bool {
    input.is_empty() || input.starts_with('\n') || input.len() == input.trim_start().len()
}

fn parse_search_marker(input: &str) -> IResult<&str, ()> {
    map(
        delimited(
            verify(take_until(SEARCH), |s: &str| ensure_line_start(s)),
            tag(SEARCH),
            line_ending,
        ),
        |_| (),
    )
    .parse(input)
}

fn parse_search_content(input: &str) -> IResult<&str, String> {
    map(take_until(DIVIDER), |s: &str| s.to_string()).parse(input)
}

fn parse_divider(input: &str) -> IResult<&str, ()> {
    map(
        delimited(
            verify(take_until(DIVIDER), ensure_line_start),
            tag(DIVIDER),
            line_ending,
        ),
        |_| (),
    )
    .parse(input)
}

fn parse_replace_content(input: &str) -> IResult<&str, String> {
    map(take_until(REPLACE), |s: &str| s.to_string()).parse(input)
}

fn parse_replace_marker(input: &str) -> IResult<&str, ()> {
    map(
        delimited(
            verify(take_until(REPLACE), ensure_line_start),
            tag(REPLACE),
            many0(line_ending),
        ),
        |_| (),
    )
    .parse(input)
}

fn parse_patch_block(input: &str) -> IResult<&str, PatchBlock> {
    map(
        (
            parse_search_marker,
            parse_search_content,
            parse_divider,
            parse_replace_content,
            parse_replace_marker,
        ),
        |(_, search, _, replace, _)| PatchBlock { search, replace },
    )
    .parse(input)
}

fn parse_one_block(input: &str, position: usize) -> Result<(&str, PatchBlock), Error> {
    if !input.contains(SEARCH) {
        return Err(Error::NoBlocks);
    }
    // Early marker position checks
    if let Some(search_idx) = input.find(SEARCH) {
        if !(ensure_line_start(&input[search_idx..]) || position == 1) {
            return Err(Error::Block { position, kind: Kind::InvalidMarkerPosition });
        }
        if !input[search_idx..].contains(&format!("{SEARCH}\n")) {
            return Err(Error::Block { position, kind: Kind::SearchNewline });
        }
    }

    if let Some(divider_idx) = input.find(DIVIDER) {
        if !ensure_line_start(&input[(divider_idx)..]) {
            return Err(Error::Block { position, kind: Kind::InvalidMarkerPosition });
        }
        if !input[divider_idx..].contains(&format!("{DIVIDER}\n")) {
            return Err(Error::Block { position, kind: Kind::SeparatorNewline });
        }
    }
    if let Some(replace_idx) = input.find(REPLACE) {
        if !ensure_line_start(&input[(replace_idx)..]) {
            return Err(Error::Block { position, kind: Kind::InvalidMarkerPosition });
        }
        if !input[replace_idx..].contains(&REPLACE.to_string()) {
            return Err(Error::Block { position, kind: Kind::Incomplete });
        }
    }

    // Now parse via the lower-level nom parser
    match parse_patch_block(input) {
        Ok((rest, block)) => Ok((rest, block)),
        Err(NomErr::Error(_)) => {
            if input.contains(SEARCH) && !input.contains(DIVIDER) {
                Err(Error::Block { position, kind: Kind::Separator })
            } else if input.contains(DIVIDER) && !input.contains(REPLACE) {
                Err(Error::Block { position, kind: Kind::ReplaceMarker })
            } else {
                Err(Error::Block { position, kind: Kind::Incomplete })
            }
        }
        Err(NomErr::Incomplete(_)) => Err(Error::Block { position, kind: Kind::Incomplete }),
        Err(e) => Err(Error::Parse(e.to_string())),
    }
}

/// Parse a single patch block, incrementing the position on success
fn parse_one_block_nom(position: &mut usize) -> impl FnMut(&str) -> IResult<&str, PatchBlock> + '_ {
    move |input: &str| {
        match parse_one_block(input, *position) {
            Ok((rest, block)) => {
                // If successful, increment the block position
                *position += 1;
                Ok((rest, block))
            }
            Err(Error::NoBlocks) => {
                // This signals “no more blocks” -> a soft error so `many0` stops
                Err(NomErr::Error(nom::error::Error::new(input, ErrorKind::Eof)))
            }
            Err(_) => {
                // Any real parse error => `Failure` so the entire parse aborts
                Err(NomErr::Failure(nom::error::Error::new(
                    input,
                    ErrorKind::Fail,
                )))
            }
        }
    }
}

/// Parse the input string into a series of patch blocks
pub fn parse_blocks(input: &str) -> Result<Vec<PatchBlock>, Error> {
    // Use nom::multi::many0 to parse 0 or more PatchBlock items
    use nom::combinator::map_res;
    use nom::multi::many0;

    let mut position = 1;
    let parser = parse_one_block_nom(&mut position);

    // many0(...) returns IResult<remaining, Vec<PatchBlock>>
    //   but if parse_one_block_nom(...) fails with Failure, the entire parse fails.
    //   if it returns Error, many0 stops and returns the blocks so far.
    let result = many0(map_res(
        parser,
        // map_res(...) expects to convert the internal Ok(...) into a new Result
        // But parse_one_block_nom already returns a normal IResult with PatchBlock,
        // so we can just do an identity here:
        |block: PatchBlock| -> Result<PatchBlock, Error> { Ok(block) },
    ))
    .parse(input);

    match result {
        Ok((_remaining, blocks)) => {
            if blocks.is_empty() {
                // This means we never successfully parsed any block
                return Err(Error::NoBlocks);
            }
            Ok(blocks)
        }
        Err(NomErr::Incomplete(_)) => Err(Error::Block { position, kind: Kind::Incomplete }),
        Err(NomErr::Error(_)) | Err(NomErr::Failure(_)) => {
            match parse_one_block(input, position) {
                Err(e) => Err(e),
                // If it unexpectedly succeeds, we just fallback:
                Ok(_) => Err(Error::Block { position, kind: Kind::Incomplete }),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_parse_blocks_missing_separator() {
        let diff = format!("{SEARCH}\nsearch content\n");
        let result = parse_blocks(&diff);
        assert!(matches!(
            result.unwrap_err(),
            Error::Block { position: 1, kind: Kind::Separator }
        ));
    }

    #[test]
    fn test_parse_blocks_missing_newline() {
        let diff = format!("{SEARCH}search content");
        let result = parse_blocks(&diff);
        assert!(matches!(
            result.unwrap_err(),
            Error::Block { position: 1, kind: Kind::SearchNewline }
        ));
    }

    #[test]
    fn test_parse_blocks_missing_separator_newline() {
        let diff = format!("{SEARCH}\nsearch content\n{DIVIDER}content");
        let result = parse_blocks(&diff);
        assert!(matches!(
            result.unwrap_err(),
            Error::Block { position: 1, kind: Kind::SeparatorNewline }
        ));
    }

    #[test]
    fn test_parse_blocks_missing_replace_marker() {
        let diff = format!("{SEARCH}\nsearch content\n{DIVIDER}\nreplace content\n");
        let result = parse_blocks(&diff);
        assert!(matches!(
            result.unwrap_err(),
            Error::Block { position: 1, kind: Kind::ReplaceMarker }
        ));
    }

    #[test]
    fn test_parse_blocks_no_blocks() {
        // Test both an empty string and random content
        let empty_result = parse_blocks("");
        assert!(matches!(empty_result.unwrap_err(), Error::NoBlocks));

        let random_result = parse_blocks("some random content");
        assert!(matches!(random_result.unwrap_err(), Error::NoBlocks));
    }

    #[test]
    fn test_parse_blocks_multiple_blocks_with_error() {
        let diff = format!(
            "{SEARCH}\nfirst block\n{DIVIDER}\nreplacement\n{REPLACE}\n{SEARCH}\nsecond block\n{DIVIDER}missing_newline"
        );
        let result = parse_blocks(&diff);
        println!("{:?}", result);
        assert!(matches!(
            result.unwrap_err(),
            Error::Block { position: 2, kind: Kind::Incomplete }
        ));
    }

    #[test]
    fn test_error_messages() {
        // Test error message formatting for block errors
        let diff = format!("{SEARCH}search content");
        let err = parse_blocks(&diff).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Error in block 1: Missing newline after SEARCH marker"
        );

        // Test error message for no blocks
        let err = parse_blocks("").unwrap_err();
        assert_eq!(err.to_string(), "No search/replace blocks found in content");
    }

    #[test]
    fn test_valid_single_block() {
        let diff = format!("{SEARCH}\nold code\n{DIVIDER}\nnew code\n{REPLACE}\n");
        let result = parse_blocks(&diff).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].search, "old code\n");
        assert_eq!(result[0].replace, "new code\n");
    }

    #[test]
    fn test_valid_multiple_blocks() {
        let diff = format!(
            "{SEARCH}\nfirst old\n{DIVIDER}\nfirst new\n{REPLACE}\n{SEARCH}\nsecond old\n{DIVIDER}\nsecond new\n{REPLACE}\n"
        );
        let result = parse_blocks(&diff).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].search, "first old\n");
        assert_eq!(result[0].replace, "first new\n");
        assert_eq!(result[1].search, "second old\n");
        assert_eq!(result[1].replace, "second new\n");
    }

    #[test]
    fn test_empty_sections() {
        let diff = format!("{SEARCH}\n{DIVIDER}\n{REPLACE}\n");
        let result = parse_blocks(&diff).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].search, "");
        assert_eq!(result[0].replace, "");
    }

    #[test]
    fn test_whitespace_preservation() {
        let diff = format!(
            "{SEARCH}\n    indented\n\n  spaces  \n{DIVIDER}\n\tindented\n\n\ttabbed\n{REPLACE}\n"
        );
        let result = parse_blocks(&diff).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].search, "    indented\n\n  spaces  \n");
        assert_eq!(result[0].replace, "\tindented\n\n\ttabbed\n");
    }

    #[test]
    fn test_unicode_content() {
        let diff = format!("{SEARCH}\n🦀 Rust\n{DIVIDER}\n📦 Crate\n{REPLACE}\n");
        let result = parse_blocks(&diff).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].search, "🦀 Rust\n");
        assert_eq!(result[0].replace, "📦 Crate\n");
    }

    #[test]
    fn test_markers_must_end_with_newline() {
        // Test SEARCH marker
        let diff = format!("{SEARCH}code\n{DIVIDER}\nnew\n{REPLACE}\n");
        let result = parse_blocks(&diff);
        assert!(matches!(
            result.unwrap_err(),
            Error::Block { position: 1, kind: Kind::SearchNewline }
        ));

        // Test DIVIDER marker
        let diff = format!("{SEARCH}\ncode\n{DIVIDER}new\n{REPLACE}\n");
        let result = parse_blocks(&diff);
        assert!(matches!(
            result.unwrap_err(),
            Error::Block { position: 1, kind: Kind::SeparatorNewline }
        ));
    }
    #[test]
    fn test_multiple_blocks_without_newline_end() {
        let diff = "<<<<<<< SEARCH\nhello\nhi\n=======\nhey\nhello\nhola\n>>>>>>> REPLACE\n\n<<<<<<< SEARCH\n            hola,\n=======\n            hey\n>>>>>>> REPLACE";
        let result = parse_blocks(diff);
        assert!(result.is_ok())
    }
}
