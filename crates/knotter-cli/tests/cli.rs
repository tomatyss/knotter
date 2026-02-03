use assert_cmd::cargo::cargo_bin_cmd;
use chrono::{Duration, Local, TimeZone, Utc};
use knotter_core::domain::ContactId;
use knotter_core::domain::InteractionKind;
use knotter_core::domain::MergeCandidateReason;
use knotter_core::rules::{schedule_next, MAX_SOON_DAYS};
use knotter_core::time::parse_local_timestamp;
use knotter_store::repo::ContactUpdate;
use knotter_store::repo::MergeCandidateCreate;
use knotter_store::Store;
use serde_json::Value;
use std::path::Path;
use std::str::FromStr;
use tempfile::TempDir;

fn run_cmd(db_path: &Path, args: &[&str]) -> String {
    let config_dir = TempDir::new().expect("temp config dir");
    let output = cargo_bin_cmd!("knotter")
        .env("XDG_CONFIG_HOME", config_dir.path())
        .args(["--db-path", db_path.to_str().expect("db path")])
        .args(args)
        .output()
        .expect("run command");
    assert!(output.status.success(), "command failed: {:?}", output);
    String::from_utf8(output.stdout).expect("utf8")
}

fn run_cmd_with_config(db_path: &Path, config_path: &Path, args: &[&str]) -> String {
    let config_dir = TempDir::new().expect("temp config dir");
    let output = cargo_bin_cmd!("knotter")
        .env("XDG_CONFIG_HOME", config_dir.path())
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
    let config_dir = TempDir::new().expect("temp config dir");
    cargo_bin_cmd!("knotter")
        .env("XDG_CONFIG_HOME", config_dir.path())
        .args(["--db-path", db_path.to_str().expect("db path")])
        .args(args)
        .output()
        .expect("run command")
}

fn run_cmd_output_with_config(
    db_path: &Path,
    config_path: &Path,
    args: &[&str],
) -> std::process::Output {
    let config_dir = TempDir::new().expect("temp config dir");
    cargo_bin_cmd!("knotter")
        .env("XDG_CONFIG_HOME", config_dir.path())
        .args([
            "--db-path",
            db_path.to_str().expect("db path"),
            "--config",
            config_path.to_str().expect("config path"),
        ])
        .args(args)
        .output()
        .expect("run command")
}

fn run_cmd_json(db_path: &Path, args: &[&str]) -> Value {
    let config_dir = TempDir::new().expect("temp config dir");
    let output = cargo_bin_cmd!("knotter")
        .env("XDG_CONFIG_HOME", config_dir.path())
        .args(["--db-path", db_path.to_str().expect("db path"), "--json"])
        .args(args)
        .output()
        .expect("run command");
    assert!(output.status.success(), "command failed: {:?}", output);
    serde_json::from_slice(&output.stdout).expect("parse json")
}

