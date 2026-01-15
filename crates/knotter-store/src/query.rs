use crate::error::{Result, StoreError};
use chrono::{DateTime, Duration, FixedOffset, NaiveDateTime, TimeZone, Utc};
use knotter_core::domain::TagName;
use knotter_core::filter::{ContactFilter, FilterExpr};
use knotter_core::rules::DueSelector;
use rusqlite::types::Value;

#[derive(Debug, Default, Clone)]
pub struct ContactQuery {
    pub text_terms: Vec<String>,
    pub tags: Vec<TagName>,
    pub due: Option<DueSelector>,
}

pub struct SqlQuery {
    pub sql: String,
    pub params: Vec<Value>,
}

impl ContactQuery {
    pub fn from_filter(filter: &ContactFilter) -> Result<Self> {
        let mut query = ContactQuery::default();
        match filter {
            FilterExpr::And(terms) => {
                for term in terms {
                    query.push_expr(term)?;
                }
            }
            term => query.push_expr(term)?,
        }
        Ok(query)
    }

    fn push_expr(&mut self, expr: &FilterExpr) -> Result<()> {
        match expr {
            FilterExpr::Text(text) => self.text_terms.push(text.to_string()),
            FilterExpr::Tag(tag) => self.tags.push(tag.clone()),
            FilterExpr::Due(selector) => {
                if self.due.is_some() {
                    return Err(StoreError::InvalidFilter(
                        "multiple due filters are not supported".to_string(),
                    ));
                }
                self.due = Some(*selector);
            }
            FilterExpr::And(terms) => {
                for term in terms {
                    self.push_expr(term)?;
                }
            }
        }
        Ok(())
    }

    pub fn to_sql(
        &self,
        now_utc: i64,
        soon_days: i64,
        local_offset: FixedOffset,
    ) -> Result<SqlQuery> {
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Value> = Vec::new();

        for term in &self.text_terms {
            clauses.push(
                "(display_name LIKE ? OR email LIKE ? OR phone LIKE ? OR handle LIKE ?)"
                    .to_string(),
            );
            let like = format!("%{}%", term);
            params.push(Value::from(like.clone()));
            params.push(Value::from(like.clone()));
            params.push(Value::from(like.clone()));
            params.push(Value::from(like));
        }

        for tag in &self.tags {
            clauses.push(
                "EXISTS (SELECT 1 FROM contact_tags ct INNER JOIN tags t ON t.id = ct.tag_id WHERE ct.contact_id = contacts.id AND t.name = ?)"
                    .to_string(),
            );
            params.push(Value::from(tag.as_str().to_string()));
        }

        let bounds = day_bounds(now_utc, soon_days, local_offset);
        if let Some(selector) = self.due {
            match selector {
                DueSelector::Overdue => {
                    clauses.push("next_touchpoint_at IS NOT NULL AND next_touchpoint_at < ?".to_string());
                    params.push(Value::from(now_utc));
                }
                DueSelector::Today => {
                    clauses.push("next_touchpoint_at >= ? AND next_touchpoint_at < ?".to_string());
                    params.push(Value::from(bounds.start_of_today));
                    params.push(Value::from(bounds.start_of_tomorrow));
                }
                DueSelector::Soon => {
                    clauses.push("next_touchpoint_at >= ? AND next_touchpoint_at < ?".to_string());
                    params.push(Value::from(bounds.start_of_tomorrow));
                    params.push(Value::from(bounds.soon_end));
                }
                DueSelector::Any => {
                    clauses.push("next_touchpoint_at IS NOT NULL".to_string());
                }
                DueSelector::None => {
                    clauses.push("next_touchpoint_at IS NULL".to_string());
                }
            }
        }

        let mut sql = String::from(
            "SELECT id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at FROM contacts",
        );

        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }

        sql.push_str(
            " ORDER BY CASE
                WHEN next_touchpoint_at IS NULL THEN 4
                WHEN next_touchpoint_at < ? THEN 0
                WHEN next_touchpoint_at >= ? AND next_touchpoint_at < ? THEN 1
                WHEN next_touchpoint_at >= ? AND next_touchpoint_at < ? THEN 2
                ELSE 3
            END,
            display_name COLLATE NOCASE ASC",
        );

        params.push(Value::from(now_utc));
        params.push(Value::from(bounds.start_of_today));
        params.push(Value::from(bounds.start_of_tomorrow));
        params.push(Value::from(bounds.start_of_tomorrow));
        params.push(Value::from(bounds.soon_end));

        Ok(SqlQuery { sql, params })
    }
}

#[derive(Debug, Clone, Copy)]
struct DueBounds {
    start_of_today: i64,
    start_of_tomorrow: i64,
    soon_end: i64,
}

fn day_bounds(now_utc: i64, soon_days: i64, local_offset: FixedOffset) -> DueBounds {
    let now = DateTime::<Utc>::from_utc(
        NaiveDateTime::from_timestamp_opt(now_utc, 0).expect("valid timestamp"),
        Utc,
    );
    let local = now.with_timezone(&local_offset);
    let local_date = local.date_naive();
    let start_of_today_local = local_date
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid");
    let start_of_tomorrow_local = start_of_today_local + Duration::days(1);

    let start_of_today = local_offset
        .from_local_datetime(&start_of_today_local)
        .single()
        .expect("fixed offset conversion")
        .with_timezone(&Utc)
        .timestamp();
    let start_of_tomorrow = local_offset
        .from_local_datetime(&start_of_tomorrow_local)
        .single()
        .expect("fixed offset conversion")
        .with_timezone(&Utc)
        .timestamp();

    let soon_end = start_of_tomorrow + Duration::days(soon_days).num_seconds();

    DueBounds {
        start_of_today,
        start_of_tomorrow,
        soon_end,
    }
}
