mod ast;
mod parser;

use thiserror::Error;

pub use ast::{ArchivedSelector, ContactFilter, FilterExpr};
pub use parser::parse_filter;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FilterParseError {
    #[error("empty tag token")]
    EmptyTag,
    #[error("invalid due selector: {0}")]
    InvalidDueSelector(String),
    #[error("invalid archived selector: {0}")]
    InvalidArchivedSelector(String),
    #[error("invalid tag: {0}")]
    InvalidTag(String),
}
