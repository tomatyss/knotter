use crate::domain::TagName;
use crate::error::CoreError;
use crate::rules::cadence::MAX_CADENCE_DAYS;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LoopStrategy {
    #[default]
    Shortest,
    Priority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopRule {
    pub tag: TagName,
    pub cadence_days: i32,
    pub priority: i32,
}

impl LoopRule {
    pub fn new(tag: TagName, cadence_days: i32, priority: i32) -> Result<Self, CoreError> {
        if cadence_days <= 0 || cadence_days > MAX_CADENCE_DAYS {
            return Err(CoreError::InvalidCadenceDays(cadence_days));
        }

        Ok(Self {
            tag,
            cadence_days,
            priority,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoopPolicy {
    pub default_cadence_days: Option<i32>,
    pub strategy: LoopStrategy,
    pub rules: Vec<LoopRule>,
}

impl LoopPolicy {
    pub fn resolve_cadence<'a, I>(&self, tags: I) -> Option<i32>
    where
        I: IntoIterator<Item = &'a str>,
    {
        self.resolve_cadence_with_match(tags).0
    }

    pub fn resolve_cadence_with_match<'a, I>(&self, tags: I) -> (Option<i32>, bool)
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut tag_set: HashSet<&'a str> = HashSet::new();
        for tag in tags {
            let trimmed = tag.trim();
            if !trimmed.is_empty() {
                tag_set.insert(trimmed);
            }
        }

        match self.strategy {
            LoopStrategy::Shortest => {
                let mut best: Option<i32> = None;
                let mut matched = false;
                for rule in self
                    .rules
                    .iter()
                    .filter(|rule| tag_set.contains(rule.tag.as_str()))
                {
                    matched = true;
                    best = Some(match best {
                        None => rule.cadence_days,
                        Some(current) => current.min(rule.cadence_days),
                    });
                }
                if matched {
                    (best, true)
                } else {
                    (self.default_cadence_days, false)
                }
            }
            LoopStrategy::Priority => {
                let mut best: Option<&LoopRule> = None;
                let mut matched = false;
                for rule in self
                    .rules
                    .iter()
                    .filter(|rule| tag_set.contains(rule.tag.as_str()))
                {
                    matched = true;
                    best = Some(select_priority(best, rule));
                }
                if matched {
                    (best.map(|rule| rule.cadence_days), true)
                } else {
                    (self.default_cadence_days, false)
                }
            }
        }
    }
}

fn select_priority<'a>(current: Option<&'a LoopRule>, candidate: &'a LoopRule) -> &'a LoopRule {
    match current {
        None => candidate,
        Some(existing) => {
            if candidate.priority > existing.priority {
                candidate
            } else if candidate.priority < existing.priority {
                existing
            } else if candidate.cadence_days < existing.cadence_days {
                candidate
            } else if candidate.cadence_days > existing.cadence_days {
                existing
            } else if candidate.tag.as_str() < existing.tag.as_str() {
                candidate
            } else {
                existing
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LoopPolicy, LoopRule, LoopStrategy};
    use crate::domain::TagName;

    #[test]
    fn resolve_shortest_prefers_lowest_cadence() {
        let policy = LoopPolicy {
            default_cadence_days: Some(180),
            strategy: LoopStrategy::Shortest,
            rules: vec![
                LoopRule::new(TagName::new("friend").unwrap(), 90, 0).unwrap(),
                LoopRule::new(TagName::new("family").unwrap(), 30, 0).unwrap(),
            ],
        };

        let cadence = policy.resolve_cadence(["friend", "family"].iter().copied());
        assert_eq!(cadence, Some(30));
    }

    #[test]
    fn resolve_priority_prefers_higher_priority() {
        let policy = LoopPolicy {
            default_cadence_days: Some(180),
            strategy: LoopStrategy::Priority,
            rules: vec![
                LoopRule::new(TagName::new("friend").unwrap(), 90, 10).unwrap(),
                LoopRule::new(TagName::new("family").unwrap(), 30, 5).unwrap(),
            ],
        };

        let cadence = policy.resolve_cadence(["friend", "family"].iter().copied());
        assert_eq!(cadence, Some(90));
    }

    #[test]
    fn resolve_priority_tiebreaks_on_shorter_cadence() {
        let policy = LoopPolicy {
            default_cadence_days: None,
            strategy: LoopStrategy::Priority,
            rules: vec![
                LoopRule::new(TagName::new("friend").unwrap(), 90, 5).unwrap(),
                LoopRule::new(TagName::new("family").unwrap(), 30, 5).unwrap(),
            ],
        };

        let cadence = policy.resolve_cadence(["friend", "family"].iter().copied());
        assert_eq!(cadence, Some(30));
    }

    #[test]
    fn resolve_falls_back_to_default_when_no_match() {
        let policy = LoopPolicy {
            default_cadence_days: Some(180),
            strategy: LoopStrategy::Shortest,
            rules: vec![LoopRule::new(TagName::new("friend").unwrap(), 90, 0).unwrap()],
        };

        let cadence = policy.resolve_cadence(["coworker"].iter().copied());
        assert_eq!(cadence, Some(180));
    }
}
