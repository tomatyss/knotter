use knotter_store::Store;

#[test]
fn migrations_apply_once() {
    let store = Store::open_in_memory().expect("open in memory");
    store.migrate().expect("migrate");
    store.migrate().expect("migrate again");

    let version: i64 = store
        .connection()
        .query_row("SELECT version FROM knotter_schema LIMIT 1;", [], |row| {
            row.get(0)
        })
        .expect("schema version");
    assert_eq!(version, 8);
}
