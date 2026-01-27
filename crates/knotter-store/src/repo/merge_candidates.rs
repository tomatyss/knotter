use crate::error::{Result, StoreError};
use knotter_core::domain::{ContactId, MergeCandidateId};
use rusqlite::{params, Connection, ErrorCode};
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeCandidateStatus {
    Open,
    Merged,
    Dismissed,
}

impl MergeCandidateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MergeCandidateStatus::Open => "open",
            MergeCandidateStatus::Merged => "merged",
            MergeCandidateStatus::Dismissed => "dismissed",
        }
    }
}

impl std::str::FromStr for MergeCandidateStatus {
    type Err = StoreError;

    fn from_str(value: &str) -> Result<Self> {
        match value {
            "open" => Ok(MergeCandidateStatus::Open),
            "merged" => Ok(MergeCandidateStatus::Merged),
            "dismissed" => Ok(MergeCandidateStatus::Dismissed),
            _ => Err(StoreError::InvalidMerge(format!(
                "unknown merge status {value}"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MergeCandidate {
    pub id: MergeCandidateId,
    pub created_at: i64,
    pub status: MergeCandidateStatus,
    pub reason: String,
    pub source: Option<String>,
    pub contact_a_id: ContactId,
    pub contact_b_id: ContactId,
    pub preferred_contact_id: Option<ContactId>,
    pub resolved_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MergeCandidateCreate {
    pub reason: String,
    pub source: Option<String>,
    pub preferred_contact_id: Option<ContactId>,
}

pub struct MergeCandidateCreateResult {
    pub candidate: MergeCandidate,
    pub created: bool,
}

pub struct MergeCandidatesRepo<'a> {
    conn: &'a Connection,
}

impl<'a> MergeCandidatesRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn list(&self, status: Option<MergeCandidateStatus>) -> Result<Vec<MergeCandidate>> {
        let mut candidates = Vec::new();
        let mut stmt = match status {
            Some(_) => self.conn.prepare(
                "SELECT id, created_at, status, reason, source, contact_a_id, contact_b_id, preferred_contact_id, resolved_at
                 FROM contact_merge_candidates
                 WHERE status = ?1
                 ORDER BY created_at DESC;",
            )?,
            None => self.conn.prepare(
                "SELECT id, created_at, status, reason, source, contact_a_id, contact_b_id, preferred_contact_id, resolved_at
                 FROM contact_merge_candidates
                 ORDER BY created_at DESC;",
            )?,
        };

        let mut rows = match status {
            Some(status) => stmt.query([status.as_str()])?,
            None => stmt.query([])?,
        };

        while let Some(row) = rows.next()? {
            candidates.push(merge_candidate_from_row(row)?);
        }
        Ok(candidates)
    }

    pub fn list_open(&self) -> Result<Vec<MergeCandidate>> {
        self.list(Some(MergeCandidateStatus::Open))
    }

    pub fn has_open_for_contact(&self, contact_id: ContactId) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT COUNT(1)
             FROM contact_merge_candidates
             WHERE status = ?1
               AND (contact_a_id = ?2 OR contact_b_id = ?2);",
        )?;
        let count: i64 = stmt.query_row(
            params![MergeCandidateStatus::Open.as_str(), contact_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn get(&self, id: MergeCandidateId) -> Result<Option<MergeCandidate>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, status, reason, source, contact_a_id, contact_b_id, preferred_contact_id, resolved_at
             FROM contact_merge_candidates
             WHERE id = ?1;",
        )?;
        let mut rows = stmt.query([id.to_string()])?;
        if let Some(row) = rows.next()? {
            Ok(Some(merge_candidate_from_row(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn create(
        &self,
        now_utc: i64,
        contact_a_id: ContactId,
        contact_b_id: ContactId,
        create: MergeCandidateCreate,
    ) -> Result<MergeCandidateCreateResult> {
        if contact_a_id == contact_b_id {
            return Err(StoreError::InvalidMerge(
                "merge candidate requires distinct contacts".to_string(),
            ));
        }

        let (contact_a_id, contact_b_id) = ordered_pair(contact_a_id, contact_b_id);
        let preferred = match create.preferred_contact_id {
            Some(id) if id == contact_a_id || id == contact_b_id => Some(id),
            _ => None,
        };

        if let Some(existing) =
            self.find_by_pair_and_status(contact_a_id, contact_b_id, MergeCandidateStatus::Open)?
        {
            return Ok(MergeCandidateCreateResult {
                candidate: existing,
                created: false,
            });
        }

        let candidate_id = MergeCandidateId::new();
        let insert_result = self.conn.execute(
            "INSERT INTO contact_merge_candidates (id, created_at, status, reason, source, contact_a_id, contact_b_id, preferred_contact_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            params![
                candidate_id.to_string(),
                now_utc,
                MergeCandidateStatus::Open.as_str(),
                create.reason,
                create.source,
                contact_a_id.to_string(),
                contact_b_id.to_string(),
                preferred.map(|id| id.to_string()),
            ],
        );
        if let Err(err) = insert_result {
            if let rusqlite::Error::SqliteFailure(ref failure, _) = err {
                if failure.code == ErrorCode::ConstraintViolation
                    && failure.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                {
                    if let Some(existing) = self.find_by_pair_and_status(
                        contact_a_id,
                        contact_b_id,
                        MergeCandidateStatus::Open,
                    )? {
                        return Ok(MergeCandidateCreateResult {
                            candidate: existing,
                            created: false,
                        });
                    }
                }
            }
            return Err(err.into());
        }

        let candidate = self
            .get(candidate_id)?
            .ok_or_else(|| StoreError::NotFound(candidate_id.to_string()))?;
        Ok(MergeCandidateCreateResult {
            candidate,
            created: true,
        })
    }

    pub fn dismiss(&self, now_utc: i64, id: MergeCandidateId) -> Result<MergeCandidate> {
        self.ensure_open(id)?;
        self.update_status(id, MergeCandidateStatus::Dismissed, Some(now_utc))
    }

    pub fn mark_merged(&self, now_utc: i64, id: MergeCandidateId) -> Result<MergeCandidate> {
        self.ensure_open(id)?;
        self.update_status(id, MergeCandidateStatus::Merged, Some(now_utc))
    }

    pub fn set_preferred(
        &self,
        id: MergeCandidateId,
        preferred_contact_id: Option<ContactId>,
    ) -> Result<MergeCandidate> {
        let candidate = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        if candidate.status != MergeCandidateStatus::Open {
            return Err(StoreError::InvalidMerge(format!(
                "merge candidate {} is not open",
                id
            )));
        }
        if let Some(preferred) = preferred_contact_id {
            if preferred != candidate.contact_a_id && preferred != candidate.contact_b_id {
                return Err(StoreError::InvalidMerge(format!(
                    "merge candidate {} does not reference preferred contact",
                    id
                )));
            }
        }
        self.conn.execute(
            "UPDATE contact_merge_candidates
             SET preferred_contact_id = ?2
             WHERE id = ?1;",
            params![
                id.to_string(),
                preferred_contact_id.map(|id| id.to_string())
            ],
        )?;
        self.get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    fn update_status(
        &self,
        id: MergeCandidateId,
        status: MergeCandidateStatus,
        resolved_at: Option<i64>,
    ) -> Result<MergeCandidate> {
        let updated = self.conn.execute(
            "UPDATE contact_merge_candidates
             SET status = ?2, resolved_at = ?3
             WHERE id = ?1;",
            params![id.to_string(), status.as_str(), resolved_at,],
        )?;
        if updated == 0 {
            return Err(StoreError::NotFound(id.to_string()));
        }
        self.get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))
    }

    fn ensure_open(&self, id: MergeCandidateId) -> Result<()> {
        let candidate = self
            .get(id)?
            .ok_or_else(|| StoreError::NotFound(id.to_string()))?;
        if candidate.status != MergeCandidateStatus::Open {
            return Err(StoreError::InvalidMerge(format!(
                "merge candidate {} is not open",
                id
            )));
        }
        Ok(())
    }

    fn find_by_pair_and_status(
        &self,
        contact_a_id: ContactId,
        contact_b_id: ContactId,
        status: MergeCandidateStatus,
    ) -> Result<Option<MergeCandidate>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, status, reason, source, contact_a_id, contact_b_id, preferred_contact_id, resolved_at
             FROM contact_merge_candidates
             WHERE contact_a_id = ?1 AND contact_b_id = ?2 AND status = ?3
             LIMIT 1;",
        )?;
        let mut rows = stmt.query([
            contact_a_id.to_string(),
            contact_b_id.to_string(),
            status.as_str().to_string(),
        ])?;
        if let Some(row) = rows.next()? {
            Ok(Some(merge_candidate_from_row(row)?))
        } else {
            Ok(None)
        }
    }
}

fn ordered_pair(a: ContactId, b: ContactId) -> (ContactId, ContactId) {
    let a_key = a.as_uuid().as_u128();
    let b_key = b.as_uuid().as_u128();
    if a_key <= b_key {
        (a, b)
    } else {
        (b, a)
    }
}

fn merge_candidate_from_row(row: &rusqlite::Row<'_>) -> Result<MergeCandidate> {
    let id_str: String = row.get(0)?;
    let id =
        MergeCandidateId::from_str(&id_str).map_err(|_| StoreError::InvalidId(id_str.clone()))?;
    let status_str: String = row.get(2)?;
    let status = MergeCandidateStatus::from_str(&status_str)?;
    let contact_a_str: String = row.get(5)?;
    let contact_b_str: String = row.get(6)?;
    let contact_a =
        ContactId::from_str(&contact_a_str).map_err(|_| StoreError::InvalidId(contact_a_str))?;
    let contact_b =
        ContactId::from_str(&contact_b_str).map_err(|_| StoreError::InvalidId(contact_b_str))?;
    let preferred_str: Option<String> = row.get(7)?;
    let preferred_contact_id = match preferred_str {
        Some(value) => Some(ContactId::from_str(&value).map_err(|_| StoreError::InvalidId(value))?),
        None => None,
    };

    Ok(MergeCandidate {
        id,
        created_at: row.get(1)?,
        status,
        reason: row.get(3)?,
        source: row.get(4)?,
        contact_a_id: contact_a,
        contact_b_id: contact_b,
        preferred_contact_id,
        resolved_at: row.get(8)?,
    })
}
