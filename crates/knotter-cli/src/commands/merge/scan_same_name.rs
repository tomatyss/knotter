use crate::commands::{print_json, Context};
use crate::error::invalid_input;
use anyhow::Result;
use clap::Args;
use knotter_core::domain::{Contact, ContactId};
use knotter_store::repo::MergeCandidateCreate;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

const REASON: &str = "name-duplicate";
const SOURCE: &str = "scan:same-name";

#[derive(Debug, Args)]
pub struct MergeScanSameNameArgs {
    #[arg(long, help = "Include archived contacts in the scan")]
    pub include_archived: bool,
    #[arg(
        long,
        help = "Only include contacts with a mapping in contact_sources for this source (e.g. macos-contacts)"
    )]
    pub contact_source: Option<String>,
    #[arg(
        long,
        help = "Only scan the first N duplicate-name groups (after grouping)"
    )]
    pub limit: Option<usize>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long, help = "Skip confirmation (required unless --dry-run is set)")]
    pub yes: bool,
}

#[derive(Debug, Serialize)]
struct MergeScanSameNameReport {
    considered_contacts: usize,
    skipped_empty_name_contacts: usize,
    duplicate_groups: usize,
    groups_scanned: usize,
    candidates_created: usize,
    pairs_skipped_existing_open: usize,
    dry_run: bool,
    // Results are ordered by group size desc, then normalized_name.
    results: Vec<MergeScanSameNameGroupResult>,
}

#[derive(Debug, Serialize)]
struct MergeScanSameNameGroupResult {
    display_name: String,
    normalized_name: String,
    preferred_contact_id: String,
    // Pairs are ordered by secondary_id asc.
    pairs: Vec<MergeScanSameNamePairResult>,
}

#[derive(Debug, Serialize)]
struct MergeScanSameNamePairResult {
    primary_id: String,
    secondary_id: String,
    status: String,
    merge_candidate_id: Option<String>,
}

pub fn scan_same_name(ctx: &Context<'_>, args: MergeScanSameNameArgs) -> Result<()> {
    // This is intentionally conservative: it creates merge candidates for review, but does not
    // auto-merge name-only duplicates (too easy to get wrong).
    if !args.dry_run && !args.yes {
        return Err(invalid_input(
            "merge scan-same-name requires --yes unless --dry-run is set",
        ));
    }

    let mut contacts = ctx.store.contacts().list_all()?;
    if !args.include_archived {
        contacts.retain(|c| c.archived_at.is_none());
    }

    if let Some(source) = args.contact_source.as_deref() {
        let ids = ctx
            .store
            .contact_sources()
            .list_contact_ids_for_source(source)?;
        let allowed: HashSet<ContactId> = ids.into_iter().collect();
        contacts.retain(|c| allowed.contains(&c.id));
    }

    let considered_contacts = contacts.len();
    let mut skipped_empty_name_contacts = 0;

    let mut groups: HashMap<String, Vec<Contact>> = HashMap::new();
    for contact in contacts {
        let key = normalize_display_name(&contact.display_name);
        if key.is_empty() {
            skipped_empty_name_contacts += 1;
            continue;
        }
        groups.entry(key).or_default().push(contact);
    }

    let mut dupe_groups: Vec<(String, Vec<Contact>)> = groups
        .into_iter()
        .filter(|(_key, items)| items.len() > 1)
        .collect();
    dupe_groups.sort_by(|(a_key, a_items), (b_key, b_items)| {
        b_items
            .len()
            .cmp(&a_items.len())
            .then_with(|| a_key.cmp(b_key))
    });

    let duplicate_groups = dupe_groups.len();
    if let Some(limit) = args.limit {
        dupe_groups.truncate(limit);
    }

    // Avoid creating duplicate open candidates for the same pair (in either direction).
    let open = ctx.store.merge_candidates().list_open()?;
    let mut open_pairs: HashSet<(String, String)> = HashSet::new();
    for candidate in open {
        let a = candidate.contact_a_id.to_string();
        let b = candidate.contact_b_id.to_string();
        open_pairs.insert(pair_key(&a, &b));
    }

    let mut report = MergeScanSameNameReport {
        considered_contacts,
        skipped_empty_name_contacts,
        duplicate_groups,
        groups_scanned: dupe_groups.len(),
        candidates_created: 0,
        pairs_skipped_existing_open: 0,
        dry_run: args.dry_run,
        results: Vec::new(),
    };

    let now = crate::util::now_utc();

    if args.dry_run {
        for (normalized_name, items) in dupe_groups {
            let group =
                build_group_result_dry_run(normalized_name, items, &mut open_pairs, &mut report);
            report.results.push(group);
        }
    } else {
        let tx = ctx.store.connection().unchecked_transaction()?;
        let repo = knotter_store::repo::MergeCandidatesRepo::new(&tx);

        for (normalized_name, items) in dupe_groups {
            let group = build_group_result_apply(
                now,
                normalized_name,
                items,
                &repo,
                &mut open_pairs,
                &mut report,
            )?;
            report.results.push(group);
        }

        tx.commit()?;
    }

    if ctx.json {
        return print_json(&report);
    }

    if report.groups_scanned == 0 {
        println!("No duplicate-name groups found.");
        return Ok(());
    }

    if report.skipped_empty_name_contacts > 0 {
        println!(
            "Skipped {} contact(s) with empty/whitespace display names.",
            report.skipped_empty_name_contacts
        );
    }

    if report.dry_run {
        println!(
            "Dry-run: {} duplicate-name group(s), {} pair(s) considered.",
            report.groups_scanned,
            report.results.iter().map(|g| g.pairs.len()).sum::<usize>()
        );
    } else {
        println!(
            "Created {} merge candidate(s) from {} duplicate-name group(s).",
            report.candidates_created, report.groups_scanned
        );
    }

    for group in &report.results {
        println!();
        println!(
            "{} (preferred {})",
            group.display_name, group.preferred_contact_id
        );
        for pair in &group.pairs {
            let id = pair
                .merge_candidate_id
                .as_deref()
                .map(|v| format!(" ({v})"))
                .unwrap_or_default();
            println!(
                "  {}  {} -> {}{}",
                pair.status, pair.secondary_id, pair.primary_id, id
            );
        }
    }

    Ok(())
}

