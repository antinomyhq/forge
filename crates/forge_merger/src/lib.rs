//! Forge Merger crate
//!
//! This crate provides functionality to merge all non-ignored files
//! in a directory into a single string, with an option for token-based
//! sharding.

mod merger;
mod shard;
mod token_estimator;

pub use merger::Merger;
pub use shard::Shard;
pub use token_estimator::TokenEstimator;
