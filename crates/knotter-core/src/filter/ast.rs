use crate::domain::TagName;
use crate::rules::DueSelector;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchivedSelector {
    Archived,
    Active,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterExpr {
    Text(String),
    Tag(TagName),
    Due(DueSelector),
    Archived(ArchivedSelector),
    And(Vec<FilterExpr>),
}

pub type ContactFilter = FilterExpr;