fn build_group_result_dry_run(
    normalized_name: String,
    mut items: Vec<Contact>,
    open_pairs: &mut HashSet<(String, String)>,
    report: &mut MergeScanSameNameReport,
) -> MergeScanSameNameGroupResult {
    items.sort_by(|a, b| a.id.to_string().cmp(&b.id.to_string()));
    let preferred = choose_preferred_contact(&items);
    let display_name = items
        .iter()
        .find(|c| c.id == preferred)
        .map(|c| c.display_name.clone())
        .unwrap_or_else(|| items[0].display_name.clone());

    let mut group = MergeScanSameNameGroupResult {
        display_name,
        normalized_name,
        preferred_contact_id: preferred.to_string(),
        pairs: Vec::new(),
    };

    for contact in &items {
        if contact.id == preferred {
            continue;
        }
        let a = preferred.to_string();
        let b = contact.id.to_string();
        let key = pair_key(&a, &b);
        if open_pairs.contains(&key) {
            report.pairs_skipped_existing_open += 1;
            group.pairs.push(MergeScanSameNamePairResult {
                primary_id: a,
                secondary_id: b,
                status: "skipped-existing-open".to_string(),
                merge_candidate_id: None,
            });
            continue;
        }

        group.pairs.push(MergeScanSameNamePairResult {
            primary_id: a,
            secondary_id: b,
            status: "dry-run".to_string(),
            merge_candidate_id: None,
        });
    }

    group
}

fn build_group_result_apply(
    now_utc: i64,
    normalized_name: String,
    mut items: Vec<Contact>,
    repo: &knotter_store::repo::MergeCandidatesRepo<'_>,
    open_pairs: &mut HashSet<(String, String)>,
    report: &mut MergeScanSameNameReport,
) -> Result<MergeScanSameNameGroupResult> {
    items.sort_by(|a, b| a.id.to_string().cmp(&b.id.to_string()));
    let preferred = choose_preferred_contact(&items);
    let display_name = items
        .iter()
        .find(|c| c.id == preferred)
        .map(|c| c.display_name.clone())
        .unwrap_or_else(|| items[0].display_name.clone());

    let mut group = MergeScanSameNameGroupResult {
        display_name,
        normalized_name,
        preferred_contact_id: preferred.to_string(),
        pairs: Vec::new(),
    };

    for contact in &items {
        if contact.id == preferred {
            continue;
        }
        let a = preferred.to_string();
        let b = contact.id.to_string();
        let key = pair_key(&a, &b);
        if open_pairs.contains(&key) {
            report.pairs_skipped_existing_open += 1;
            group.pairs.push(MergeScanSameNamePairResult {
                primary_id: a,
                secondary_id: b,
                status: "skipped-existing-open".to_string(),
                merge_candidate_id: None,
            });
            continue;
        }

        let result = repo.create(
            now_utc,
            preferred,
            contact.id,
            MergeCandidateCreate {
                reason: REASON.to_string(),
                source: Some(SOURCE.to_string()),
                preferred_contact_id: Some(preferred),
            },
        )?;
        if result.created {
            report.candidates_created += 1;
            open_pairs.insert(key);
        }
        group.pairs.push(MergeScanSameNamePairResult {
            primary_id: preferred.to_string(),
            secondary_id: contact.id.to_string(),
            status: if result.created {
                "created".to_string()
            } else {
                "existing".to_string()
            },
            merge_candidate_id: Some(result.candidate.id.to_string()),
        });
    }

    Ok(group)
}

fn normalize_display_name(value: &str) -> String {
    // Normalize whitespace and lowercase for grouping. (We only treat empty/whitespace as invalid.)
    let mut out = String::new();
    for part in value.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(part);
    }
    out.to_lowercase()
}

fn pair_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn choose_preferred_contact(items: &[Contact]) -> ContactId {
    // Heuristic: prefer active; then "richer" (more key identifiers); then newest update; then
    // oldest created (stable canonical record).
    let mut candidates: Vec<&Contact> = items.iter().filter(|c| c.archived_at.is_none()).collect();
    if candidates.is_empty() {
        candidates = items.iter().collect();
    }

    candidates
        .into_iter()
        .max_by(|a, b| {
            let a_score = identity_score(a);
            let b_score = identity_score(b);
            a_score
                .cmp(&b_score)
                .then_with(|| a.updated_at.cmp(&b.updated_at))
                .then_with(|| b.created_at.cmp(&a.created_at)) // older created wins
        })
        .map(|c| c.id)
        .unwrap_or(items[0].id)
}

fn identity_score(c: &Contact) -> u32 {
    let mut score = 0;
    if c.email.as_deref().is_some_and(|v| !v.trim().is_empty()) {
        score += 1;
    }
    if c.phone.as_deref().is_some_and(|v| !v.trim().is_empty()) {
        score += 1;
    }
    if c.handle.as_deref().is_some_and(|v| !v.trim().is_empty()) {
        score += 1;
    }
    score
}
