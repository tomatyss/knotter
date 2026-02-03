use crate::error::{Result, StoreError};
use crate::query::{due_bounds, ContactQuery};
use crate::repo::merge_candidates::MergeCandidateStatus;
use chrono::FixedOffset;
use knotter_core::domain::{normalize_email, Contact, ContactId, TagName};
use knotter_core::rules::validate_soon_days;
use rusqlite::{params, params_from_iter, Connection};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct ContactNew {
    pub display_name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub handle: Option<String>,
    pub timezone: Option<String>,
    pub next_touchpoint_at: Option<i64>,
    pub cadence_days: Option<i32>,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct ContactUpdate {
    pub display_name: Option<String>,
    pub email: Option<Option<String>>,
    pub email_source: Option<String>,
    pub phone: Option<Option<String>>,
    pub handle: Option<Option<String>>,
    pub timezone: Option<Option<String>>,
    pub next_touchpoint_at: Option<Option<i64>>,
    pub cadence_days: Option<Option<i32>>,
    pub archived_at: Option<Option<i64>>,
}

#[derive(Debug, Clone)]
pub enum EmailOps {
    None,
    Replace {
        emails: Vec<String>,
        primary: Option<String>,
        source: Option<String>,
    },
    Mutate {
        clear: bool,
        add: Vec<String>,
        remove: Vec<String>,
        source: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergePreference {
    Primary,
    Secondary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeTouchpointPreference {
    Primary,
    Secondary,
    Earliest,
    Latest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeArchivedPreference {
    ActiveIfAny,
    Primary,
    Secondary,
}

#[derive(Debug, Clone)]
pub struct ContactMergeOptions {
    pub prefer: MergePreference,
    pub touchpoint: MergeTouchpointPreference,
    pub archived: MergeArchivedPreference,
}

impl Default for ContactMergeOptions {
    fn default() -> Self {
        Self {
            prefer: MergePreference::Primary,
            touchpoint: MergeTouchpointPreference::Earliest,
            archived: MergeArchivedPreference::ActiveIfAny,
        }
    }
}

pub struct ContactsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> ContactsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn create(&self, now_utc: i64, input: ContactNew) -> Result<Contact> {
        let tx = self.conn.unchecked_transaction()?;
        let contact = create_inner(&tx, now_utc, input)?;
        if let Some(email) = contact.email.as_deref() {
            crate::repo::emails::EmailsRepo::new(&tx).add_email(
                now_utc,
                &contact.id,
                email,
                Some("primary"),
                true,
            )?;
        }
        tx.commit()?;
        Ok(contact)
    }

    pub fn create_with_emails_and_tags(
        &self,
        now_utc: i64,
        input: ContactNew,
        tags: Vec<TagName>,
        emails: Vec<String>,
        source: Option<&str>,
    ) -> Result<Contact> {
        if self.conn.is_autocommit() {
            let tx = self.conn.unchecked_transaction()?;
            let contact =
                create_with_emails_and_tags_inner(&tx, now_utc, input, tags, emails, source)?;
            tx.commit()?;
            Ok(contact)
        } else {
            create_with_emails_and_tags_inner(self.conn, now_utc, input, tags, emails, source)
        }
    }

    pub fn create_with_tags(
        &self,
        now_utc: i64,
        input: ContactNew,
        tags: Vec<TagName>,
    ) -> Result<Contact> {
        if self.conn.is_autocommit() {
            let tx = self.conn.unchecked_transaction()?;
            let contact = create_with_tags_inner(&tx, now_utc, input, tags)?;
            tx.commit()?;
            Ok(contact)
        } else {
            create_with_tags_inner(self.conn, now_utc, input, tags)
        }
    }

    pub fn get(&self, id: ContactId) -> Result<Option<Contact>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at
             FROM contacts WHERE id = ?1;",
        )?;
        let mut rows = stmt.query([id.to_string()])?;
        if let Some(row) = rows.next()? {
            Ok(Some(contact_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn list_by_email(&self, email: &str) -> Result<Vec<Contact>> {
        let mut stmt = self.conn.prepare(
            "SELECT c.id, c.display_name, c.email, c.phone, c.handle, c.timezone, c.next_touchpoint_at, c.cadence_days, c.created_at, c.updated_at, c.archived_at
             FROM contacts c
             INNER JOIN contact_emails ce ON ce.contact_id = c.id
             WHERE ce.email = ?1
             ORDER BY (c.archived_at IS NOT NULL) ASC, c.updated_at DESC;",
        )?;
        let mut rows = stmt.query([normalize_email(email).unwrap_or_else(|| email.to_string())])?;
        let mut contacts = Vec::new();
        while let Some(row) = rows.next()? {
            contacts.push(contact_from_row(row)?);
        }
        Ok(contacts)
    }

    pub fn list_by_display_name(&self, name: &str) -> Result<Vec<Contact>> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at
             FROM contacts
             WHERE display_name = ?1 COLLATE NOCASE
             ORDER BY (archived_at IS NOT NULL) ASC, updated_at DESC;",
        )?;
        let mut rows = stmt.query([trimmed])?;
        let mut contacts = Vec::new();
        while let Some(row) = rows.next()? {
            contacts.push(contact_from_row(row)?);
        }
        Ok(contacts)
    }

    pub fn list_by_handle(&self, handle: &str) -> Result<Vec<Contact>> {
        let trimmed = handle.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = self.conn.prepare(
            "SELECT id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at
             FROM contacts
             WHERE handle = ?1 COLLATE NOCASE
             ORDER BY (archived_at IS NOT NULL) ASC, updated_at DESC;",
        )?;
        let mut rows = stmt.query([trimmed])?;
        let mut contacts = Vec::new();
        while let Some(row) = rows.next()? {
            contacts.push(contact_from_row(row)?);
        }
        Ok(contacts)
    }

    pub fn update(&self, now_utc: i64, id: ContactId, update: ContactUpdate) -> Result<Contact> {
        if self.conn.is_autocommit() {
            let tx = self.conn.unchecked_transaction()?;
            let contact = update_inner(&tx, now_utc, id, update)?;
            tx.commit()?;
            Ok(contact)
        } else {
            update_inner(self.conn, now_utc, id, update)
        }
    }

    pub fn update_with_email_ops(
        &self,
        now_utc: i64,
        id: ContactId,
        update: ContactUpdate,
        email_ops: EmailOps,
    ) -> Result<Contact> {
        if self.conn.is_autocommit() {
            let tx = self.conn.unchecked_transaction()?;
            let contact = update_with_email_ops_inner(&tx, now_utc, id, update, email_ops)?;
            tx.commit()?;
            Ok(contact)
        } else {
            update_with_email_ops_inner(self.conn, now_utc, id, update, email_ops)
        }
    }

    pub fn delete(&self, now_utc: i64, id: ContactId) -> Result<()> {
        if self.conn.is_autocommit() {
            let tx = self.conn.unchecked_transaction()?;
            delete_inner(&tx, now_utc, id)?;
            tx.commit()?;
            Ok(())
        } else {
            delete_inner(self.conn, now_utc, id)
        }
    }

    pub fn archive(&self, now_utc: i64, id: ContactId) -> Result<Contact> {
        let update = ContactUpdate {
            archived_at: Some(Some(now_utc)),
            ..Default::default()
        };
        self.update(now_utc, id, update)
    }

    pub fn unarchive(&self, now_utc: i64, id: ContactId) -> Result<Contact> {
        let update = ContactUpdate {
            archived_at: Some(None),
            ..Default::default()
        };
        self.update(now_utc, id, update)
    }

    pub fn merge_contacts(
        &self,
        now_utc: i64,
        primary_id: ContactId,
        secondary_id: ContactId,
        options: ContactMergeOptions,
    ) -> Result<Contact> {
        if self.conn.is_autocommit() {
            let tx = self.conn.unchecked_transaction()?;
            let contact = merge_contacts_inner(&tx, now_utc, primary_id, secondary_id, options)?;
            tx.commit()?;
            Ok(contact)
        } else {
            merge_contacts_inner(self.conn, now_utc, primary_id, secondary_id, options)
        }
    }

    pub fn list_all(&self) -> Result<Vec<Contact>> {
        let query = ContactQuery::default();
        self.list_contacts(&query, 0, 7, FixedOffset::east_opt(0).expect("utc offset"))
    }

    pub fn list_contacts(
        &self,
        query: &ContactQuery,
        now_utc: i64,
        soon_days: i64,
        local_offset: FixedOffset,
    ) -> Result<Vec<Contact>> {
        let compiled = query.to_sql(now_utc, soon_days, local_offset)?;
        let mut stmt = self.conn.prepare(&compiled.sql)?;
        let mut rows = stmt.query(params_from_iter(compiled.params))?;
        let mut contacts = Vec::new();
        while let Some(row) = rows.next()? {
            contacts.push(contact_from_row(row)?);
        }
        Ok(contacts)
    }

    pub fn list_due_contacts(
        &self,
        now_utc: i64,
        soon_days: i64,
        local_offset: FixedOffset,
    ) -> Result<Vec<Contact>> {
        let soon_days = validate_soon_days(soon_days).map_err(StoreError::Core)?;
        let bounds = due_bounds(now_utc, soon_days, local_offset);
        let mut stmt = self.conn.prepare(
            "SELECT id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at
             FROM contacts
             WHERE archived_at IS NULL
               AND next_touchpoint_at IS NOT NULL
               AND next_touchpoint_at < ?1
             ORDER BY CASE
                WHEN next_touchpoint_at < ?2 THEN 0
                WHEN next_touchpoint_at >= ?3 AND next_touchpoint_at < ?4 THEN 1
                WHEN next_touchpoint_at >= ?4 AND next_touchpoint_at < ?5 THEN 2
                ELSE 3
             END,
             display_name COLLATE NOCASE ASC;",
        )?;
        let mut rows = stmt.query(params![
            bounds.soon_end,
            now_utc,
            bounds.start_of_today,
            bounds.start_of_tomorrow,
            bounds.soon_end
        ])?;
        let mut contacts = Vec::new();
        while let Some(row) = rows.next()? {
            contacts.push(contact_from_row(row)?);
        }
        Ok(contacts)
    }
}

fn create_inner(conn: &Connection, now_utc: i64, input: ContactNew) -> Result<Contact> {
    let contact = Contact {
        id: ContactId::new(),
        display_name: input.display_name,
        email: input.email.and_then(|email| normalize_email(&email)),
        phone: input.phone,
        handle: input.handle,
        timezone: input.timezone,
        next_touchpoint_at: input.next_touchpoint_at,
        cadence_days: input.cadence_days,
        created_at: now_utc,
        updated_at: now_utc,
        archived_at: input.archived_at,
    };

    contact.validate()?;

    conn.execute(
        "INSERT INTO contacts (id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11);",
        params![
            contact.id.to_string(),
            contact.display_name,
            contact.email,
            contact.phone,
            contact.handle,
            contact.timezone,
            contact.next_touchpoint_at,
            contact.cadence_days,
            contact.created_at,
            contact.updated_at,
            contact.archived_at,
        ],
    )?;

    Ok(contact)
}

fn create_with_tags_inner(
    conn: &Connection,
    now_utc: i64,
    input: ContactNew,
    tags: Vec<TagName>,
) -> Result<Contact> {
    let contact = create_inner(conn, now_utc, input)?;
    if let Some(email) = contact.email.as_deref() {
        crate::repo::emails::EmailsRepo::new(conn).add_email(
            now_utc,
            &contact.id,
            email,
            Some("primary"),
            true,
        )?;
    }
    if !tags.is_empty() {
        crate::repo::tags::set_contact_tags_inner(conn, &contact.id.to_string(), tags)?;
    }
    Ok(contact)
}

fn update_inner(
    conn: &Connection,
    now_utc: i64,
    id: ContactId,
    update: ContactUpdate,
) -> Result<Contact> {
    let mut contact = get_inner(conn, id)?.ok_or_else(|| StoreError::NotFound(id.to_string()))?;

    if let Some(value) = update.display_name {
        contact.display_name = value;
    }
    let email_update = update.email.is_some();
    let normalized_email = update
        .email
        .as_ref()
        .and_then(|value| value.as_deref())
        .and_then(normalize_email);
    if let Some(email) = normalized_email.as_deref() {
        if let Some(existing_id) =
            crate::repo::emails::EmailsRepo::new(conn).find_contact_id_by_email(email)?
        {
            if existing_id != contact.id {
                return Err(StoreError::DuplicateEmail(email.to_string()));
            }
        }
    }
    if email_update {
        contact.email = normalized_email.clone();
    }
    if let Some(value) = update.phone {
        contact.phone = value;
    }
    if let Some(value) = update.handle {
        contact.handle = value;
    }
    if let Some(value) = update.timezone {
        contact.timezone = value;
    }
    if let Some(value) = update.next_touchpoint_at {
        contact.next_touchpoint_at = value;
    }
    if let Some(value) = update.cadence_days {
        contact.cadence_days = value;
    }
    if let Some(value) = update.archived_at {
        contact.archived_at = value;
    }

    contact.updated_at = now_utc;
    contact.validate()?;

    conn.execute(
        "UPDATE contacts SET display_name = ?2, email = ?3, phone = ?4, handle = ?5, timezone = ?6, next_touchpoint_at = ?7, cadence_days = ?8, updated_at = ?9, archived_at = ?10
         WHERE id = ?1;",
        params![
            contact.id.to_string(),
            contact.display_name,
            contact.email,
            contact.phone,
            contact.handle,
            contact.timezone,
            contact.next_touchpoint_at,
            contact.cadence_days,
            contact.updated_at,
            contact.archived_at,
        ],
    )?;

    if email_update {
        let emails = crate::repo::emails::EmailsRepo::new(conn);
        match normalized_email.as_deref() {
            Some(email) => {
                let source = update.email_source.as_deref().or(Some("primary"));
                emails.add_email(now_utc, &contact.id, email, source, true)?;
                emails.set_primary(&contact.id, Some(email))?;
            }
            None => {
                emails.clear_emails(&contact.id)?;
            }
        }
    }

    Ok(contact)
}

fn update_with_email_ops_inner(
    conn: &Connection,
    now_utc: i64,
    id: ContactId,
    update: ContactUpdate,
    email_ops: EmailOps,
) -> Result<Contact> {
    let update_empty = update_is_empty(&update);
    let mut contact = if update_empty {
        get_inner(conn, id)?.ok_or_else(|| StoreError::NotFound(id.to_string()))?
    } else {
        update_inner(conn, now_utc, id, update)?
    };

    let emails_repo = crate::repo::emails::EmailsRepo::new(conn);
    match email_ops {
        EmailOps::None => {}
        EmailOps::Replace {
            emails,
            primary,
            source,
        } => {
            emails_repo.replace_emails_in_tx(
                now_utc,
                &contact.id,
                emails,
                primary,
                source.as_deref(),
            )?;
        }
        EmailOps::Mutate {
            clear,
            add,
            remove,
            source,
        } => {
            if clear {
                emails_repo.clear_emails(&contact.id)?;
            }
            for email in add {
                emails_repo.add_email(now_utc, &contact.id, &email, source.as_deref(), true)?;
            }
            for email in remove {
                emails_repo.remove_email(&contact.id, &email)?;
            }
        }
    }

    if update_empty {
        conn.execute(
            "UPDATE contacts SET updated_at = ?2 WHERE id = ?1;",
            params![contact.id.to_string(), now_utc],
        )?;
    }

    if let Some(updated) = get_inner(conn, id)? {
        contact = updated;
    }
    Ok(contact)
}

fn get_inner(conn: &Connection, id: ContactId) -> Result<Option<Contact>> {
    let mut stmt = conn.prepare(
        "SELECT id, display_name, email, phone, handle, timezone, next_touchpoint_at, cadence_days, created_at, updated_at, archived_at
         FROM contacts WHERE id = ?1;",
    )?;
    let mut rows = stmt.query([id.to_string()])?;
    if let Some(row) = rows.next()? {
        Ok(Some(contact_from_row(row)?))
    } else {
        Ok(None)
    }
}

fn update_is_empty(update: &ContactUpdate) -> bool {
    update.display_name.is_none()
        && update.email.is_none()
        && update.phone.is_none()
        && update.handle.is_none()
        && update.timezone.is_none()
        && update.next_touchpoint_at.is_none()
        && update.cadence_days.is_none()
        && update.archived_at.is_none()
}

fn delete_inner(conn: &Connection, now_utc: i64, id: ContactId) -> Result<()> {
    let open_status = MergeCandidateStatus::Open.as_str();
    let dismissed_status = MergeCandidateStatus::Dismissed.as_str();
    let id_key = id.to_string();
    conn.execute(
        "UPDATE contact_merge_candidates
         SET status = ?2, resolved_at = ?3
         WHERE status = ?1
           AND (contact_a_id = ?4 OR contact_b_id = ?4);",
        params![open_status, dismissed_status, now_utc, id_key],
    )?;
    conn.execute("DELETE FROM contacts WHERE id = ?1;", [id.to_string()])?;
    Ok(())
}

fn merge_contacts_inner(
    conn: &Connection,
    now_utc: i64,
    primary_id: ContactId,
    secondary_id: ContactId,
    options: ContactMergeOptions,
) -> Result<Contact> {
    if primary_id == secondary_id {
        return Err(StoreError::InvalidMerge(
            "merge requires distinct contact IDs".to_string(),
        ));
    }

    let primary =
        get_inner(conn, primary_id)?.ok_or_else(|| StoreError::NotFound(primary_id.to_string()))?;
    let secondary = get_inner(conn, secondary_id)?
        .ok_or_else(|| StoreError::NotFound(secondary_id.to_string()))?;

    let prefer_secondary = matches!(options.prefer, MergePreference::Secondary);
    let merged = merge_contact_fields(now_utc, &primary, &secondary, options);
    merged.validate()?;

    conn.execute(
        "UPDATE contacts
         SET display_name = ?2,
             phone = ?3,
             handle = ?4,
             timezone = ?5,
             next_touchpoint_at = ?6,
             cadence_days = ?7,
             updated_at = ?8,
             archived_at = ?9
         WHERE id = ?1;",
        params![
            primary_id.to_string(),
            merged.display_name,
            merged.phone,
            merged.handle,
            merged.timezone,
            merged.next_touchpoint_at,
            merged.cadence_days,
            merged.updated_at,
            merged.archived_at,
        ],
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO contact_tags (contact_id, tag_id)\n         SELECT ?1, tag_id FROM contact_tags WHERE contact_id = ?2;",
        params![primary_id.to_string(), secondary_id.to_string()],
    )?;

    conn.execute(
        "UPDATE interactions SET contact_id = ?1 WHERE contact_id = ?2;",
        params![primary_id.to_string(), secondary_id.to_string()],
    )?;

    conn.execute(
        "UPDATE email_messages SET contact_id = ?1 WHERE contact_id = ?2;",
        params![primary_id.to_string(), secondary_id.to_string()],
    )?;

    conn.execute(
        "DELETE FROM contact_telegram_accounts
         WHERE contact_id = ?1
           AND telegram_user_id IN (
             SELECT telegram_user_id FROM contact_telegram_accounts WHERE contact_id = ?2
           );",
        params![secondary_id.to_string(), primary_id.to_string()],
    )?;
    conn.execute(
        "UPDATE contact_telegram_accounts SET contact_id = ?1 WHERE contact_id = ?2;",
        params![primary_id.to_string(), secondary_id.to_string()],
    )?;

    conn.execute(
        "UPDATE telegram_messages SET contact_id = ?1 WHERE contact_id = ?2;",
        params![primary_id.to_string(), secondary_id.to_string()],
    )?;

    conn.execute(
        "UPDATE contact_dates
         SET year = (
             SELECT d2.year FROM contact_dates d2
             WHERE d2.contact_id = ?2
               AND d2.kind = contact_dates.kind
               AND d2.label = contact_dates.label
               AND d2.month = contact_dates.month
               AND d2.day = contact_dates.day
         ),
             updated_at = ?3
         WHERE contact_id = ?1
           AND year IS NULL
           AND EXISTS (
             SELECT 1 FROM contact_dates d2
             WHERE d2.contact_id = ?2
               AND d2.kind = contact_dates.kind
               AND d2.label = contact_dates.label
               AND d2.month = contact_dates.month
               AND d2.day = contact_dates.day
               AND d2.year IS NOT NULL
           );",
        params![primary_id.to_string(), secondary_id.to_string(), now_utc],
    )?;

    conn.execute(
        "DELETE FROM contact_dates
         WHERE contact_id = ?1
           AND EXISTS (
             SELECT 1 FROM contact_dates d2
             WHERE d2.contact_id = ?2
               AND d2.kind = contact_dates.kind
               AND d2.label = contact_dates.label
               AND d2.month = contact_dates.month
               AND d2.day = contact_dates.day
           );",
        params![secondary_id.to_string(), primary_id.to_string()],
    )?;
    conn.execute(
        "UPDATE contact_dates SET contact_id = ?1 WHERE contact_id = ?2;",
        params![primary_id.to_string(), secondary_id.to_string()],
    )?;

    let primary_email =
        merge_contact_emails(conn, now_utc, &primary_id, &secondary_id, prefer_secondary)?;
    crate::repo::emails::EmailsRepo::new(conn)
        .set_primary(&primary_id, primary_email.as_deref())?;
    conn.execute(
        "UPDATE contacts SET email = ?2 WHERE id = ?1;",
        params![primary_id.to_string(), primary_email],
    )?;

    let open_status = MergeCandidateStatus::Open.as_str();
    let merged_status = MergeCandidateStatus::Merged.as_str();
    let dismissed_status = MergeCandidateStatus::Dismissed.as_str();
    let primary_key = primary_id.to_string();
    let secondary_key = secondary_id.to_string();
    conn.execute(
        "UPDATE contact_merge_candidates
         SET status = ?3, resolved_at = ?4
         WHERE status = ?1
           AND ((contact_a_id = ?2 AND contact_b_id = ?5)
             OR (contact_a_id = ?5 AND contact_b_id = ?2));",
        params![
            open_status,
            primary_key,
            merged_status,
            now_utc,
            secondary_key,
        ],
    )?;
    conn.execute(
        "UPDATE contact_merge_candidates
         SET status = ?2, resolved_at = ?3
         WHERE status = ?1
           AND (contact_a_id = ?4 OR contact_b_id = ?4);",
        params![open_status, dismissed_status, now_utc, secondary_key],
    )?;

    conn.execute(
        "DELETE FROM contacts WHERE id = ?1;",
        [secondary_id.to_string()],
    )?;

    get_inner(conn, primary_id)?.ok_or_else(|| StoreError::NotFound(primary_id.to_string()))
}

fn merge_contact_fields(
    now_utc: i64,
    primary: &Contact,
    secondary: &Contact,
    options: ContactMergeOptions,
) -> Contact {
    let prefer_secondary = matches!(options.prefer, MergePreference::Secondary);
    let display_name = if prefer_secondary {
        secondary.display_name.clone()
    } else {
        primary.display_name.clone()
    };
    let phone = choose_optional(
        primary.phone.clone(),
        secondary.phone.clone(),
        prefer_secondary,
    );
    let handle = choose_optional(
        primary.handle.clone(),
        secondary.handle.clone(),
        prefer_secondary,
    );
    let timezone = choose_optional(
        primary.timezone.clone(),
        secondary.timezone.clone(),
        prefer_secondary,
    );
    let cadence_days = choose_optional(
        primary.cadence_days,
        secondary.cadence_days,
        prefer_secondary,
    );

    let next_touchpoint_at = match options.touchpoint {
        MergeTouchpointPreference::Primary => primary.next_touchpoint_at,
        MergeTouchpointPreference::Secondary => secondary.next_touchpoint_at,
        MergeTouchpointPreference::Earliest => {
            match (primary.next_touchpoint_at, secondary.next_touchpoint_at) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            }
        }
        MergeTouchpointPreference::Latest => {
            match (primary.next_touchpoint_at, secondary.next_touchpoint_at) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => None,
            }
        }
    };

    let archived_at = match options.archived {
        MergeArchivedPreference::Primary => primary.archived_at,
        MergeArchivedPreference::Secondary => secondary.archived_at,
        MergeArchivedPreference::ActiveIfAny => {
            match (primary.archived_at, secondary.archived_at) {
                (None, _) | (_, None) => None,
                (Some(a), Some(b)) => Some(a.min(b)),
            }
        }
    };

    Contact {
        id: primary.id,
        display_name,
        email: primary.email.clone(),
        phone,
        handle,
        timezone,
        next_touchpoint_at,
        cadence_days,
        created_at: primary.created_at,
        updated_at: now_utc,
        archived_at,
    }
}

fn choose_optional<T: Clone>(
    primary: Option<T>,
    secondary: Option<T>,
    prefer_secondary: bool,
) -> Option<T> {
    if prefer_secondary {
        secondary.or(primary)
    } else {
        primary.or(secondary)
    }
}

fn merge_contact_emails(
    conn: &Connection,
    now_utc: i64,
    primary_id: &ContactId,
    secondary_id: &ContactId,
    prefer_secondary: bool,
) -> Result<Option<String>> {
    let emails_repo = crate::repo::emails::EmailsRepo::new(conn);
    let primary_emails = emails_repo.list_for_contact(primary_id)?;
    let secondary_emails = emails_repo.list_for_contact(secondary_id)?;

    let primary_primary = primary_emails
        .iter()
        .find(|email| email.is_primary)
        .map(|email| email.email.clone());
    let secondary_primary = secondary_emails
        .iter()
        .find(|email| email.is_primary)
        .map(|email| email.email.clone());

    let primary_email = if prefer_secondary {
        secondary_primary
            .clone()
            .or_else(|| primary_primary.clone())
            .or_else(|| secondary_emails.first().map(|email| email.email.clone()))
            .or_else(|| primary_emails.first().map(|email| email.email.clone()))
    } else {
        primary_primary
            .clone()
            .or_else(|| secondary_primary.clone())
            .or_else(|| primary_emails.first().map(|email| email.email.clone()))
            .or_else(|| secondary_emails.first().map(|email| email.email.clone()))
    };

    let mut primary_map = std::collections::HashMap::new();
    for email in &primary_emails {
        primary_map.insert(email.email.clone(), email.clone());
    }

    for secondary in &secondary_emails {
        if let Some(existing) = primary_map.get(&secondary.email) {
            let merged_source =
                merge_email_source(existing.source.clone(), secondary.source.clone());
            let merged_created_at = existing.created_at.min(secondary.created_at);
            conn.execute(
                "UPDATE contact_emails
                 SET source = ?3, created_at = ?4
                 WHERE contact_id = ?1 AND email = ?2;",
                params![
                    primary_id.to_string(),
                    secondary.email,
                    merged_source,
                    merged_created_at,
                ],
            )?;
            conn.execute(
                "DELETE FROM contact_emails WHERE contact_id = ?1 AND email = ?2;",
                params![secondary_id.to_string(), secondary.email],
            )?;
        } else {
            conn.execute(
                "UPDATE contact_emails
                 SET contact_id = ?1, is_primary = 0
                 WHERE contact_id = ?2 AND email = ?3;",
                params![
                    primary_id.to_string(),
                    secondary_id.to_string(),
                    secondary.email,
                ],
            )?;
        }
    }

    if primary_emails.is_empty() && secondary_emails.is_empty() {
        return Ok(None);
    }

    if let Some(primary_email) = primary_email.as_deref() {
        emails_repo.add_email(now_utc, primary_id, primary_email, Some("primary"), true)?;
    }

    Ok(primary_email)
}

fn create_with_emails_and_tags_inner(
    conn: &Connection,
    now_utc: i64,
    mut input: ContactNew,
    tags: Vec<TagName>,
    emails: Vec<String>,
    source: Option<&str>,
) -> Result<Contact> {
    let mut normalized = normalize_emails(emails);
    let primary = input
        .email
        .as_deref()
        .and_then(normalize_email)
        .or_else(|| normalized.first().cloned());
    input.email = primary.clone();
    let contact = create_inner(conn, now_utc, input)?;
    let emails_repo = crate::repo::emails::EmailsRepo::new(conn);
    if let Some(primary) = primary.as_deref() {
        emails_repo.add_email(
            now_utc,
            &contact.id,
            primary,
            source.or(Some("primary")),
            true,
        )?;
    }
    if !normalized.is_empty() {
        if let Some(primary) = primary.as_deref() {
            normalized.retain(|email| email != primary);
        }
        for email in &normalized {
            emails_repo.add_email(now_utc, &contact.id, email, source, false)?;
        }
    }
    if let Some(primary) = primary.as_deref() {
        emails_repo.set_primary(&contact.id, Some(primary))?;
    }
    if !tags.is_empty() {
        crate::repo::tags::set_contact_tags_inner(conn, &contact.id.to_string(), tags)?;
    }
    Ok(contact)
}

fn merge_email_source(primary: Option<String>, secondary: Option<String>) -> Option<String> {
    match (primary.as_deref(), secondary.as_deref()) {
        (None, Some(value)) => Some(value.to_string()),
        (Some("primary"), Some(value)) => Some(value.to_string()),
        _ => primary,
    }
}

fn normalize_emails(emails: Vec<String>) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::new();
    for email in emails {
        if let Some(email) = normalize_email(&email) {
            if !normalized.contains(&email) {
                normalized.push(email);
            }
        }
    }
    normalized
}

fn contact_from_row(row: &rusqlite::Row<'_>) -> Result<Contact> {
    let id_str: String = row.get(0)?;
    let id = ContactId::from_str(&id_str).map_err(|_| StoreError::InvalidId(id_str.clone()))?;
    Ok(Contact {
        id,
        display_name: row.get(1)?,
        email: row.get(2)?,
        phone: row.get(3)?,
        handle: row.get(4)?,
        timezone: row.get(5)?,
        next_touchpoint_at: row.get(6)?,
        cadence_days: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        archived_at: row.get(10)?,
    })
}
