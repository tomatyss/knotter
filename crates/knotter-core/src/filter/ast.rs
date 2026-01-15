use crate::domain::TagName;
use crate::rules::DueSelector;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterExpr {
    Text(String),
    Tag(TagName),
    Due(DueSelector),
    And(Vec<FilterExpr>),
}

pub type ContactFilter = FilterExpr;
