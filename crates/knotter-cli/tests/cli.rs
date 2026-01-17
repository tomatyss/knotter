use assert_cmd::cargo::cargo_bin_cmd;
use knotter_core::rules::MAX_SOON_DAYS;
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
