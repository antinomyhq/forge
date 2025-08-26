use thiserror::Error;

#[derive(Error, Debug)]
pub enum JsonRepairError {
    #[error("Invalid character {character:?} at position {position}")]
    InvalidCharacter { character: char, position: usize },

    #[error("Unexpected character {character:?} at position {position}")]
    UnexpectedCharacter { character: char, position: usize },

    #[error("Unexpected end of JSON string at position {position}")]
    UnexpectedEnd { position: usize },

    #[error("Object key expected at position {position}")]
    ObjectKeyExpected { position: usize },

    #[error("Colon expected at position {position}")]
    ColonExpected { position: usize },

    #[error("Invalid unicode character {chars:?} at position {position}")]
    InvalidUnicodeCharacter { chars: String, position: usize },

    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, JsonRepairError>;