fn run_cmd_json_with_config(db_path: &Path, config_path: &Path, args: &[&str]) -> Value {
    let config_dir = TempDir::new().expect("temp config dir");
    let output = cargo_bin_cmd!("knotter")
        .env("XDG_CONFIG_HOME", config_dir.path())
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

fn run_cmd_json_with_env(db_path: &Path, args: &[&str], envs: &[(&str, &str)]) -> Value {
    let config_dir = TempDir::new().expect("temp config dir");
    let mut cmd = cargo_bin_cmd!("knotter");
    cmd.env("XDG_CONFIG_HOME", config_dir.path())
        .args(["--db-path", db_path.to_str().expect("db path"), "--json"])
        .args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("run command");
    assert!(output.status.success(), "command failed: {:?}", output);
    serde_json::from_slice(&output.stdout).expect("parse json")
}

#[test]
fn cli_merge_contacts_merges_records() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("knotter.sqlite3");
    let store = Store::open(&db_path).expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let primary = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Ada".to_string(),
                email: Some("ada@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create primary");

    let secondary = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Ada L".to_string(),
                email: Some("ada@work.test".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create secondary");

    let merged = run_cmd_json(
        &db_path,
        &[
            "merge",
            "contacts",
            &primary.id.to_string(),
            &secondary.id.to_string(),
        ],
    );

    assert_eq!(merged["id"], primary.id.to_string());
    assert!(store
        .contacts()
        .get(secondary.id)
        .expect("get secondary")
        .is_none());
}

#[test]
fn cli_merge_list_outputs_candidates() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("knotter.sqlite3");
    let store = Store::open(&db_path).expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let contact_a = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Ada".to_string(),
                email: Some("ada@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact a");

    let contact_b = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Ada L".to_string(),
                email: Some("ada@work.test".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create contact b");

    store
        .merge_candidates()
        .create(
            now,
            contact_a.id,
            contact_b.id,
            MergeCandidateCreate {
                reason: "test".to_string(),
                source: Some("cli".to_string()),
                preferred_contact_id: Some(contact_a.id),
            },
        )
        .expect("create candidate");

    let value = run_cmd_json(&db_path, &["merge", "list"]);
    let array = value.as_array().expect("array");
    assert_eq!(array.len(), 1);
}

#[test]
fn cli_merge_apply_merges_candidate() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("knotter.sqlite3");
    let store = Store::open(&db_path).expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let primary = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Ada".to_string(),
                email: Some("ada@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create primary");

    let secondary = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Ada L".to_string(),
                email: Some("ada@work.test".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create secondary");

    let created = store
        .merge_candidates()
        .create(
            now,
            primary.id,
            secondary.id,
            MergeCandidateCreate {
                reason: "test".to_string(),
                source: None,
                preferred_contact_id: Some(primary.id),
            },
        )
        .expect("create candidate");

    let merged = run_cmd_json(
        &db_path,
        &["merge", "apply", &created.candidate.id.to_string()],
    );
    assert_eq!(merged["id"], primary.id.to_string());

    let store = Store::open(&db_path).expect("open store");
    let candidate = store
        .merge_candidates()
        .get(created.candidate.id)
        .expect("get candidate")
        .expect("missing candidate");
    assert_eq!(
        candidate.status,
        knotter_store::repo::MergeCandidateStatus::Merged
    );
    assert!(store
        .contacts()
        .get(secondary.id)
        .expect("get secondary")
        .is_none());
}

#[test]
fn cli_merge_apply_all_applies_safe_candidates_only() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("knotter.sqlite3");
    let store = Store::open(&db_path).expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    let primary = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Safe Primary".to_string(),
                email: Some("safe@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create primary");
    let secondary = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Safe Secondary".to_string(),
                email: Some("safe-alt@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create secondary");
    let safe_candidate = store
        .merge_candidates()
        .create(
            now,
            primary.id,
            secondary.id,
            MergeCandidateCreate {
                reason: MergeCandidateReason::EmailDuplicate.as_str().to_string(),
                source: Some("cli".to_string()),
                preferred_contact_id: Some(primary.id),
            },
        )
        .expect("create safe candidate");

    let other_primary = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Unsafe Primary".to_string(),
                email: Some("unsafe@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create other primary");
    let other_secondary = store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Unsafe Secondary".to_string(),
                email: Some("unsafe-alt@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create other secondary");
    let unsafe_candidate = store
        .merge_candidates()
        .create(
            now,
            other_primary.id,
            other_secondary.id,
            MergeCandidateCreate {
                reason: MergeCandidateReason::EmailNameAmbiguous
                    .as_str()
                    .to_string(),
                source: Some("cli".to_string()),
                preferred_contact_id: Some(other_primary.id),
            },
        )
        .expect("create unsafe candidate");

    let report = run_cmd_json(&db_path, &["merge", "apply-all", "--yes"]);
    assert_eq!(report["considered"], 2);
    assert_eq!(report["selected"], 1);
    assert_eq!(report["applied"], 1);
    assert_eq!(report["skipped"], 0);
    assert_eq!(report["failed"], 0);

    let store = Store::open(&db_path).expect("open store");
    let safe = store
        .merge_candidates()
        .get(safe_candidate.candidate.id)
        .expect("get safe candidate")
        .expect("missing safe candidate");
    assert_eq!(
        safe.status,
        knotter_store::repo::MergeCandidateStatus::Merged
    );
    assert!(store
        .contacts()
        .get(secondary.id)
        .expect("get secondary")
        .is_none());

    let unsafe_candidate = store
        .merge_candidates()
        .get(unsafe_candidate.candidate.id)
        .expect("get unsafe candidate")
        .expect("missing unsafe candidate");
    assert_eq!(
        unsafe_candidate.status,
        knotter_store::repo::MergeCandidateStatus::Open
    );
    assert!(store
        .contacts()
        .get(other_secondary.id)
        .expect("get other secondary")
        .is_some());
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
fn cli_completions_bash_emits_output() {
    let output = cargo_bin_cmd!("knotter")
        .args(["completions", "bash"])
        .output()
        .expect("run completions");
    assert!(output.status.success(), "command failed: {:?}", output);
    let stdout = String::from_utf8(output.stdout).expect("utf8");
    assert!(!stdout.trim().is_empty());
    assert!(stdout.contains("knotter"));
}

#[test]
fn cli_import_vcf_dry_run_skips_writes() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let vcf_path = temp.path().join("contacts.vcf");

    std::fs::write(
        &vcf_path,
        "BEGIN:VCARD\nVERSION:3.0\nFN:Ada Lovelace\nEMAIL:ada@example.com\nEND:VCARD\n",
    )
    .expect("write vcf");

    let output = run_cmd_json(
        &db_path,
        &[
            "import",
            "vcf",
            "--dry-run",
            vcf_path.to_str().expect("vcf path"),
        ],
    );
    assert_eq!(output["created"], 1);
    assert_eq!(output["updated"], 0);
    assert_eq!(output["skipped"], 0);
    assert_eq!(output["dry_run"], true);

    let list = run_cmd_json(&db_path, &["list"]);
    assert!(list.as_array().expect("array").is_empty());
}

