use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use knotter_core::rules::DueState;
use knotter_core::time::{format_timestamp_date, format_timestamp_datetime};

use crate::app::{
    App, ConfirmState, ContactForm, Mode, NoteForm, ScheduleForm, TagEditor, TagEditorFocus,
};

pub fn draw(frame: &mut Frame<'_>, app: &App) {
    let size = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(size);

    render_header(frame, chunks[0], app);

    match &app.mode {
        Mode::Detail(_) => render_detail(frame, chunks[1], app),
        Mode::MergeList => render_merge_list(frame, chunks[1], app),
        _ => render_list(frame, chunks[1], app),
    }

    render_footer(frame, chunks[2], app);

    if app.show_help {
        render_help(frame, size);
    }

    match &app.mode {
        Mode::ModalAddContact(form) => render_contact_form(frame, size, "Add Contact", form),
        Mode::ModalEditContact(form) => render_contact_form(frame, size, "Edit Contact", form),
        Mode::ModalAddNote(form) => render_note_form(frame, size, form),
        Mode::ModalEditTags(editor) => render_tag_editor(frame, size, editor),
        Mode::ModalSchedule(form) => render_schedule_form(frame, size, form),
        Mode::Confirm(state) => render_confirm(frame, size, state),
        _ => {}
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let filter_display = if app.filter_input.trim().is_empty() {
        "(none)".to_string()
    } else {
        app.filter_input.clone()
    };
    let title = format!(
        "knotter  contacts: {}  filter: {}",
        app.contacts.len(),
        filter_display
    );
    let mut lines = vec![Line::from(title)];
    if let Some(err) = &app.filter_error {
        lines.push(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        )));
    }

    let block = Block::default().borders(Borders::ALL).title("knotter");
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let hint = match app.mode {
        Mode::List => "j/k move  enter detail  / filter  a add  e edit  n note  t tags  s schedule  x clear  A archive  v archived  m merges  ? help",
        Mode::Detail(_) => "esc back  j/k scroll  e edit  n note  t tags  s schedule  x clear  A archive  m merges  ? help",
        Mode::MergeList => "j/k move  enter merge  p prefer  d dismiss  r refresh  esc back",
        Mode::FilterEditing => "enter apply  esc cancel",
        Mode::ModalAddContact(_) | Mode::ModalEditContact(_) => {
            "tab next  shift+tab prev  enter select  ctrl+n set now  esc cancel"
        }
        Mode::ModalSchedule(_) => "tab next  shift+tab prev  enter select  ctrl+n set now  esc cancel",
        _ => "tab next  shift+tab prev  enter select  esc cancel",
    };

    let mut lines = vec![Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    ))];

    if let Some(status) = &app.status {
        lines.push(Line::from(Span::styled(
            status.clone(),
            Style::default().fg(Color::Green),
        )));
    }
    if let Some(err) = &app.error {
        lines.push(Line::from(Span::styled(
            err.clone(),
            Style::default().fg(Color::Red),
        )));
    }

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn render_list(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if app.contacts.is_empty() {
        let paragraph = Paragraph::new(app.empty_hint())
            .block(Block::default().borders(Borders::ALL).title("Contacts"))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = app
        .contacts
        .iter()
        .map(|contact| {
            let (label, style) = due_badge(contact.due_state);
            let due_span = Span::styled(format!("[{}]", label), style);
            let next = contact
                .next_touchpoint_at
                .map(format_timestamp_date)
                .unwrap_or_else(|| "-".to_string());
            let tags = if contact.tags.is_empty() {
                "".to_string()
            } else {
                contact
                    .tags
                    .iter()
                    .map(|tag| format!("#{}", tag))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let name_style = if contact.archived_at.is_some() {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            let archived_badge = if contact.archived_at.is_some() {
                Some(Span::styled(
                    "[archived]",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ))
            } else {
                None
            };
            let mut spans = vec![
                Span::styled(contact.display_name.clone(), name_style),
                Span::raw(" "),
            ];
            if let Some(badge) = archived_badge {
                spans.push(badge);
                spans.push(Span::raw(" "));
            }
            spans.push(due_span);
            spans.push(Span::raw("  "));
            spans.push(Span::raw(next));
            spans.push(Span::raw("  "));
            spans.push(Span::styled(tags, Style::default().fg(Color::DarkGray)));
            let line = Line::from(spans);
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.selected));

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Contacts"))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➤ ");

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_merge_list(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if app.merge_candidates.is_empty() {
        let paragraph = Paragraph::new("No merge candidates.")
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Merge candidates"),
            )
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = app
        .merge_candidates
        .iter()
        .map(|candidate| {
            let preferred = candidate
                .preferred_contact_id
                .map(|id| {
                    if id == candidate.contact_a_id {
                        candidate.contact_a_name.as_str()
                    } else {
                        candidate.contact_b_name.as_str()
                    }
                })
                .unwrap_or("?");
            let line = Line::from(vec![
                Span::styled(
                    format!(
                        "{} <-> {}",
                        candidate.contact_a_name, candidate.contact_b_name
                    ),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    candidate.reason.clone(),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("preferred: {}", preferred),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.merge_selected));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Merge candidates"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➤ ");

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(detail) = &app.detail else {
        let paragraph = Paragraph::new("Loading...")
            .block(Block::default().borders(Borders::ALL).title("Detail"));
        frame.render_widget(paragraph, area);
        return;
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(9), Constraint::Min(4)])
        .split(area);

    let emails = if !detail.emails.is_empty() {
        detail.emails.join(", ")
    } else {
        detail.email.clone().unwrap_or_else(|| "-".to_string())
    };
    let mut info_lines = vec![
        Line::from(vec![Span::styled(
            detail.display_name.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )]),
        Line::from(format!("Emails: {}", emails)),
        Line::from(format!(
            "Phone: {}",
            detail.phone.clone().unwrap_or_else(|| "-".to_string())
        )),
        Line::from(format!(
            "Handle: {}",
            detail.handle.clone().unwrap_or_else(|| "-".to_string())
        )),
        Line::from(format!(
            "Timezone: {}",
            detail.timezone.clone().unwrap_or_else(|| "-".to_string())
        )),
        Line::from(format!(
            "Cadence: {}",
            detail
                .cadence_days
                .map(|value| format!("{} days", value))
                .unwrap_or_else(|| "-".to_string())
        )),
        Line::from(format!(
            "Next touchpoint: {}",
            detail
                .next_touchpoint_at
                .map(format_timestamp_date)
                .unwrap_or_else(|| "-".to_string())
        )),
        Line::from(format!(
            "Archived: {}",
            detail
                .archived_at
                .map(format_timestamp_date)
                .unwrap_or_else(|| "-".to_string())
        )),
    ];

    if !detail.tags.is_empty() {
        info_lines.push(Line::from(format!(
            "Tags: {}",
            detail
                .tags
                .iter()
                .map(|tag| format!("#{}", tag))
                .collect::<Vec<_>>()
                .join(" ")
        )));
    }

    let info =
        Paragraph::new(info_lines).block(Block::default().borders(Borders::ALL).title("Contact"));
    frame.render_widget(info, chunks[0]);

    let mut interaction_lines = Vec::new();
    if detail.recent_interactions.is_empty() {
        interaction_lines.push(Line::from("No interactions yet."));
    } else {
        for interaction in &detail.recent_interactions {
            let when = format_timestamp_datetime(interaction.occurred_at);
            let header = Line::from(vec![
                Span::styled(when, Style::default().fg(Color::Yellow)),
                Span::raw(" "),
                Span::styled(interaction.kind.clone(), Style::default().fg(Color::Cyan)),
            ]);
            interaction_lines.push(header);
            if !interaction.note.trim().is_empty() {
                interaction_lines.push(Line::from(Span::raw(interaction.note.clone())));
            }
            interaction_lines.push(Line::from(""));
        }
    }

    let interactions = Paragraph::new(Text::from(interaction_lines))
        .block(Block::default().borders(Borders::ALL).title("Interactions"))
        .scroll((app.detail_scroll as u16, 0))
        .wrap(Wrap { trim: true });
    frame.render_widget(interactions, chunks[1]);
}

fn render_contact_form(frame: &mut Frame<'_>, area: Rect, title: &str, form: &ContactForm) {
    let modal = centered_rect(70, 70, area);
    frame.render_widget(Clear, modal);

    let block = Block::default().borders(Borders::ALL).title(title);
    let mut lines = vec![
        field_line("Name", &form.name, form.focus == 0),
        field_line("Emails", &form.emails, form.focus == 1),
        field_line("Phone", &form.phone, form.focus == 2),
        field_line("Handle", &form.handle, form.focus == 3),
        field_line("Timezone", &form.timezone, form.focus == 4),
        field_line("Cadence days", &form.cadence_days, form.focus == 5),
        field_line(
            "Next touchpoint (YYYY-MM-DD or YYYY-MM-DD HH:MM)",
            &form.next_touchpoint_at,
            form.focus == 6,
        ),
        Line::from(Span::styled(
            "Must be now or later. Ctrl+N sets to now.",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
    ];

    let save_style = if form.is_save_focus() {
        Style::default().fg(Color::Black).bg(Color::LightGreen)
    } else {
        Style::default().fg(Color::Green)
    };
    let cancel_style = if form.is_cancel_focus() {
        Style::default().fg(Color::Black).bg(Color::LightRed)
    } else {
        Style::default().fg(Color::Red)
    };

    lines.push(Line::from(vec![
        Span::styled("[Save]", save_style),
        Span::raw("  "),
        Span::styled("[Cancel]", cancel_style),
    ]));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, modal);
}

fn render_note_form(frame: &mut Frame<'_>, area: Rect, form: &NoteForm) {
    let modal = centered_rect(70, 70, area);
    frame.render_widget(Clear, modal);

    let block = Block::default().borders(Borders::ALL).title("Add Note");
    let mut lines = Vec::new();
    lines.push(field_line("Kind", &form.kind, form.focus == 0));
    lines.push(field_line("When (optional)", &form.when, form.focus == 1));
    lines.push(Line::from("Note:"));

    let note_style = if form.is_note_focus() {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };

    let note_lines: Vec<Line> = if form.note.is_empty() {
        vec![Line::from(Span::styled("(empty)", note_style))]
    } else {
        form.note
            .lines()
            .map(|line| Line::from(Span::styled(line.to_string(), note_style)))
            .collect()
    };

    lines.extend(note_lines);
    lines.push(Line::from(""));

    let save_style = if form.is_save_focus() {
        Style::default().fg(Color::Black).bg(Color::LightGreen)
    } else {
        Style::default().fg(Color::Green)
    };
    let cancel_style = if form.is_cancel_focus() {
        Style::default().fg(Color::Black).bg(Color::LightRed)
    } else {
        Style::default().fg(Color::Red)
    };

    lines.push(Line::from(vec![
        Span::styled("[Save]", save_style),
        Span::raw("  "),
        Span::styled("[Cancel]", cancel_style),
    ]));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, modal);
}

fn render_tag_editor(frame: &mut Frame<'_>, area: Rect, editor: &TagEditor) {
    let modal = centered_rect(70, 70, area);
    frame.render_widget(Clear, modal);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(modal);

    let filter_line = field_line(
        "Filter",
        &editor.filter,
        editor.focus == TagEditorFocus::Filter,
    );
    let filter_block = Block::default().borders(Borders::ALL).title("Tags");
    frame.render_widget(Paragraph::new(filter_line).block(filter_block), chunks[0]);

    let items: Vec<ListItem> = editor
        .filtered
        .iter()
        .map(|idx| &editor.tags[*idx])
        .map(|tag| {
            let marker = if tag.selected { "[x]" } else { "[ ]" };
            ListItem::new(Line::from(format!(
                "{} {} ({})",
                marker, tag.name, tag.count
            )))
        })
        .collect();

    let mut state = ListState::default();
    if !editor.filtered.is_empty() {
        state.select(Some(editor.selected_index));
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Tag List"))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("➤ ");

    frame.render_stateful_widget(list, chunks[1], &mut state);

    let save_style = if editor.focus == TagEditorFocus::Save {
        Style::default().fg(Color::Black).bg(Color::LightGreen)
    } else {
        Style::default().fg(Color::Green)
    };
    let cancel_style = if editor.focus == TagEditorFocus::Cancel {
        Style::default().fg(Color::Black).bg(Color::LightRed)
    } else {
        Style::default().fg(Color::Red)
    };

    let buttons = Paragraph::new(Line::from(vec![
        Span::styled("[Save]", save_style),
        Span::raw("  "),
        Span::styled("[Cancel]", cancel_style),
    ]))
    .block(Block::default().borders(Borders::ALL));

    frame.render_widget(buttons, chunks[2]);
}

fn render_schedule_form(frame: &mut Frame<'_>, area: Rect, form: &ScheduleForm) {
    let modal = centered_rect(60, 50, area);
    frame.render_widget(Clear, modal);

    let block = Block::default()
        .borders(Borders::ALL)
        .title("Schedule Touchpoint");
    let mut lines = vec![
        field_line("Date (YYYY-MM-DD)", &form.date, form.focus == 0),
        field_line("Time (HH:MM)", &form.time, form.focus == 1),
        Line::from(Span::styled(
            "Must be now or later. Ctrl+N sets to now.",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
    ];

    let save_style = if form.is_save_focus() {
        Style::default().fg(Color::Black).bg(Color::LightGreen)
    } else {
        Style::default().fg(Color::Green)
    };
    let cancel_style = if form.is_cancel_focus() {
        Style::default().fg(Color::Black).bg(Color::LightRed)
    } else {
        Style::default().fg(Color::Red)
    };

    lines.push(Line::from(vec![
        Span::styled("[Save]", save_style),
        Span::raw("  "),
        Span::styled("[Cancel]", cancel_style),
    ]));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, modal);
}

fn render_confirm(frame: &mut Frame<'_>, area: Rect, state: &ConfirmState) {
    let modal = centered_rect(50, 30, area);
    frame.render_widget(Clear, modal);
    let paragraph = Paragraph::new(state.message.clone())
        .block(Block::default().borders(Borders::ALL).title("Confirm"))
        .alignment(Alignment::Center);
    frame.render_widget(paragraph, modal);
}

fn render_help(frame: &mut Frame<'_>, area: Rect) {
    let modal = centered_rect(70, 80, area);
    frame.render_widget(Clear, modal);

    let text = vec![
        Line::from("Global: q quit, Ctrl+C quit, ? help"),
        Line::from("List: j/k move, enter detail, / filter, a add, e edit, n note, t tags, s schedule, x clear, A archive, v archived, m merges"),
        Line::from("Filter: enter apply, esc cancel"),
        Line::from("Detail: esc back, j/k scroll, e edit, n note, t tags, s schedule, x clear, A archive, m merges"),
        Line::from("Merge: j/k move, enter merge, p prefer, d dismiss, r refresh, esc back"),
        Line::from("Modals: tab/shift+tab move, enter activate, esc cancel, Ctrl+N set now (contact/schedule)"),
        Line::from(""),
        Line::from("Filter syntax: #tag, due:overdue|today|soon|any|none, archived:true|false, text matches name/email/phone/handle"),
    ];

    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Help"))
        .wrap(Wrap { trim: true });
    frame.render_widget(paragraph, modal);
}

fn field_line(label: &str, value: &str, focused: bool) -> Line<'static> {
    let style = if focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    Line::from(vec![
        Span::styled(
            format!("{}: ", label),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled(value.to_string(), style),
    ])
}

fn due_badge(state: DueState) -> (&'static str, Style) {
    match state {
        DueState::Overdue => (
            "overdue",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        DueState::Today => (
            "today",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        DueState::Soon => ("soon", Style::default().fg(Color::Magenta)),
        DueState::Scheduled => ("scheduled", Style::default().fg(Color::Blue)),
        DueState::Unscheduled => ("unscheduled", Style::default().fg(Color::DarkGray)),
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, rect: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(rect);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
