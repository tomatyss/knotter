use crate::error::{Result, StoreError};
use knotter_core::domain::{ContactId, Tag, TagId, TagName};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::str::FromStr;

struct TempTableGuard<'a> {
    conn: &'a Connection,
    name: &'static str,
}

impl<'a> TempTableGuard<'a> {
    fn new(conn: &'a Connection, name: &'static str) -> Self {
        Self { conn, name }
    }
}

impl Drop for TempTableGuard<'_> {
    fn drop(&mut self) {
        let _ = self.conn.execute(
            &format!("DROP TABLE IF EXISTS temp.{name};", name = self.name),
            [],
        );
    }
}

pub struct TagsRepo<'a> {
    conn: &'a Connection,
}

impl<'a> TagsRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn upsert(&self, name: TagName) -> Result<Tag> {
        upsert_inner(self.conn, name)
    }

    pub fn list_with_counts(&self) -> Result<Vec<(Tag, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT tags.id, tags.name, COUNT(contact_tags.contact_id) AS cnt
             FROM tags
             LEFT JOIN contact_tags ON tags.id = contact_tags.tag_id
             GROUP BY tags.id, tags.name
             ORDER BY tags.name ASC;",
        )?;
        let mut rows = stmt.query([])?;
        let mut items = Vec::new();
        while let Some(row) = rows.next()? {
            let tag = tag_from_row(row)?;
            let count: i64 = row.get(2)?;
            items.push((tag, count));
        }
        Ok(items)
    }

    pub fn list_for_contact(&self, contact_id: &str) -> Result<Vec<Tag>> {
        let mut stmt = self.conn.prepare(
            "SELECT tags.id, tags.name
             FROM tags
             INNER JOIN contact_tags ON tags.id = contact_tags.tag_id
             WHERE contact_tags.contact_id = ?1
             ORDER BY tags.name ASC;",
        )?;
        let mut rows = stmt.query([contact_id])?;
        let mut tags = Vec::new();
        while let Some(row) = rows.next()? {
            tags.push(tag_from_row(row)?);
        }
        Ok(tags)
    }

    pub fn list_names_for_contacts(
        &self,
        contact_ids: &[ContactId],
    ) -> Result<HashMap<ContactId, Vec<String>>> {
        let mut map: HashMap<ContactId, Vec<String>> = HashMap::new();
        if contact_ids.is_empty() {
            return Ok(map);
        }

        const TEMP_TABLE: &str = "temp_contact_ids";
        let _guard = TempTableGuard::new(self.conn, TEMP_TABLE);
        self.conn.execute_batch(
            "DROP TABLE IF EXISTS temp.temp_contact_ids;
             CREATE TEMP TABLE temp.temp_contact_ids (id TEXT PRIMARY KEY);",
        )?;

        {
            let mut insert_stmt = self
                .conn
                .prepare("INSERT OR IGNORE INTO temp.temp_contact_ids (id) VALUES (?1);")?;
            for id in contact_ids {
                insert_stmt.execute([id.to_string()])?;
            }
        }

        {
            let mut stmt = self.conn.prepare(
                "SELECT ct.contact_id, t.name
                 FROM contact_tags ct
                 INNER JOIN tags t ON t.id = ct.tag_id
                 INNER JOIN temp.temp_contact_ids tmp ON tmp.id = ct.contact_id
                 ORDER BY t.name ASC;",
            )?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let contact_id_raw: String = row.get(0)?;
                let contact_id = ContactId::from_str(&contact_id_raw)
                    .map_err(|_| StoreError::InvalidId(contact_id_raw.clone()))?;
                let tag_name: String = row.get(1)?;
                map.entry(contact_id).or_default().push(tag_name);
            }
        }

        Ok(map)
    }

    pub fn add_tag_to_contact(&self, contact_id: &str, tag: TagName) -> Result<()> {
        let tag = self.upsert(tag)?;
        self.conn.execute(
            "INSERT OR IGNORE INTO contact_tags (contact_id, tag_id) VALUES (?1, ?2);",
            params![contact_id, tag.id.to_string()],
        )?;
        Ok(())
    }

    pub fn remove_tag_from_contact(&self, contact_id: &str, tag: TagName) -> Result<()> {
        let tag_row: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1;",
                [tag.as_str()],
                |row| row.get(0),
            )
            .optional()?;

        if let Some(tag_id) = tag_row {
            self.conn.execute(
                "DELETE FROM contact_tags WHERE contact_id = ?1 AND tag_id = ?2;",
                params![contact_id, tag_id],
            )?;
        }
        Ok(())
    }

    pub fn set_contact_tags(&self, contact_id: &str, tags: Vec<TagName>) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM contact_tags WHERE contact_id = ?1;",
            [contact_id],
        )?;

        for tag in tags {
            let tag = upsert_inner(&tx, tag)?;
            tx.execute(
                "INSERT OR IGNORE INTO contact_tags (contact_id, tag_id) VALUES (?1, ?2);",
                params![contact_id, tag.id.to_string()],
            )?;
        }

        tx.commit()?;
        Ok(())
    }
}

fn upsert_inner(conn: &Connection, name: TagName) -> Result<Tag> {
    let tag_name = name.as_str().to_string();
    let new_id = TagId::new();

    conn.execute(
        "INSERT INTO tags (id, name) VALUES (?1, ?2) ON CONFLICT(name) DO NOTHING;",
        params![new_id.to_string(), tag_name],
    )?;

    let mut stmt = conn.prepare("SELECT id, name FROM tags WHERE name = ?1;")?;
    let mut rows = stmt.query([name.as_str()])?;
    if let Some(row) = rows.next()? {
        tag_from_row(row)
    } else {
        Err(StoreError::Migration(
            "missing tag after upsert".to_string(),
        ))
    }
}

fn tag_from_row(row: &rusqlite::Row<'_>) -> Result<Tag> {
    let id_str: String = row.get(0)?;
    let id = TagId::from_str(&id_str).map_err(|_| StoreError::InvalidId(id_str.clone()))?;
    let name_raw: String = row.get(1)?;
    let name = TagName::new(&name_raw)?;
    Ok(Tag { id, name })
}