#[test]
#[cfg(not(feature = "dav-sync"))]
fn cli_import_source_requires_dav_sync() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[contacts]
[[contacts.sources]]
name = "gmail"
type = "carddav"
url = "https://example.test/carddav/"
username = "user@example.com"
password_env = "KNOTTER_GMAIL_PASSWORD"
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let output = cargo_bin_cmd!("knotter")
        .args([
            "--db-path",
            db_path.to_str().expect("db path"),
            "--config",
            config_path.to_str().expect("config path"),
            "import",
            "source",
            "gmail",
            "--dry-run",
        ])
        .env("KNOTTER_GMAIL_PASSWORD", "secret")
        .output()
        .expect("run command");

    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("dav-sync"));
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
fn cli_schedule_rejects_past_date() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);
    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();

    let output = run_cmd_output(&db_path, &["schedule", &id, "--at", "2000-01-01"]);
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("timestamp must be now or later"));
}

#[test]
fn cli_add_contact_rejects_past_next_touchpoint() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    let output = run_cmd_output(
        &db_path,
        &[
            "add-contact",
            "--name",
            "Ada Lovelace",
            "--next-touchpoint-at",
            "2000-01-01",
        ],
    );
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("timestamp must be now or later"));
}

#[test]
fn cli_schedule_date_only_sets_end_of_day() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);
    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();

    run_cmd(&db_path, &["schedule", &id, "--at", "2030-01-15"]);

    let detail = run_cmd_json(&db_path, &["show", &id]);
    let (timestamp, precision) =
        knotter_core::time::parse_local_timestamp_with_precision("2030-01-15").expect("parse date");
    let expected = knotter_core::rules::ensure_future_timestamp_with_precision(
        knotter_core::time::now_utc(),
        timestamp,
        precision,
    )
    .expect("expected schedule");
    assert_eq!(detail["next_touchpoint_at"], expected);
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
fn cli_date_add_list_and_remind_includes_today() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);
    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();

    let fixed_local = Local
        .with_ymd_and_hms(2030, 1, 15, 12, 0, 0)
        .single()
        .expect("local time");
    let date_str = fixed_local.format("%Y-%m-%d").to_string();
    let now_env = fixed_local.with_timezone(&Utc).timestamp().to_string();

    run_cmd(
        &db_path,
        &["date", "add", &id, "--kind", "birthday", "--on", &date_str],
    );

    let dates = run_cmd_json(&db_path, &["date", "ls", &id]);
    let dates = dates.as_array().expect("dates array");
    assert_eq!(dates.len(), 1);
    assert_eq!(dates[0]["kind"], "birthday");

    let remind = run_cmd_json_with_env(
        &db_path,
        &["remind"],
        &[
            ("KNOTTER_TEST_NOW_UTC", now_env.as_str()),
            ("KNOTTER_ALLOW_TEST_NOW_UTC", "1"),
        ],
    );
    let dates_today = remind["dates_today"].as_array().expect("dates_today array");
    assert_eq!(dates_today.len(), 1);
    assert_eq!(dates_today[0]["display_name"], "Ada Lovelace");
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
fn cli_remind_email_backend_fails_without_feature() {
    if cfg!(feature = "email-notify") {
        return;
    }

    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        "due_soon_days = 3650\n[notifications]\nenabled = true\nbackend = \"email\"\n\n[notifications.email]\nfrom = \"Knotter <knotter@example.com>\"\nto = [\"ada@example.com\"]\nsmtp_host = \"smtp.example.com\"\nsmtp_port = 587\nusername = \"user@example.com\"\npassword_env = \"KNOTTER_SMTP_PASSWORD\"\ntls = \"start-tls\"\ntimeout_seconds = 20\n",
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

    let output = run_cmd_output_with_config(
        &db_path,
        &config_path,
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
fn cli_import_vcf_dedupes_by_uid() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let vcf_path = temp.path().join("contacts.vcf");

    let vcf = "BEGIN:VCARD\nVERSION:3.0\nUID:abc-123\nFN:Grace Hopper\nEND:VCARD\n";
    std::fs::write(&vcf_path, vcf).expect("write vcf");

    run_cmd(
        &db_path,
        &["import", "vcf", vcf_path.to_str().expect("path")],
    );

    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["display_name"], "Grace Hopper");

    let vcf = "BEGIN:VCARD\nVERSION:3.0\nUID:abc-123\nFN:Grace H.\nEND:VCARD\n";
    std::fs::write(&vcf_path, vcf).expect("write vcf");

    run_cmd(
        &db_path,
        &["import", "vcf", vcf_path.to_str().expect("path")],
    );

    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["display_name"], "Grace H.");
}

