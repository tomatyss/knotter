use knotter_store::error::StoreError;
use knotter_store::repo::contacts::ContactNew;
use knotter_store::Store;
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn backup_creates_readable_snapshot() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let backup_path = temp.path().join("backup.sqlite3");

    let store = Store::open(&db_path).expect("open store");
    store.migrate().expect("migrate");

    let now_utc = 1_700_000_000;
    store
        .contacts()
        .create(
            now_utc,
            ContactNew {
                display_name: "Ada Lovelace".to_string(),
                email: Some("ada@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact");

    store.backup_to(&backup_path).expect("backup");
    assert!(backup_path.exists());

    let backup = Store::open(&backup_path).expect("open backup");
    let contacts = backup.contacts().list_all().expect("list contacts");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0].display_name, "Ada Lovelace");
}

#[test]
fn backup_rejects_database_path() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let store = Store::open(&db_path).expect("open store");
    store.migrate().expect("migrate");

    let err = store.backup_to(&db_path).expect_err("backup should fail");
    assert!(matches!(err, StoreError::InvalidBackupPath(_)));
}

#[test]
fn backup_rejects_sidecar_paths() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let store = Store::open(&db_path).expect("open store");
    store.migrate().expect("migrate");

    let wal_path = PathBuf::from(format!("{}-wal", db_path.display()));
    let shm_path = PathBuf::from(format!("{}-shm", db_path.display()));

    let err = store.backup_to(&wal_path).expect_err("backup should fail");
    assert!(matches!(err, StoreError::InvalidBackupPath(_)));

    let err = store.backup_to(&shm_path).expect_err("backup should fail");
    assert!(matches!(err, StoreError::InvalidBackupPath(_)));
}

#[cfg(unix)]
#[test]
fn backup_rejects_hardlink_paths() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let link_path = temp.path().join("knotter-link.sqlite3");
    let store = Store::open(&db_path).expect("open store");
    store.migrate().expect("migrate");

    std::fs::hard_link(&db_path, &link_path).expect("hard link");
    let err = store.backup_to(&link_path).expect_err("backup should fail");
    assert!(matches!(err, StoreError::InvalidBackupPath(_)));
}
