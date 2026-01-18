use assert_cmd::cargo::cargo_bin_cmd;
use knotter_core::rules::MAX_SOON_DAYS;
use knotter_store::Store;
use serde_json::Value;
use std::path::Path;
use tempfile::TempDir;

fn run_cmd(db_path: &Path, args: &[&str]) -> String {
    let output = cargo_bin_cmd!("knotter")
        .args(["--db-path", db_path.to_str().expect("db path")])
        .args(args)
        .output()
        .expect("run command");
    assert!(output.status.success(), "command failed: {:?}", output);
    String::from_utf8(output.stdout).expect("utf8")
}

fn run_cmd_output(db_path: &Path, args: &[&str]) -> std::process::Output {
    cargo_bin_cmd!("knotter")
        .args(["--db-path", db_path.to_str().expect("db path")])
        .args(args)
        .output()
        .expect("run command")
}

fn run_cmd_json(db_path: &Path, args: &[&str]) -> Value {
    let output = cargo_bin_cmd!("knotter")
        .args(["--db-path", db_path.to_str().expect("db path"), "--json"])
        .args(args)
        .output()
        .expect("run command");
    assert!(output.status.success(), "command failed: {:?}", output);
    serde_json::from_slice(&output.stdout).expect("parse json")
}

#[test]
fn cli_add_list_tag_schedule_flow() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);

    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["display_name"], "Ada Lovelace");
    let id = items[0]["id"].as_str().expect("id").to_string();

    run_cmd(&db_path, &["tag", "add", &id, "friend"]);

    let filtered = run_cmd_json(&db_path, &["list", "--filter", "#friend"]);
    let filtered_items = filtered.as_array().expect("array");
    assert_eq!(filtered_items.len(), 1);

    run_cmd(&db_path, &["schedule", &id, "--at", "2030-01-01"]);

    let detail = run_cmd_json(&db_path, &["show", &id]);
    assert!(detail["next_touchpoint_at"].is_number());
}

#[test]
fn cli_remind_includes_soon_contact() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);

    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();

    let scheduled = "2030-01-02";
    run_cmd(&db_path, &["schedule", &id, "--at", scheduled]);

    let remind = run_cmd_json(
        &db_path,
        &["remind", "--soon-days", &MAX_SOON_DAYS.to_string()],
    );
    let soon = remind["soon"].as_array().expect("soon array");
    assert_eq!(soon.len(), 1);
    assert_eq!(soon[0]["id"], id);
}

#[test]
fn cli_remind_notify_json_fails_without_desktop_feature() {
    if cfg!(feature = "desktop-notify") {
        return;
    }

    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);

    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();

    run_cmd(&db_path, &["schedule", &id, "--at", "2030-01-02"]);

    let output = run_cmd_output(
        &db_path,
        &[
            "--json",
            "remind",
            "--notify",
            "--soon-days",
            &MAX_SOON_DAYS.to_string(),
        ],
    );
    assert!(!output.status.success());
    let parsed: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    let soon = parsed["soon"].as_array().expect("soon array");
    assert_eq!(soon.len(), 1);
}

#[test]
fn cli_import_vcf_creates_contact() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let vcf_path = temp.path().join("contacts.vcf");

    let vcf = "BEGIN:VCARD\nVERSION:3.0\nFN:Grace Hopper\nEMAIL:grace@example.com\nCATEGORIES:friends\nEND:VCARD\n";
    std::fs::write(&vcf_path, vcf).expect("write vcf");

    run_cmd(
        &db_path,
        &["import", "vcf", vcf_path.to_str().expect("path")],
    );

    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["display_name"], "Grace Hopper");
}

#[test]
fn cli_export_vcf_writes_file() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let out_path = temp.path().join("export.vcf");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);

    run_cmd(
        &db_path,
        &["export", "vcf", "--out", out_path.to_str().expect("path")],
    );

    let contents = std::fs::read_to_string(&out_path).expect("read vcf");
    assert!(contents.contains("BEGIN:VCARD"));
    assert!(contents.contains("FN:Ada Lovelace"));
}

#[test]
fn cli_export_ics_writes_file() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let out_path = temp.path().join("export.ics");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);
    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();
    run_cmd(&db_path, &["schedule", &id, "--at", "2030-01-01"]);

    run_cmd(
        &db_path,
        &["export", "ics", "--out", out_path.to_str().expect("path")],
    );

    let contents = std::fs::read_to_string(&out_path).expect("read ics");
    assert!(contents.contains("BEGIN:VEVENT"));
    assert!(contents.contains("SUMMARY:Reach out to Ada Lovelace"));
}

#[test]
fn cli_backup_writes_file() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let backup_path = temp.path().join("backup.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);
    run_cmd(
        &db_path,
        &["backup", "--out", backup_path.to_str().expect("path")],
    );

    assert!(backup_path.exists());
    let backup = Store::open(&backup_path).expect("open backup");
    backup.migrate().expect("migrate backup");
    let contacts = backup.contacts().list_all().expect("list contacts");
    assert_eq!(contacts.len(), 1);
}

#[test]
fn cli_backup_rejects_db_path() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);

    let output = run_cmd_output(
        &db_path,
        &["backup", "--out", db_path.to_str().expect("path")],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("backup path"));
}

#[test]
fn cli_import_vcf_skips_duplicate_email() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let vcf_path = temp.path().join("contacts.vcf");

    run_cmd(
        &db_path,
        &[
            "add-contact",
            "--name",
            "First",
            "--email",
            "dup@example.com",
        ],
    );
    run_cmd(
        &db_path,
        &[
            "add-contact",
            "--name",
            "Second",
            "--email",
            "dup@example.com",
        ],
    );

    let vcf = "BEGIN:VCARD\nVERSION:3.0\nFN:Updated Name\nEMAIL:dup@example.com\nEND:VCARD\n";
    std::fs::write(&vcf_path, vcf).expect("write vcf");

    let report = run_cmd_json(
        &db_path,
        &["import", "vcf", vcf_path.to_str().expect("path")],
    );
    assert_eq!(report["updated"], 0);
    assert_eq!(report["skipped"], 1);

    let list = run_cmd_json(&db_path, &["list"]);
    let names: Vec<String> = list
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item["display_name"].as_str().expect("name").to_string())
        .collect();
    assert!(names.contains(&"First".to_string()));
    assert!(names.contains(&"Second".to_string()));
}