#[test]
fn cli_import_vcf_updates_when_emails_match_active_and_archived() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let vcf_path = temp.path().join("contacts.vcf");
    let store = Store::open(&db_path).expect("open store");
    store.migrate().expect("migrate");
    let now = 1_700_000_000;

    store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Active".to_string(),
                email: Some("active@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: None,
            },
        )
        .expect("create active");
    store
        .contacts()
        .create(
            now,
            knotter_store::repo::ContactNew {
                display_name: "Archived".to_string(),
                email: Some("archived@example.com".to_string()),
                phone: None,
                handle: None,
                timezone: None,
                next_touchpoint_at: None,
                cadence_days: None,
                archived_at: Some(now),
            },
        )
        .expect("create archived");

    let vcf = "BEGIN:VCARD\nVERSION:3.0\nFN:Mixed\nEMAIL:active@example.com\nEMAIL:archived@example.com\nEND:VCARD\n";
    std::fs::write(&vcf_path, vcf).expect("write vcf");

    let report = run_cmd_json(
        &db_path,
        &["import", "vcf", vcf_path.to_str().expect("path")],
    );
    assert_eq!(report["created"], 0);
    assert_eq!(report["updated"], 1);
    assert_eq!(report["skipped"], 0);
    assert_eq!(report["merge_candidates_created"], 0);

    let store = Store::open(&db_path).expect("open store");
    let candidates = store
        .merge_candidates()
        .list(None)
        .expect("list candidates");
    assert!(candidates.is_empty());
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
fn cli_invalid_filter_returns_exit_code_3() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    let output = run_cmd_output(&db_path, &["list", "--filter", "due:later"]);
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid due selector"));
}

#[test]
fn cli_show_missing_contact_returns_exit_code_2() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let missing = ContactId::new().to_string();

    let output = run_cmd_output(&db_path, &["show", &missing]);
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("contact not found"));
}

#[test]
fn cli_export_ics_invalid_window_returns_exit_code_3() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    let output = run_cmd_output(&db_path, &["export", "ics", "--window-days", "0"]);
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--window-days must be positive"));
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
fn cli_add_note_reschedule_updates_next_touchpoint() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    let created = run_cmd_json(
        &db_path,
        &[
            "add-contact",
            "--name",
            "Ada Lovelace",
            "--cadence-days",
            "7",
        ],
    );
    let id = created["id"].as_str().expect("id").to_string();

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
            "--reschedule",
        ],
    );

    let detail = run_cmd_json(&db_path, &["show", &id]);
    let occurred_at = parse_local_timestamp("2030-01-02").expect("parse when");
    let expected = schedule_next(occurred_at, 7).expect("schedule");
    assert_eq!(detail["next_touchpoint_at"], expected);
}

