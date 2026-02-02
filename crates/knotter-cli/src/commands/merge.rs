use crate::commands::{print_json, Context};
use crate::error::{invalid_input, not_found};
use anyhow::Result;
use clap::{ArgAction, Args, Subcommand, ValueEnum};
use knotter_core::domain::{Contact, ContactId, MergeCandidateId, MergeCandidateReason};
use knotter_store::repo::{
    ContactMergeOptions, MergeArchivedPreference, MergeCandidate, MergeCandidateStatus,
    MergePreference, MergeTouchpointPreference,
};
use serde::Serialize;
use std::str::FromStr;

#[derive(Debug, Subcommand)]
pub enum MergeCommand {
    List(MergeListArgs),
    Show(MergeShowArgs),
    Apply(MergeApplyArgs),
    ApplyAll(MergeApplyAllArgs),
    Dismiss(MergeDismissArgs),
    Contacts(MergeContactsArgs),
}

#[derive(Debug, Args)]
pub struct MergeListArgs {
    #[arg(long, value_enum)]
    pub status: Option<MergeStatusArg>,
}

#[derive(Debug, Args)]
pub struct MergeShowArgs {
    pub id: String,
}

#[derive(Debug, Args)]
pub struct MergeApplyArgs {
    pub id: String,
    #[arg(long, value_enum)]
    pub prefer: Option<MergePreferArg>,
    #[arg(long, value_enum)]
    pub touchpoint: Option<MergeTouchpointArg>,
    #[arg(long, value_enum)]
    pub archived: Option<MergeArchivedArg>,
}

#[derive(Debug, Args)]
pub struct MergeApplyAllArgs {
    #[arg(long, value_enum)]
    pub prefer: Option<MergePreferArg>,
    #[arg(long, value_enum)]
    pub touchpoint: Option<MergeTouchpointArg>,
    #[arg(long, value_enum)]
    pub archived: Option<MergeArchivedArg>,
    #[arg(long, value_enum, action = ArgAction::Append)]
    pub reason: Vec<MergeReasonArg>,
    #[arg(long)]
    pub source: Option<String>,
    #[arg(long, help = "Include candidates that are not marked auto-merge safe")]
    pub include_unsafe: bool,
    #[arg(long)]
    pub limit: Option<usize>,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long, help = "Skip confirmation for bulk apply")]
    pub yes: bool,
}

#[derive(Debug, Args)]
pub struct MergeDismissArgs {
    pub id: String,
}

