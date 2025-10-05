//! Cache infrastructure implementations.
//!
//! This module provides content-addressable caching using the cacache library.

mod cacache_repository;
mod mcp_cache_repository;

pub use cacache_repository::CacacheRepository;
pub use mcp_cache_repository::ForgeMcpCacheRepository;