#[test]
fn cli_add_note_auto_reschedule_config_updates_next_touchpoint() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[interactions]
auto_reschedule = true
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &[
            "add-contact",
            "--name",
            "Grace Hopper",
            "--cadence-days",
            "14",
        ],
    );
    let id = created["id"].as_str().expect("id").to_string();

    run_cmd_with_config(
        &db_path,
        &config_path,
        &[
            "add-note",
            &id,
            "--kind",
            "email",
            "--note",
            "follow up",
            "--when",
            "2030-02-01",
        ],
    );

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    let occurred_at = parse_local_timestamp("2030-02-01").expect("parse when");
    let expected = schedule_next(occurred_at, 14).expect("schedule");
    assert_eq!(detail["next_touchpoint_at"], expected);
}

#[test]
fn cli_add_note_no_reschedule_overrides_config() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[interactions]
auto_reschedule = true
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &[
            "add-contact",
            "--name",
            "Ada Lovelace",
            "--cadence-days",
            "7",
        ],
    );
    let id = created["id"].as_str().expect("id").to_string();

    run_cmd_with_config(
        &db_path,
        &config_path,
        &[
            "add-note",
            &id,
            "--kind",
            "call",
            "--note",
            "hello",
            "--when",
            "2030-01-02",
            "--no-reschedule",
        ],
    );

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    assert!(detail["next_touchpoint_at"].is_null());
}

#[test]
fn cli_touch_auto_reschedule_config_updates_next_touchpoint() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[interactions]
auto_reschedule = true
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &[
            "add-contact",
            "--name",
            "Grace Hopper",
            "--cadence-days",
            "10",
        ],
    );
    let id = created["id"].as_str().expect("id").to_string();

    let before = knotter_core::time::now_utc();
    run_cmd_with_config(&db_path, &config_path, &["touch", &id]);

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    let next = detail["next_touchpoint_at"]
        .as_i64()
        .expect("next touchpoint");
    let expected_min = schedule_next(before, 10).expect("schedule");
    assert!(next >= expected_min);
}

#[test]
fn cli_touch_no_reschedule_overrides_config() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[interactions]
auto_reschedule = true
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &[
            "add-contact",
            "--name",
            "Ada Lovelace",
            "--cadence-days",
            "10",
        ],
    );
    let id = created["id"].as_str().expect("id").to_string();

    run_cmd_with_config(&db_path, &config_path, &["touch", &id, "--no-reschedule"]);

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    assert!(detail["next_touchpoint_at"].is_null());
}

