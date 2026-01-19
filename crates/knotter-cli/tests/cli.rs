use assert_cmd::cargo::cargo_bin_cmd;
use chrono::{Duration, Local};
use knotter_core::rules::MAX_SOON_DAYS;
use knotter_store::repo::ContactUpdate;
use knotter_store::Store;
use serde_json::Value;
use std::path::Path;
use std::str::FromStr;
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

fn run_cmd_with_config(db_path: &Path, config_path: &Path, args: &[&str]) -> String {
    let output = cargo_bin_cmd!("knotter")
        .args([
            "--db-path",
            db_path.to_str().expect("db path"),
            "--config",
            config_path.to_str().expect("config path"),
        ])
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

fn run_cmd_json_with_config(db_path: &Path, config_path: &Path, args: &[&str]) -> Value {
    let output = cargo_bin_cmd!("knotter")
        .args([
            "--db-path",
            db_path.to_str().expect("db path"),
            "--config",
            config_path.to_str().expect("config path"),
            "--json",
        ])
        .args(args)
        .output()
        .expect("run command");
    assert!(output.status.success(), "command failed: {:?}", output);
    serde_json::from_slice(&output.stdout).expect("parse json")
}

fn restrict_config_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)
            .expect("config metadata")
            .permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms).expect("chmod config");
    }
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
fn cli_remind_uses_config_due_soon_days() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(&config_path, "due_soon_days = 0\n").expect("write config");
    restrict_config_permissions(&config_path);

    run_cmd_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace"],
    );

    let list = run_cmd_json_with_config(&db_path, &config_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();

    let tomorrow = Local::now()
        .date_naive()
        .checked_add_signed(Duration::days(1))
        .expect("tomorrow");
    let scheduled = tomorrow.format("%Y-%m-%d").to_string();
    run_cmd_with_config(
        &db_path,
        &config_path,
        &["schedule", &id, "--at", &scheduled],
    );

    let remind = run_cmd_json_with_config(&db_path, &config_path, &["remind"]);
    assert!(remind["overdue"].as_array().expect("overdue").is_empty());
    assert!(remind["today"].as_array().expect("today").is_empty());
    assert!(remind["soon"].as_array().expect("soon").is_empty());
}

#[test]
fn cli_remind_no_notify_overrides_config() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        "due_soon_days = 3650\n[notifications]\nenabled = true\nbackend = \"desktop\"\n",
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    run_cmd_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace"],
    );

    let list = run_cmd_json_with_config(&db_path, &config_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();
    run_cmd_with_config(
        &db_path,
        &config_path,
        &["schedule", &id, "--at", "2030-01-02"],
    );

    let output = run_cmd_with_config(&db_path, &config_path, &["remind", "--no-notify"]);
    assert!(output.contains("soon:"));
    assert!(output.contains("Ada Lovelace"));
}

#[test]
fn cli_remind_config_stdout_backend_prints_full_list() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        "due_soon_days = 3650\n[notifications]\nenabled = true\nbackend = \"stdout\"\n",
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    run_cmd_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace"],
    );

    let list = run_cmd_json_with_config(&db_path, &config_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();
    run_cmd_with_config(
        &db_path,
        &config_path,
        &["schedule", &id, "--at", "2030-01-02"],
    );

    let output = run_cmd_with_config(&db_path, &config_path, &["remind"]);
    assert!(output.contains("soon:"));
    assert!(output.contains("Ada Lovelace"));
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
fn cli_export_json_outputs_snapshot() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);
    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();

    run_cmd(&db_path, &["tag", "add", &id, "friend"]);
    run_cmd(
        &db_path,
        &[
            "add-note",
            &id,
            "--kind",
            "call",
            "--note",
            "hello",
            "--when",
            "2030-01-02",
        ],
    );

    let output = run_cmd_output(&db_path, &["export", "json"]);
    assert!(output.status.success(), "command failed: {:?}", output);
    let snapshot: Value = serde_json::from_slice(&output.stdout).expect("parse json");

    assert!(snapshot["metadata"]["exported_at"].is_number());
    assert_eq!(snapshot["metadata"]["format_version"], 1);

    let contacts = snapshot["contacts"].as_array().expect("contacts array");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0]["display_name"], "Ada Lovelace");

    let tags = contacts[0]["tags"].as_array().expect("tags array");
    assert_eq!(tags.len(), 1);
    assert_eq!(tags[0], "friend");

    let interactions = contacts[0]["interactions"]
        .as_array()
        .expect("interactions array");
    assert_eq!(interactions.len(), 1);
    assert_eq!(interactions[0]["kind"], "call");
    assert_eq!(interactions[0]["note"], "hello");
}

#[test]
fn cli_export_json_excludes_archived_when_requested() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Active"]);
    run_cmd(&db_path, &["add-contact", "--name", "Archived"]);

    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    let mut active_id = None;
    let mut archived_id = None;
    for item in items {
        match item["display_name"].as_str().expect("name") {
            "Active" => active_id = item["id"].as_str().map(|id| id.to_string()),
            "Archived" => archived_id = item["id"].as_str().map(|id| id.to_string()),
            _ => {}
        }
    }
    let active_id = active_id.expect("active id");
    let archived_id = archived_id.expect("archived id");

    let store = Store::open(&db_path).expect("open store");
    let now = 1_700_000_000;
    store
        .contacts()
        .update(
            now,
            knotter_core::domain::ContactId::from_str(&archived_id).expect("contact id"),
            ContactUpdate {
                archived_at: Some(Some(now)),
                ..Default::default()
            },
        )
        .expect("archive contact");

    let output = run_cmd_output(&db_path, &["export", "json", "--exclude-archived"]);
    assert!(output.status.success(), "command failed: {:?}", output);
    let snapshot: Value = serde_json::from_slice(&output.stdout).expect("parse json");
    let contacts = snapshot["contacts"].as_array().expect("contacts array");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0]["id"], active_id);
}

#[test]
fn cli_export_json_with_out_and_json_emits_report() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let out_path = temp.path().join("export.json");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);

    let output = run_cmd_output(
        &db_path,
        &[
            "--json",
            "export",
            "json",
            "--out",
            out_path.to_str().expect("path"),
        ],
    );
    assert!(output.status.success(), "command failed: {:?}", output);

    let report: Value = serde_json::from_slice(&output.stdout).expect("parse json report");
    assert_eq!(report["format"], "json");
    assert_eq!(report["count"], 1);
    assert_eq!(report["output"], out_path.to_str().expect("path"));

    let snapshot: Value = serde_json::from_slice(&std::fs::read(&out_path).expect("read snapshot"))
        .expect("parse snapshot");
    let contacts = snapshot["contacts"].as_array().expect("contacts array");
    assert_eq!(contacts.len(), 1);
    assert_eq!(contacts[0]["display_name"], "Ada Lovelace");
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