#[derive(Debug, Args)]
pub struct MergeContactsArgs {
    pub primary_id: String,
    pub secondary_id: String,
    #[arg(long, value_enum)]
    pub prefer: Option<MergePreferArg>,
    #[arg(long, value_enum)]
    pub touchpoint: Option<MergeTouchpointArg>,
    #[arg(long, value_enum)]
    pub archived: Option<MergeArchivedArg>,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum MergeStatusArg {
    Open,
    Merged,
    Dismissed,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum MergePreferArg {
    Primary,
    Secondary,
    A,
    B,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum MergeTouchpointArg {
    Primary,
    Secondary,
    Earliest,
    Latest,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum MergeArchivedArg {
    ActiveIfAny,
    Primary,
    Secondary,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum MergeReasonArg {
    EmailDuplicate,
    EmailNameAmbiguous,
    VcfAmbiguousEmail,
    VcfAmbiguousPhoneName,
    TelegramUsernameAmbiguous,
    TelegramHandleAmbiguous,
    TelegramNameAmbiguous,
}

#[derive(Debug, Serialize)]
struct MergeCandidateDto {
    id: String,
    created_at: i64,
    status: String,
    reason: String,
    auto_merge_safe: bool,
    source: Option<String>,
    preferred_contact_id: Option<String>,
    resolved_at: Option<i64>,
    contact_a: ContactSummaryDto,
    contact_b: ContactSummaryDto,
}

#[derive(Debug, Serialize)]
struct ContactSummaryDto {
    id: String,
    display_name: String,
    email: Option<String>,
    archived_at: Option<i64>,
    updated_at: i64,
}

#[derive(Debug, Serialize)]
struct MergeApplyAllReport {
    considered: usize,
    selected: usize,
    applied: usize,
    skipped: usize,
    failed: usize,
    dry_run: bool,
    results: Vec<MergeApplyAllResult>,
}

#[derive(Debug, Serialize)]
struct MergeApplyAllResult {
    id: String,
    status: String,
    reason: String,
    source: Option<String>,
    primary_id: Option<String>,
    secondary_id: Option<String>,
    merged_contact_id: Option<String>,
    error: Option<String>,
}

pub fn list_merges(ctx: &Context<'_>, args: MergeListArgs) -> Result<()> {
    let status = args.status.map(status_from_arg);
    let candidates = ctx.store.merge_candidates().list(status)?;
    if ctx.json {
        let dtos = build_candidate_dtos(ctx, &candidates)?;
        return print_json(&dtos);
    }

    if candidates.is_empty() {
        println!("No merge candidates.");
        return Ok(());
    }

    let dtos = build_candidate_dtos(ctx, &candidates)?;
    for dto in dtos {
        println!(
            "{}  {}  {}  {} <-> {}{}",
            dto.id,
            dto.status,
            dto.reason,
            dto.contact_a.display_name,
            dto.contact_b.display_name,
            dto.preferred_contact_id
                .as_ref()
                .map(|id| format!(" (preferred {id})"))
                .unwrap_or_default()
        );
    }
    Ok(())
}

pub fn show_merge(ctx: &Context<'_>, args: MergeShowArgs) -> Result<()> {
    let id = parse_merge_candidate_id(&args.id)?;
    let candidate = ctx
        .store
        .merge_candidates()
        .get(id)?
        .ok_or_else(|| not_found("merge candidate not found"))?;
    let dto = build_candidate_dto(ctx, &candidate)?;
    if ctx.json {
        return print_json(&dto);
    }
    print_candidate_human(&dto);
    Ok(())
}

pub fn apply_merge(ctx: &Context<'_>, args: MergeApplyArgs) -> Result<()> {
    let id = parse_merge_candidate_id(&args.id)?;
    let options = build_merge_options_for_apply(args.touchpoint, args.archived)?;
    let now = crate::util::now_utc();
    let tx = ctx.store.connection().unchecked_transaction()?;
    let candidate = knotter_store::repo::MergeCandidatesRepo::new(&tx)
        .get(id)?
        .ok_or_else(|| not_found("merge candidate not found"))?;

    if candidate.status != MergeCandidateStatus::Open {
        return Err(invalid_input("merge candidate is not open"));
    }

    let (primary_id, secondary_id) = select_primary_secondary(&candidate, args.prefer)?;
    let merged = knotter_store::repo::ContactsRepo::new(&tx).merge_contacts(
        now,
        primary_id,
        secondary_id,
        options,
    )?;
    tx.commit()?;

    if ctx.json {
        return print_json(&merged);
    }
    println!("Merged {} into {}", secondary_id, primary_id);
    Ok(())
}

pub fn apply_all_merges(ctx: &Context<'_>, args: MergeApplyAllArgs) -> Result<()> {
    let mut candidates = ctx.store.merge_candidates().list_open()?;
    let considered = candidates.len();
    let reason_filter = if args.reason.is_empty() {
        None
    } else {
        let values: std::collections::HashSet<MergeCandidateReason> =
            args.reason.iter().map(|arg| arg.as_reason()).collect();
        Some(values)
    };

    candidates.retain(|candidate| {
        if let Some(reasons) = &reason_filter {
            if let Some(kind) = candidate.reason_kind() {
                if !reasons.contains(&kind) {
                    return false;
                }
            } else {
                return false;
            }
        }
        if let Some(source) = args.source.as_deref() {
            if candidate.source.as_deref() != Some(source) {
                return false;
            }
        }
        if !args.include_unsafe && !candidate.auto_merge_safe() {
            return false;
        }
        true
    });

    if let Some(limit) = args.limit {
        candidates.truncate(limit);
    }

    let selected = candidates.len();
    if selected == 0 {
        let report = MergeApplyAllReport {
            considered,
            selected,
            applied: 0,
            skipped: 0,
            failed: 0,
            dry_run: args.dry_run,
            results: Vec::new(),
        };
        if ctx.json {
            return print_json(&report);
        }
        println!("No merge candidates matched the filters.");
        return Ok(());
    }

    if !args.dry_run && !args.yes {
        return Err(invalid_input(
            "merge apply-all requires --yes unless --dry-run is set",
        ));
    }

    let options = build_merge_options_for_apply(args.touchpoint, args.archived)?;
    let mut report = MergeApplyAllReport {
        considered,
        selected,
        applied: 0,
        skipped: 0,
        failed: 0,
        dry_run: args.dry_run,
        results: Vec::new(),
    };

    if args.dry_run {
        for candidate in candidates {
            match select_primary_secondary(&candidate, args.prefer.clone()) {
                Ok((primary_id, secondary_id)) => {
                    report.results.push(MergeApplyAllResult {
                        id: candidate.id.to_string(),
                        status: "dry-run".to_string(),
                        reason: candidate.reason,
                        source: candidate.source,
                        primary_id: Some(primary_id.to_string()),
                        secondary_id: Some(secondary_id.to_string()),
                        merged_contact_id: None,
                        error: None,
                    });
                }
                Err(err) => {
                    report.failed += 1;
                    report.results.push(MergeApplyAllResult {
                        id: candidate.id.to_string(),
                        status: "failed".to_string(),
                        reason: candidate.reason,
                        source: candidate.source,
                        primary_id: None,
                        secondary_id: None,
                        merged_contact_id: None,
                        error: Some(err.to_string()),
                    });
                }
            }
        }

        if ctx.json {
            return print_json(&report);
        }
        println!(
            "Dry-run: {} candidate(s) selected ({} considered).",
            report.selected, report.considered
        );
        for result in &report.results {
            let primary = result.primary_id.as_deref().unwrap_or("?");
            let secondary = result.secondary_id.as_deref().unwrap_or("?");
            println!("{}  {} -> {}", result.id, secondary, primary);
        }
        return Ok(());
    }

    let now = crate::util::now_utc();
    for candidate in candidates {
        let tx = ctx.store.connection().unchecked_transaction()?;
        let current = knotter_store::repo::MergeCandidatesRepo::new(&tx).get(candidate.id)?;
        let Some(current) = current else {
            report.skipped += 1;
            report.results.push(MergeApplyAllResult {
                id: candidate.id.to_string(),
                status: "skipped".to_string(),
                reason: candidate.reason,
                source: candidate.source,
                primary_id: None,
                secondary_id: None,
                merged_contact_id: None,
                error: Some("merge candidate not found".to_string()),
            });
            continue;
        };

        if current.status != MergeCandidateStatus::Open {
            report.skipped += 1;
            report.results.push(MergeApplyAllResult {
                id: current.id.to_string(),
                status: "skipped".to_string(),
                reason: current.reason,
                source: current.source,
                primary_id: None,
                secondary_id: None,
                merged_contact_id: None,
                error: Some("merge candidate is not open".to_string()),
            });
            continue;
        }

        if !args.include_unsafe && !current.auto_merge_safe() {
            report.skipped += 1;
            report.results.push(MergeApplyAllResult {
                id: current.id.to_string(),
                status: "skipped".to_string(),
                reason: current.reason,
                source: current.source,
                primary_id: None,
                secondary_id: None,
                merged_contact_id: None,
                error: Some("merge candidate is not auto-merge safe".to_string()),
            });
            continue;
        }

        let (primary_id, secondary_id) =
            match select_primary_secondary(&current, args.prefer.clone()) {
                Ok(value) => value,
                Err(err) => {
                    report.failed += 1;
                    report.results.push(MergeApplyAllResult {
                        id: current.id.to_string(),
                        status: "failed".to_string(),
                        reason: current.reason,
                        source: current.source,
                        primary_id: None,
                        secondary_id: None,
                        merged_contact_id: None,
                        error: Some(err.to_string()),
                    });
                    continue;
                }
            };

        let merged = knotter_store::repo::ContactsRepo::new(&tx).merge_contacts(
            now,
            primary_id,
            secondary_id,
            options.clone(),
        );
        match merged {
            Ok(merged) => {
                tx.commit()?;
                report.applied += 1;
                report.results.push(MergeApplyAllResult {
                    id: current.id.to_string(),
                    status: "merged".to_string(),
                    reason: current.reason,
                    source: current.source,
                    primary_id: Some(primary_id.to_string()),
                    secondary_id: Some(secondary_id.to_string()),
                    merged_contact_id: Some(merged.id.to_string()),
                    error: None,
                });
            }
            Err(err) => {
                report.failed += 1;
                report.results.push(MergeApplyAllResult {
                    id: current.id.to_string(),
                    status: "failed".to_string(),
                    reason: current.reason,
                    source: current.source,
                    primary_id: Some(primary_id.to_string()),
                    secondary_id: Some(secondary_id.to_string()),
                    merged_contact_id: None,
                    error: Some(err.to_string()),
                });
            }
        }
    }

    if ctx.json {
        return print_json(&report);
    }
    println!(
        "Applied {} merge(s); skipped {}; failed {}.",
        report.applied, report.skipped, report.failed
    );
    for result in &report.results {
        let primary = result.primary_id.as_deref().unwrap_or("?");
        let secondary = result.secondary_id.as_deref().unwrap_or("?");
        println!(
            "{}  {}  {} -> {}",
            result.id, result.status, secondary, primary
        );
    }
    Ok(())
}

pub fn dismiss_merge(ctx: &Context<'_>, args: MergeDismissArgs) -> Result<()> {
    let id = parse_merge_candidate_id(&args.id)?;
    let candidate = ctx
        .store
        .merge_candidates()
        .get(id)?
        .ok_or_else(|| not_found("merge candidate not found"))?;
    if candidate.status != MergeCandidateStatus::Open {
        return Err(invalid_input("merge candidate is not open"));
    }
    let candidate = ctx
        .store
        .merge_candidates()
        .dismiss(crate::util::now_utc(), id)?;
    if ctx.json {
        return print_json(&candidate_to_dto(ctx, candidate)?);
    }
    println!("Dismissed merge candidate {}", id);
    Ok(())
}

pub fn merge_contacts(ctx: &Context<'_>, args: MergeContactsArgs) -> Result<()> {
    let primary_id = parse_contact_id(&args.primary_id)?;
    let secondary_id = parse_contact_id(&args.secondary_id)?;
    let options = build_merge_options(args.prefer, args.touchpoint, args.archived)?;
    let merged = ctx.store.contacts().merge_contacts(
        crate::util::now_utc(),
        primary_id,
        secondary_id,
        options,
    )?;
    if ctx.json {
        return print_json(&merged);
    }
    println!("Merged {} into {}", secondary_id, primary_id);
    Ok(())
}

fn build_candidate_dtos(
    ctx: &Context<'_>,
    candidates: &[MergeCandidate],
) -> Result<Vec<MergeCandidateDto>> {
    let mut dtos = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        dtos.push(build_candidate_dto(ctx, candidate)?);
    }
    Ok(dtos)
}

fn build_candidate_dto(ctx: &Context<'_>, candidate: &MergeCandidate) -> Result<MergeCandidateDto> {
    let contact_a = load_contact_summary(ctx, candidate.contact_a_id)?;
    let contact_b = load_contact_summary(ctx, candidate.contact_b_id)?;
    Ok(candidate_to_dto_with_summaries(
        candidate.clone(),
        contact_a,
        contact_b,
    ))
}

fn candidate_to_dto(ctx: &Context<'_>, candidate: MergeCandidate) -> Result<MergeCandidateDto> {
    let contact_a = load_contact_summary(ctx, candidate.contact_a_id)?;
    let contact_b = load_contact_summary(ctx, candidate.contact_b_id)?;
    Ok(candidate_to_dto_with_summaries(
        candidate, contact_a, contact_b,
    ))
}

fn candidate_to_dto_with_summaries(
    candidate: MergeCandidate,
    contact_a: ContactSummaryDto,
    contact_b: ContactSummaryDto,
) -> MergeCandidateDto {
    let auto_merge_safe = candidate.auto_merge_safe();
    MergeCandidateDto {
        id: candidate.id.to_string(),
        created_at: candidate.created_at,
        status: candidate.status.as_str().to_string(),
        reason: candidate.reason,
        auto_merge_safe,
        source: candidate.source,
        preferred_contact_id: candidate.preferred_contact_id.map(|id| id.to_string()),
        resolved_at: candidate.resolved_at,
        contact_a,
        contact_b,
    }
}

fn load_contact_summary(ctx: &Context<'_>, id: ContactId) -> Result<ContactSummaryDto> {
    match ctx.store.contacts().get(id)? {
        Some(contact) => Ok(contact_summary(contact)),
        None => Ok(ContactSummaryDto {
            id: id.to_string(),
            display_name: "<missing contact>".to_string(),
            email: None,
            archived_at: None,
            updated_at: 0,
        }),
    }
}

fn contact_summary(contact: Contact) -> ContactSummaryDto {
    ContactSummaryDto {
        id: contact.id.to_string(),
        display_name: contact.display_name,
        email: contact.email,
        archived_at: contact.archived_at,
        updated_at: contact.updated_at,
    }
}

fn status_from_arg(status: MergeStatusArg) -> MergeCandidateStatus {
    match status {
        MergeStatusArg::Open => MergeCandidateStatus::Open,
        MergeStatusArg::Merged => MergeCandidateStatus::Merged,
        MergeStatusArg::Dismissed => MergeCandidateStatus::Dismissed,
    }
}

fn build_merge_options(
    prefer: Option<MergePreferArg>,
    touchpoint: Option<MergeTouchpointArg>,
    archived: Option<MergeArchivedArg>,
) -> Result<ContactMergeOptions> {
    let mut options = ContactMergeOptions::default();
    if let Some(prefer) = prefer {
        match prefer {
            MergePreferArg::Secondary | MergePreferArg::B => {
                options.prefer = MergePreference::Secondary;
            }
            MergePreferArg::Primary | MergePreferArg::A => {
                options.prefer = MergePreference::Primary;
            }
        }
    }
    if let Some(touchpoint) = touchpoint {
        options.touchpoint = match touchpoint {
            MergeTouchpointArg::Primary => MergeTouchpointPreference::Primary,
            MergeTouchpointArg::Secondary => MergeTouchpointPreference::Secondary,
            MergeTouchpointArg::Earliest => MergeTouchpointPreference::Earliest,
            MergeTouchpointArg::Latest => MergeTouchpointPreference::Latest,
        };
    }
    if let Some(archived) = archived {
        options.archived = match archived {
            MergeArchivedArg::ActiveIfAny => MergeArchivedPreference::ActiveIfAny,
            MergeArchivedArg::Primary => MergeArchivedPreference::Primary,
            MergeArchivedArg::Secondary => MergeArchivedPreference::Secondary,
        };
    }
    Ok(options)
}

fn build_merge_options_for_apply(
    touchpoint: Option<MergeTouchpointArg>,
    archived: Option<MergeArchivedArg>,
) -> Result<ContactMergeOptions> {
    let mut options = ContactMergeOptions::default();
    if let Some(touchpoint) = touchpoint {
        options.touchpoint = match touchpoint {
            MergeTouchpointArg::Primary => MergeTouchpointPreference::Primary,
            MergeTouchpointArg::Secondary => MergeTouchpointPreference::Secondary,
            MergeTouchpointArg::Earliest => MergeTouchpointPreference::Earliest,
            MergeTouchpointArg::Latest => MergeTouchpointPreference::Latest,
        };
    }
    if let Some(archived) = archived {
        options.archived = match archived {
            MergeArchivedArg::ActiveIfAny => MergeArchivedPreference::ActiveIfAny,
            MergeArchivedArg::Primary => MergeArchivedPreference::Primary,
            MergeArchivedArg::Secondary => MergeArchivedPreference::Secondary,
        };
    }
    Ok(options)
}

fn select_primary_secondary(
    candidate: &MergeCandidate,
    prefer: Option<MergePreferArg>,
) -> Result<(ContactId, ContactId)> {
    let default_primary = candidate
        .preferred_contact_id
        .unwrap_or(candidate.contact_a_id);
    let default_secondary = if default_primary == candidate.contact_a_id {
        candidate.contact_b_id
    } else {
        candidate.contact_a_id
    };

    let (primary, secondary) = match prefer {
        Some(MergePreferArg::A) => (candidate.contact_a_id, candidate.contact_b_id),
        Some(MergePreferArg::B) => (candidate.contact_b_id, candidate.contact_a_id),
        Some(MergePreferArg::Primary) => (default_primary, default_secondary),
        Some(MergePreferArg::Secondary) => (default_secondary, default_primary),
        None => (default_primary, default_secondary),
    };

    if primary == secondary {
        return Err(invalid_input("merge candidate references the same contact"));
    }
    Ok((primary, secondary))
}

fn parse_merge_candidate_id(value: &str) -> Result<MergeCandidateId> {
    MergeCandidateId::from_str(value).map_err(|_| invalid_input("invalid merge candidate id"))
}

impl MergeReasonArg {
    fn as_reason(&self) -> MergeCandidateReason {
        match self {
            MergeReasonArg::EmailDuplicate => MergeCandidateReason::EmailDuplicate,
            MergeReasonArg::EmailNameAmbiguous => MergeCandidateReason::EmailNameAmbiguous,
            MergeReasonArg::VcfAmbiguousEmail => MergeCandidateReason::VcfAmbiguousEmail,
            MergeReasonArg::VcfAmbiguousPhoneName => MergeCandidateReason::VcfAmbiguousPhoneName,
            MergeReasonArg::TelegramUsernameAmbiguous => {
                MergeCandidateReason::TelegramUsernameAmbiguous
            }
            MergeReasonArg::TelegramHandleAmbiguous => {
                MergeCandidateReason::TelegramHandleAmbiguous
            }
            MergeReasonArg::TelegramNameAmbiguous => MergeCandidateReason::TelegramNameAmbiguous,
        }
    }
}

fn parse_contact_id(value: &str) -> Result<ContactId> {
    ContactId::from_str(value).map_err(|_| invalid_input("invalid contact id"))
}

fn print_candidate_human(dto: &MergeCandidateDto) {
    println!("id: {}", dto.id);
    println!("status: {}", dto.status);
    println!("reason: {}", dto.reason);
    println!("auto_merge_safe: {}", dto.auto_merge_safe);
    if let Some(source) = &dto.source {
        println!("source: {}", source);
    }
    if let Some(preferred) = &dto.preferred_contact_id {
        println!("preferred: {}", preferred);
    }
    println!(
        "contact_a: {} ({})",
        dto.contact_a.display_name, dto.contact_a.id
    );
    println!(
        "contact_b: {} ({})",
        dto.contact_b.display_name, dto.contact_b.id
    );
}