#[test]
fn cli_touch_records_kind_and_reschedules() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    let created = run_cmd_json(
        &db_path,
        &[
            "add-contact",
            "--name",
            "Margaret Hamilton",
            "--cadence-days",
            "10",
        ],
    );
    let id = created["id"].as_str().expect("id").to_string();

    run_cmd(
        &db_path,
        &[
            "touch",
            &id,
            "--kind",
            "call",
            "--note",
            "sync",
            "--when",
            "2030-03-01",
            "--reschedule",
        ],
    );

    let detail = run_cmd_json(&db_path, &["show", &id]);
    let occurred_at = parse_local_timestamp("2030-03-01").expect("parse when");
    let expected = schedule_next(occurred_at, 10).expect("schedule");
    assert_eq!(detail["next_touchpoint_at"], expected);

    let store = Store::open(&db_path).expect("open store");
    let contact_id = ContactId::from_str(&id).expect("contact id");
    let interactions = store
        .interactions()
        .list_for_contact(contact_id, 10, 0)
        .expect("list interactions");
    assert_eq!(interactions.len(), 1);
    assert!(matches!(interactions[0].kind, InteractionKind::Call));
    assert_eq!(interactions[0].note, "sync");
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
fn cli_archive_and_list_filters_archived() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    let active = run_cmd_json(&db_path, &["add-contact", "--name", "Active"]);
    let archived = run_cmd_json(&db_path, &["add-contact", "--name", "Archived"]);
    let archived_id = archived["id"].as_str().expect("archived id");

    let archived_out = run_cmd_json(&db_path, &["archive-contact", archived_id]);
    assert!(archived_out["archived_at"].is_number());

    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("list array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], active["id"]);
    assert!(items[0]["archived_at"].is_null());

    let list = run_cmd_json(&db_path, &["list", "--include-archived"]);
    let items = list.as_array().expect("list array");
    assert_eq!(items.len(), 2);
    let archived_item = items
        .iter()
        .find(|item| item["id"] == archived["id"])
        .expect("archived item");
    assert!(archived_item["archived_at"].is_number());

    let list = run_cmd_json(&db_path, &["list", "--only-archived"]);
    let items = list.as_array().expect("list array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], archived["id"]);

    let unarchived_out = run_cmd_json(&db_path, &["unarchive-contact", archived_id]);
    assert!(unarchived_out["archived_at"].is_null());
}

#[test]
fn cli_list_archived_filter_tokens() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    let active = run_cmd_json(&db_path, &["add-contact", "--name", "Active"]);
    let archived = run_cmd_json(&db_path, &["add-contact", "--name", "Archived"]);
    let archived_id = archived["id"].as_str().expect("archived id");

    let archived_out = run_cmd_json(&db_path, &["archive-contact", archived_id]);
    assert!(archived_out["archived_at"].is_number());

    let list = run_cmd_json(&db_path, &["list", "--filter", "archived:true"]);
    let items = list.as_array().expect("list array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], archived["id"]);

    let list = run_cmd_json(&db_path, &["list", "--filter", "archived:false"]);
    let items = list.as_array().expect("list array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], active["id"]);

    let list = run_cmd_json(
        &db_path,
        &["list", "--only-archived", "--filter", "archived:true"],
    );
    let items = list.as_array().expect("list array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], archived["id"]);
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
fn cli_add_contact_rejects_duplicate_email() {
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
    let output = run_cmd_output(
        &db_path,
        &[
            "add-contact",
            "--name",
            "Second",
            "--email",
            "dup@example.com",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("duplicate email"));

    let vcf = "BEGIN:VCARD\nVERSION:3.0\nFN:Updated Name\nEMAIL:dup@example.com\nEND:VCARD\n";
    std::fs::write(&vcf_path, vcf).expect("write vcf");

    let report = run_cmd_json(
        &db_path,
        &["import", "vcf", vcf_path.to_str().expect("path")],
    );
    assert_eq!(report["created"], 0);
    assert_eq!(report["updated"], 1);
    assert_eq!(report["skipped"], 0);

    let list = run_cmd_json(&db_path, &["list"]);
    let names: Vec<String> = list
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item["display_name"].as_str().expect("name").to_string())
        .collect();
    assert!(names.contains(&"Updated Name".to_string()));
    assert!(!names.contains(&"Second".to_string()));
}

#[test]
fn cli_add_contact_rejects_duplicate_secondary_email() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

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

    let output = run_cmd_output(
        &db_path,
        &[
            "add-contact",
            "--name",
            "Second",
            "--email",
            "second@example.com",
            "--email",
            "dup@example.com",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("duplicate email"));

    let list = run_cmd_json(&db_path, &["list"]);
    let names: Vec<String> = list
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item["display_name"].as_str().expect("name").to_string())
        .collect();
    assert!(names.contains(&"First".to_string()));
    assert!(!names.contains(&"Second".to_string()));
}

#[test]
fn cli_edit_contact_rejects_add_remove_overlap() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    let output = run_cmd_json(
        &db_path,
        &["add-contact", "--name", "Ada", "--email", "ada@example.com"],
    );
    let id = output["id"].as_str().expect("id");

    let output = run_cmd_output(
        &db_path,
        &[
            "edit-contact",
            id,
            "--add-email",
            "ada.work@example.com",
            "--remove-email",
            "ada.work@example.com",
        ],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("cannot be both added and removed"));
}

#[test]
fn cli_loops_apply_updates_cadence_and_schedules() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
strategy = "shortest"
schedule_missing = true
anchor = "created-at"

[[loops.tags]]
tag = "friend"
cadence_days = 90

[[loops.tags]]
tag = "family"
cadence_days = 30
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    run_cmd_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace"],
    );
    run_cmd_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Grace Hopper"],
    );

    let list = run_cmd_json_with_config(&db_path, &config_path, &["list"]);
    let items = list.as_array().expect("array");
    let mut ada_id = None;
    let mut grace_id = None;
    for item in items {
        match item["display_name"].as_str().expect("name") {
            "Ada Lovelace" => ada_id = item["id"].as_str().map(|id| id.to_string()),
            "Grace Hopper" => grace_id = item["id"].as_str().map(|id| id.to_string()),
            _ => {}
        }
    }
    let ada_id = ada_id.expect("ada id");
    let grace_id = grace_id.expect("grace id");

    run_cmd_with_config(&db_path, &config_path, &["tag", "add", &grace_id, "friend"]);

    run_cmd_with_config(&db_path, &config_path, &["loops", "apply"]);

    let ada = run_cmd_json_with_config(&db_path, &config_path, &["show", &ada_id]);
    assert!(ada["cadence_days"].is_null());
    assert!(ada["next_touchpoint_at"].is_null());

    let grace = run_cmd_json_with_config(&db_path, &config_path, &["show", &grace_id]);
    assert_eq!(grace["cadence_days"], 90);
    assert!(grace["next_touchpoint_at"].is_number());
}

