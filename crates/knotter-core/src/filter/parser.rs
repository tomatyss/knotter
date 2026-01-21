use crate::domain::TagName;
use crate::filter::ast::{ArchivedSelector, ContactFilter, FilterExpr};
use crate::filter::FilterParseError;
use crate::rules::DueSelector;

pub fn parse_filter(input: &str) -> Result<ContactFilter, FilterParseError> {
    let mut terms = Vec::new();

    for token in input.split_whitespace() {
        if let Some(tag_raw) = token.strip_prefix('#') {
            if tag_raw.is_empty() {
                return Err(FilterParseError::EmptyTag);
            }
            let tag = TagName::new(tag_raw)
                .map_err(|_| FilterParseError::InvalidTag(tag_raw.to_string()))?;
            terms.push(FilterExpr::Tag(tag));
        } else if let Some(selector_raw) = token.strip_prefix("due:") {
            let selector = parse_due_selector(selector_raw)?;
            terms.push(FilterExpr::Due(selector));
        } else if let Some(selector_raw) = token.strip_prefix("archived:") {
            let selector = parse_archived_selector(selector_raw)?;
            terms.push(FilterExpr::Archived(selector));
        } else {
            terms.push(FilterExpr::Text(token.to_string()));
        }
    }

    Ok(FilterExpr::And(terms))
}

fn parse_due_selector(raw: &str) -> Result<DueSelector, FilterParseError> {
    match raw {
        "overdue" => Ok(DueSelector::Overdue),
        "today" => Ok(DueSelector::Today),
        "soon" => Ok(DueSelector::Soon),
        "any" => Ok(DueSelector::Any),
        "none" => Ok(DueSelector::None),
        _ => Err(FilterParseError::InvalidDueSelector(raw.to_string())),
    }
}

fn parse_archived_selector(raw: &str) -> Result<ArchivedSelector, FilterParseError> {
    match raw {
        "true" | "yes" | "1" | "archived" => Ok(ArchivedSelector::Archived),
        "false" | "no" | "0" | "active" => Ok(ArchivedSelector::Active),
        _ => Err(FilterParseError::InvalidArchivedSelector(raw.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_filter;
    use crate::domain::TagName;
    use crate::filter::ast::{ArchivedSelector, FilterExpr};
    use crate::filter::FilterParseError;
    use crate::rules::DueSelector;

    #[test]
    fn parse_tags_and_due() {
        let filter = parse_filter("#friends due:soon").unwrap();
        assert_eq!(
            filter,
            FilterExpr::And(vec![
                FilterExpr::Tag(TagName::new("friends").unwrap()),
                FilterExpr::Due(DueSelector::Soon)
            ])
        );
    }

    #[test]
    fn parse_archived_selector() {
        let filter = parse_filter("archived:true").unwrap();
        assert_eq!(
            filter,
            FilterExpr::And(vec![FilterExpr::Archived(ArchivedSelector::Archived)])
        );

        let filter = parse_filter("archived:active").unwrap();
        assert_eq!(
            filter,
            FilterExpr::And(vec![FilterExpr::Archived(ArchivedSelector::Active)])
        );
    }

    #[test]
    fn parse_text_terms() {
        let filter = parse_filter("alice bob").unwrap();
        assert_eq!(
            filter,
            FilterExpr::And(vec![
                FilterExpr::Text("alice".to_string()),
                FilterExpr::Text("bob".to_string())
            ])
        );
    }

    #[test]
    fn empty_tag_is_error() {
        let err = parse_filter("#").unwrap_err();
        assert_eq!(err, FilterParseError::EmptyTag);
    }

    #[test]
    fn invalid_due_is_error() {
        let err = parse_filter("due:later").unwrap_err();
        assert_eq!(
            err,
            FilterParseError::InvalidDueSelector("later".to_string())
        );
    }

    #[test]
    fn invalid_archived_is_error() {
        let err = parse_filter("archived:maybe").unwrap_err();
        assert_eq!(
            err,
            FilterParseError::InvalidArchivedSelector("maybe".to_string())
        );
    }
}