#[test]
fn cli_tag_add_apply_on_tag_change_updates_cadence() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
apply_on_tag_change = true
schedule_missing = false

[[loops.tags]]
tag = "friend"
cadence_days = 90
"#,
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

    run_cmd_with_config(&db_path, &config_path, &["tag", "add", &id, "friend"]);

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    assert_eq!(detail["cadence_days"], 90);
    assert!(detail["next_touchpoint_at"].is_null());
}

#[test]
fn cli_add_contact_with_tag_applies_loop_policy() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
default_cadence_days = 180
strategy = "shortest"
schedule_missing = true
anchor = "created-at"

[[loops.tags]]
tag = "friend"
cadence_days = 90
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace", "--tag", "friend"],
    );
    let id = created["id"].as_str().expect("id").to_string();
    assert_eq!(created["cadence_days"], 90);

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    let tags = detail["tags"].as_array().expect("tags");
    assert!(tags.iter().any(|tag| tag == "friend"));
    assert_eq!(detail["cadence_days"], 90);
    assert!(detail["next_touchpoint_at"].is_number());
}

#[test]
fn cli_loops_apply_no_schedule_missing_skips_scheduling() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
schedule_missing = true

[[loops.tags]]
tag = "friend"
cadence_days = 10
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace"],
    );
    let id = created["id"].as_str().expect("id").to_string();
    run_cmd_with_config(&db_path, &config_path, &["tag", "add", &id, "friend"]);

    run_cmd_with_config(
        &db_path,
        &config_path,
        &["loops", "apply", "--no-schedule-missing"],
    );

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    assert_eq!(detail["cadence_days"], 10);
    assert!(detail["next_touchpoint_at"].is_null());
}

#[test]
fn cli_loops_apply_anchor_last_interaction_uses_interaction_timestamp() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
schedule_missing = false
anchor = "last-interaction"

[[loops.tags]]
tag = "friend"
cadence_days = 7
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace", "--tag", "friend"],
    );
    let id = created["id"].as_str().expect("id").to_string();
    let contact_id = ContactId::from_str(&id).expect("contact id");

    let store = Store::open(&db_path).expect("open store");
    let occurred_at = 1_700_000_000;
    store
        .interactions()
        .add(knotter_store::repo::InteractionNew {
            contact_id,
            occurred_at,
            created_at: occurred_at,
            kind: knotter_core::domain::InteractionKind::Call,
            note: "hello".to_string(),
            follow_up_at: None,
        })
        .expect("add interaction");

    run_cmd_with_config(
        &db_path,
        &config_path,
        &["loops", "apply", "--schedule-missing"],
    );

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    let expected = schedule_next(occurred_at, 7).expect("schedule");
    assert_eq!(detail["next_touchpoint_at"], expected);
}

#[test]
fn cli_loops_apply_anchor_last_interaction_skips_without_interactions() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
schedule_missing = true
anchor = "last-interaction"

[[loops.tags]]
tag = "friend"
cadence_days = 7
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace", "--tag", "friend"],
    );
    let id = created["id"].as_str().expect("id").to_string();

    run_cmd_with_config(&db_path, &config_path, &["loops", "apply"]);

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    assert!(detail["next_touchpoint_at"].is_null());
}

#[test]
fn cli_loops_apply_force_overrides_existing_cadence() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
schedule_missing = false

[[loops.tags]]
tag = "friend"
cadence_days = 90
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &[
            "add-contact",
            "--name",
            "Ada Lovelace",
            "--cadence-days",
            "180",
        ],
    );
    let id = created["id"].as_str().expect("id").to_string();
    run_cmd_with_config(&db_path, &config_path, &["tag", "add", &id, "friend"]);

    run_cmd_with_config(&db_path, &config_path, &["loops", "apply"]);
    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    assert_eq!(detail["cadence_days"], 180);

    run_cmd_with_config(&db_path, &config_path, &["loops", "apply", "--force"]);
    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    assert_eq!(detail["cadence_days"], 90);
}

#[test]
fn cli_tag_remove_apply_loop_keeps_command_successful() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
schedule_missing = false

[[loops.tags]]
tag = "friend"
cadence_days = 90
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace"],
    );
    let id = created["id"].as_str().expect("id").to_string();
    run_cmd_with_config(&db_path, &config_path, &["tag", "add", &id, "friend"]);

    run_cmd_with_config(
        &db_path,
        &config_path,
        &["tag", "rm", &id, "friend", "--apply-loop"],
    );

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    let tags = detail["tags"].as_array().expect("tags");
    assert!(tags.is_empty());
}

#[test]
fn cli_add_contact_anchor_last_interaction_does_not_schedule() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
schedule_missing = true
anchor = "last-interaction"

[[loops.tags]]
tag = "friend"
cadence_days = 30
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace", "--tag", "friend"],
    );
    let id = created["id"].as_str().expect("id").to_string();
    assert_eq!(created["cadence_days"], 30);

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    assert_eq!(detail["cadence_days"], 30);
    assert!(detail["next_touchpoint_at"].is_null());
}

#[test]
fn cli_tag_add_apply_loop_requires_loops_configured() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    run_cmd(&db_path, &["add-contact", "--name", "Ada Lovelace"]);
    let list = run_cmd_json(&db_path, &["list"]);
    let items = list.as_array().expect("array");
    let id = items[0]["id"].as_str().expect("id").to_string();

    let output = run_cmd_output(&db_path, &["tag", "add", &id, "friend", "--apply-loop"]);
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("no loops configured"));
}

#[test]
fn cli_loops_apply_dry_run_does_not_modify_data() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
schedule_missing = false
anchor = "created-at"

[[loops.tags]]
tag = "friend"
cadence_days = 30
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let created = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace", "--tag", "friend"],
    );
    let id = created["id"].as_str().expect("id").to_string();

    run_cmd_with_config(&db_path, &config_path, &["loops", "apply", "--dry-run"]);

    let detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &id]);
    assert!(detail["next_touchpoint_at"].is_null());
    assert_eq!(detail["cadence_days"], 30);
}

#[test]
fn cli_loops_apply_filter_scopes_updates() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");
    let config_path = temp.path().join("config.toml");

    std::fs::write(
        &config_path,
        r#"
[loops]
schedule_missing = false
anchor = "created-at"

[[loops.tags]]
tag = "friend"
cadence_days = 30
"#,
    )
    .expect("write config");
    restrict_config_permissions(&config_path);

    let ada = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &["add-contact", "--name", "Ada Lovelace", "--tag", "friend"],
    );
    let ada_id = ada["id"].as_str().expect("id").to_string();

    let grace = run_cmd_json_with_config(
        &db_path,
        &config_path,
        &[
            "add-contact",
            "--name",
            "Grace Hopper",
            "--tag",
            "friend",
            "--cadence-days",
            "7",
        ],
    );
    let grace_id = grace["id"].as_str().expect("id").to_string();

    run_cmd_with_config(
        &db_path,
        &config_path,
        &["loops", "apply", "--filter", "Ada"],
    );

    let ada_detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &ada_id]);
    let grace_detail = run_cmd_json_with_config(&db_path, &config_path, &["show", &grace_id]);

    assert!(ada_detail["cadence_days"].is_number());
    assert_eq!(grace_detail["cadence_days"], 7);
}

#[test]
fn cli_sync_rejects_json() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    let output = cargo_bin_cmd!("knotter")
        .args([
            "--db-path",
            db_path.to_str().expect("db path"),
            "--json",
            "sync",
        ])
        .output()
        .expect("run command");

    assert!(!output.status.success(), "command unexpectedly succeeded");
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).expect("utf8");
    assert!(stderr.contains("sync does not support --json"));
}

#[test]
fn cli_sync_errors_without_sources_or_accounts() {
    let temp = TempDir::new().expect("temp dir");
    let db_path = temp.path().join("knotter.sqlite3");

    let output = run_cmd_output(&db_path, &["sync"]);

    assert!(!output.status.success(), "command unexpectedly succeeded");
    assert_eq!(output.status.code(), Some(3));
    let stderr = String::from_utf8(output.stderr).expect("utf8");
    assert!(stderr.contains("no contact sources, email accounts, or telegram accounts configured"));
}
