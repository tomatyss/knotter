use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use knotter_core::domain::{ContactId, TagName};
use knotter_core::filter::{parse_filter, ContactFilter};
use knotter_core::rules::ensure_future_timestamp_with_precision;

use crate::actions::Action;

const LIST_EMPTY: &str = "No contacts. Press 'a' to add one.";

#[derive(Debug, Clone)]
pub enum Mode {
    List,
    FilterEditing,
    Detail(ContactId),
    ModalAddContact(ContactForm),
    ModalEditContact(ContactForm),
    ModalAddNote(NoteForm),
    ModalEditTags(TagEditor),
    ModalSchedule(ScheduleForm),
    Confirm(ConfirmState),
}

#[derive(Debug, Clone)]
pub struct App {
    pub mode: Mode,
    pub show_help: bool,
    pub should_quit: bool,
    pub filter_input: String,
    pub filter: Option<ContactFilter>,
    pub filter_error: Option<String>,
    pub contacts: Vec<knotter_core::dto::ContactListItemDto>,
    pub selected: usize,
    pub detail: Option<knotter_core::dto::ContactDetailDto>,
    pub detail_scroll: usize,
    pub status: Option<String>,
    pub error: Option<String>,
    pub soon_days: i64,
    pub default_cadence_days: Option<i32>,
    pub auto_reschedule_interactions: bool,
    pub show_archived: bool,
    pub empty_hint: &'static str,
    actions: VecDeque<Action>,
    pub(crate) pending_select: Option<ContactId>,
}

impl App {
    pub fn new(
        soon_days: i64,
        default_cadence_days: Option<i32>,
        auto_reschedule_interactions: bool,
    ) -> Self {
        let mut app = Self {
            mode: Mode::List,
            show_help: false,
            should_quit: false,
            filter_input: String::new(),
            filter: None,
            filter_error: None,
            contacts: Vec::new(),
            selected: 0,
            detail: None,
            detail_scroll: 0,
            status: None,
            error: None,
            soon_days,
            default_cadence_days,
            auto_reschedule_interactions,
            show_archived: false,
            empty_hint: LIST_EMPTY,
            actions: VecDeque::new(),
            pending_select: None,
        };
        app.enqueue(Action::LoadList);
        app
    }

    pub fn enqueue(&mut self, action: Action) {
        self.actions.push_back(action);
    }

    pub fn next_action(&mut self) -> Option<Action> {
        self.actions.pop_front()
    }

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.error = Some(message.into());
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    pub fn set_status(&mut self, message: impl Into<String>) {
        self.status = Some(message.into());
    }

    pub fn selected_contact_id(&self) -> Option<ContactId> {
        self.contacts.get(self.selected).map(|c| c.id)
    }

    pub fn apply_list(&mut self, items: Vec<knotter_core::dto::ContactListItemDto>) {
        self.contacts = items;
        if let Some(target) = self.pending_select.take() {
            if let Some(pos) = self.contacts.iter().position(|item| item.id == target) {
                self.selected = pos;
            }
        }
        if self.selected >= self.contacts.len() {
            self.selected = self.contacts.len().saturating_sub(1);
        }
    }

    pub fn apply_detail(&mut self, detail: knotter_core::dto::ContactDetailDto) {
        self.detail_scroll = 0;
        self.detail = Some(detail);
    }

    pub fn empty_hint(&self) -> String {
        self.empty_hint.to_string()
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press && key.kind != KeyEventKind::Repeat {
            return;
        }

        if self.show_help {
            if matches!(key.code, KeyCode::Char('?') | KeyCode::Esc) {
                self.show_help = false;
            }
            return;
        }

        if matches!(
            key,
            KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }
        ) {
            self.should_quit = true;
            return;
        }

        if matches!(key.code, KeyCode::Char('q')) {
            self.should_quit = true;
            return;
        }

        if matches!(key.code, KeyCode::Char('?')) {
            self.show_help = true;
            return;
        }

        let mut mode = std::mem::replace(&mut self.mode, Mode::List);
        match &mut mode {
            Mode::List => {
                if let Some(next) = self.handle_list_key(key) {
                    mode = next;
                }
            }
            Mode::FilterEditing => {
                if let Some(next) = self.handle_filter_key(key) {
                    mode = next;
                }
            }
            Mode::Detail(contact_id) => {
                if let Some(next) = self.handle_detail_key(key, *contact_id) {
                    mode = next;
                }
            }
            Mode::ModalAddContact(form) | Mode::ModalEditContact(form) => {
                if let Some(next) = self.handle_contact_form_key(form, key) {
                    mode = next;
                }
            }
            Mode::ModalAddNote(form) => {
                if let Some(next) = self.handle_note_form_key(form, key) {
                    mode = next;
                }
            }
            Mode::ModalEditTags(editor) => {
                if let Some(next) = self.handle_tag_editor_key(editor, key) {
                    mode = next;
                }
            }
            Mode::ModalSchedule(form) => {
                if let Some(next) = self.handle_schedule_form_key(form, key) {
                    mode = next;
                }
            }
            Mode::Confirm(state) => {
                if let Some(next) = self.handle_confirm_key(state, key) {
                    mode = next;
                }
            }
        }
        self.mode = mode;
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> Option<Mode> {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(5),
            KeyCode::PageUp => self.move_selection(-5),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => {
                if !self.contacts.is_empty() {
                    self.selected = self.contacts.len() - 1;
                }
            }
            KeyCode::Enter => {
                if let Some(id) = self.selected_contact_id() {
                    self.enqueue(Action::LoadDetail(id));
                    return Some(Mode::Detail(id));
                }
            }
            KeyCode::Char('/') => {
                self.filter_error = None;
                return Some(Mode::FilterEditing);
            }
            KeyCode::Char('c') => {
                self.filter_input.clear();
                self.filter = None;
                self.filter_error = None;
                self.enqueue(Action::LoadList);
            }
            KeyCode::Char('a') => {
                return Some(Mode::ModalAddContact(ContactForm::new(
                    self.default_cadence_days,
                )));
            }
            KeyCode::Char('e') => {
                if let Some(detail) = self.detail_for_selected() {
                    return Some(Mode::ModalEditContact(ContactForm::from_detail(&detail)));
                } else if let Some(id) = self.selected_contact_id() {
                    self.enqueue(Action::LoadDetail(id));
                    return Some(Mode::Detail(id));
                }
            }
            KeyCode::Char('n') => {
                if let Some(id) = self.selected_contact_id() {
                    return Some(Mode::ModalAddNote(NoteForm::new(id)));
                }
            }
            KeyCode::Char('t') => {
                if let Some(id) = self.selected_contact_id() {
                    self.enqueue(Action::LoadTags(id));
                    return Some(Mode::ModalEditTags(TagEditor::new(id)));
                }
            }
            KeyCode::Char('s') => {
                if let Some(id) = self.selected_contact_id() {
                    return Some(Mode::ModalSchedule(ScheduleForm::new(id)));
                }
            }
            KeyCode::Char('v') => {
                self.show_archived = !self.show_archived;
                let status = if self.show_archived {
                    "Showing archived contacts"
                } else {
                    "Hiding archived contacts"
                };
                self.set_status(status.to_string());
                self.enqueue(Action::LoadList);
            }
            KeyCode::Char('x') => {
                if let Some(id) = self.selected_contact_id() {
                    let message = "Clear scheduled touchpoint? (y/n)".to_string();
                    return Some(Mode::Confirm(ConfirmState::new(
                        message,
                        ConfirmAction::ClearSchedule(id),
                    )));
                }
            }
            KeyCode::Char('A') => {
                if let Some(item) = self.contacts.get(self.selected) {
                    let (message, action) = if item.archived_at.is_some() {
                        (
                            format!("Unarchive {}? (y/n)", item.display_name),
                            ConfirmAction::UnarchiveContact(item.id),
                        )
                    } else {
                        (
                            format!("Archive {}? (y/n)", item.display_name),
                            ConfirmAction::ArchiveContact(item.id),
                        )
                    };
                    return Some(Mode::Confirm(ConfirmState::new(message, action)));
                }
            }
            KeyCode::Char('r') => self.enqueue(Action::LoadList),
            _ => {}
        }
        None
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> Option<Mode> {
        match key.code {
            KeyCode::Esc => {
                self.filter_error = None;
                return Some(Mode::List);
            }
            KeyCode::Enter => {
                if self.filter_input.trim().is_empty() {
                    self.filter = None;
                    self.filter_error = None;
                    self.enqueue(Action::LoadList);
                    return Some(Mode::List);
                }
                match parse_filter(&self.filter_input) {
                    Ok(parsed) => {
                        self.filter = Some(parsed);
                        self.filter_error = None;
                        self.enqueue(Action::LoadList);
                        return Some(Mode::List);
                    }
                    Err(err) => {
                        self.filter_error = Some(err.to_string());
                    }
                }
            }
            _ => {
                apply_text_input(&mut self.filter_input, key);
            }
        }
        None
    }

    fn handle_detail_key(&mut self, key: KeyEvent, contact_id: ContactId) -> Option<Mode> {
        match key.code {
            KeyCode::Esc | KeyCode::Backspace => {
                self.detail = None;
                return Some(Mode::List);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.detail_scroll = self.detail_scroll.saturating_add(1);
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.detail_scroll = self.detail_scroll.saturating_sub(1);
            }
            KeyCode::PageDown => {
                self.detail_scroll = self.detail_scroll.saturating_add(5);
            }
            KeyCode::PageUp => {
                self.detail_scroll = self.detail_scroll.saturating_sub(5);
            }
            KeyCode::Char('e') => {
                if let Some(detail) = self.detail.clone() {
                    return Some(Mode::ModalEditContact(ContactForm::from_detail(&detail)));
                }
            }
            KeyCode::Char('n') => {
                return Some(Mode::ModalAddNote(NoteForm::new(contact_id)));
            }
            KeyCode::Char('t') => {
                self.enqueue(Action::LoadTags(contact_id));
                return Some(Mode::ModalEditTags(TagEditor::new(contact_id)));
            }
            KeyCode::Char('s') => {
                return Some(Mode::ModalSchedule(ScheduleForm::new(contact_id)));
            }
            KeyCode::Char('x') => {
                let message = "Clear scheduled touchpoint? (y/n)".to_string();
                return Some(Mode::Confirm(ConfirmState::new(
                    message,
                    ConfirmAction::ClearSchedule(contact_id),
                )));
            }
            KeyCode::Char('A') => {
                if let Some(detail) = &self.detail {
                    let (message, action) = if detail.archived_at.is_some() {
                        (
                            format!("Unarchive {}? (y/n)", detail.display_name),
                            ConfirmAction::UnarchiveContact(contact_id),
                        )
                    } else {
                        (
                            format!("Archive {}? (y/n)", detail.display_name),
                            ConfirmAction::ArchiveContact(contact_id),
                        )
                    };
                    return Some(Mode::Confirm(ConfirmState::new(message, action)));
                }
            }
            KeyCode::Char('r') => {
                self.enqueue(Action::LoadDetail(contact_id));
            }
            _ => {}
        }
        None
    }

    fn handle_contact_form_key(&mut self, form: &mut ContactForm, key: KeyEvent) -> Option<Mode> {
        match key.code {
            KeyCode::Esc => {
                return Some(Mode::List);
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if form.focus == 6 {
                    let now = knotter_core::time::now_utc();
                    form.set_next_touchpoint_now(now);
                    self.set_status("Next touchpoint set to now".to_string());
                }
            }
            KeyCode::Tab => form.focus_next(),
            KeyCode::BackTab => form.focus_prev(),
            KeyCode::Enter => {
                if form.is_save_focus() {
                    match form.to_action() {
                        Ok(action) => {
                            self.enqueue(action);
                            return Some(Mode::List);
                        }
                        Err(err) => self.set_error(err),
                    }
                } else if form.is_cancel_focus() {
                    return Some(Mode::List);
                } else {
                    form.focus_next();
                }
            }
            _ => {
                if let Some(target) = form.active_field_mut() {
                    apply_text_input(target, key);
                }
            }
        }
        None
    }

    fn handle_note_form_key(&mut self, form: &mut NoteForm, key: KeyEvent) -> Option<Mode> {
        match key.code {
            KeyCode::Esc => return Some(Mode::List),
            KeyCode::Tab => form.focus_next(),
            KeyCode::BackTab => form.focus_prev(),
            KeyCode::Enter => {
                if form.is_save_focus() {
                    match form.to_action() {
                        Ok(action) => {
                            self.enqueue(action);
                            return Some(Mode::List);
                        }
                        Err(err) => self.set_error(err),
                    }
                } else if form.is_cancel_focus() {
                    return Some(Mode::List);
                } else if form.is_note_focus() {
                    form.note.push('\n');
                } else {
                    form.focus_next();
                }
            }
            _ => {
                if let Some(target) = form.active_field_mut() {
                    apply_text_input(target, key);
                }
            }
        }
        None
    }

    fn handle_tag_editor_key(&mut self, editor: &mut TagEditor, key: KeyEvent) -> Option<Mode> {
        match key.code {
            KeyCode::Esc => return Some(Mode::List),
            KeyCode::Tab => editor.focus_next(),
            KeyCode::BackTab => editor.focus_prev(),
            KeyCode::Enter => match editor.focus {
                TagEditorFocus::Filter => {
                    if !editor.filter.trim().is_empty() {
                        let raw = editor.filter.trim();
                        match TagName::new(raw) {
                            Ok(tag) => {
                                let name = tag.as_str().to_string();
                                editor.toggle_tag(&name);
                                editor.filter.clear();
                            }
                            Err(err) => self.set_error(err.to_string()),
                        }
                    }
                }
                TagEditorFocus::List => editor.toggle_selected(),
                TagEditorFocus::Save => match editor.to_action() {
                    Ok(action) => {
                        self.enqueue(action);
                        return Some(Mode::List);
                    }
                    Err(err) => self.set_error(err),
                },
                TagEditorFocus::Cancel => return Some(Mode::List),
            },
            KeyCode::Char(' ') if editor.focus == TagEditorFocus::List => editor.toggle_selected(),
            KeyCode::Up | KeyCode::Char('k') => editor.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => editor.move_selection(1),
            _ => {
                if editor.focus == TagEditorFocus::Filter {
                    apply_text_input(&mut editor.filter, key);
                    editor.refresh_filter();
                }
            }
        }
        None
    }

    fn handle_schedule_form_key(&mut self, form: &mut ScheduleForm, key: KeyEvent) -> Option<Mode> {
        match key.code {
            KeyCode::Esc => return Some(Mode::List),
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if form.focus == 0 || form.focus == 1 {
                    let now = knotter_core::time::now_utc();
                    form.set_now(now);
                    self.set_status("Schedule set to now".to_string());
                }
            }
            KeyCode::Tab => form.focus_next(),
            KeyCode::BackTab => form.focus_prev(),
            KeyCode::Enter => {
                if form.is_save_focus() {
                    match form.to_action() {
                        Ok(action) => {
                            self.enqueue(action);
                            return Some(Mode::List);
                        }
                        Err(err) => self.set_error(err),
                    }
                } else if form.is_cancel_focus() {
                    return Some(Mode::List);
                } else {
                    form.focus_next();
                }
            }
            _ => {
                if let Some(target) = form.active_field_mut() {
                    apply_text_input(target, key);
                }
            }
        }
        None
    }

    fn handle_confirm_key(&mut self, state: &mut ConfirmState, key: KeyEvent) -> Option<Mode> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(action) = state.to_action() {
                    self.enqueue(action);
                }
                return Some(Mode::List);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                return Some(Mode::List);
            }
            _ => {}
        }
        None
    }

    fn move_selection(&mut self, delta: i32) {
        if self.contacts.is_empty() {
            self.selected = 0;
            return;
        }
        let len = self.contacts.len() as i32;
        let mut next = self.selected as i32 + delta;
        if next < 0 {
            next = 0;
        }
        if next >= len {
            next = len - 1;
        }
        self.selected = next as usize;
    }

    fn detail_for_selected(&self) -> Option<knotter_core::dto::ContactDetailDto> {
        let selected = self.selected_contact_id()?;
        let detail = self.detail.as_ref()?;
        if detail.id == selected {
            Some(detail.clone())
        } else {
            None
        }
    }
}

fn apply_text_input(target: &mut String, key: KeyEvent) {
    match key.code {
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            target.clear();
        }
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            delete_last_word(target);
        }
        KeyCode::Char(ch) => {
            if !key.modifiers.contains(KeyModifiers::CONTROL) {
                target.push(ch);
            }
        }
        KeyCode::Backspace => {
            target.pop();
        }
        _ => {}
    }
}

fn delete_last_word(value: &mut String) {
    while value.ends_with(|ch: char| ch.is_whitespace()) {
        value.pop();
    }
    while value.ends_with(|ch: char| !ch.is_whitespace()) {
        value.pop();
    }
}

#[derive(Debug, Clone)]
pub struct ContactForm {
    pub(crate) focus: usize,
    pub contact_id: Option<ContactId>,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub handle: String,
    pub timezone: String,
    pub cadence_days: String,
    pub next_touchpoint_at: String,
    pub original_next_touchpoint_at: Option<i64>,
    pub original_next_touchpoint_display: String,
}

impl ContactForm {
    const FIELD_COUNT: usize = 7;

    pub fn new(default_cadence_days: Option<i32>) -> Self {
        Self {
            focus: 0,
            contact_id: None,
            name: String::new(),
            email: String::new(),
            phone: String::new(),
            handle: String::new(),
            timezone: String::new(),
            cadence_days: default_cadence_days
                .map(|value| value.to_string())
                .unwrap_or_default(),
            next_touchpoint_at: String::new(),
            original_next_touchpoint_at: None,
            original_next_touchpoint_display: String::new(),
        }
    }

    pub fn from_detail(detail: &knotter_core::dto::ContactDetailDto) -> Self {
        let next_touchpoint_display = detail
            .next_touchpoint_at
            .map(knotter_core::time::format_timestamp_date_or_datetime)
            .unwrap_or_default();
        Self {
            focus: 0,
            contact_id: Some(detail.id),
            name: detail.display_name.clone(),
            email: detail.email.clone().unwrap_or_default(),
            phone: detail.phone.clone().unwrap_or_default(),
            handle: detail.handle.clone().unwrap_or_default(),
            timezone: detail.timezone.clone().unwrap_or_default(),
            cadence_days: detail
                .cadence_days
                .map(|value| value.to_string())
                .unwrap_or_default(),
            next_touchpoint_at: next_touchpoint_display.clone(),
            original_next_touchpoint_at: detail.next_touchpoint_at,
            original_next_touchpoint_display: next_touchpoint_display,
        }
    }

    pub fn focus_next(&mut self) {
        let total = Self::FIELD_COUNT + 2;
        self.focus = (self.focus + 1) % total;
    }

    pub fn focus_prev(&mut self) {
        let total = Self::FIELD_COUNT + 2;
        if self.focus == 0 {
            self.focus = total - 1;
        } else {
            self.focus -= 1;
        }
    }

    pub fn is_save_focus(&self) -> bool {
        self.focus == Self::FIELD_COUNT
    }

    pub fn is_cancel_focus(&self) -> bool {
        self.focus == Self::FIELD_COUNT + 1
    }

    pub fn active_field_mut(&mut self) -> Option<&mut String> {
        match self.focus {
            0 => Some(&mut self.name),
            1 => Some(&mut self.email),
            2 => Some(&mut self.phone),
            3 => Some(&mut self.handle),
            4 => Some(&mut self.timezone),
            5 => Some(&mut self.cadence_days),
            6 => Some(&mut self.next_touchpoint_at),
            _ => None,
        }
    }

    pub fn set_next_touchpoint_now(&mut self, now_utc: i64) {
        self.next_touchpoint_at = knotter_core::time::format_timestamp_datetime(now_utc);
    }

    pub fn to_action(&self) -> Result<Action, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("name is required".to_string());
        }

        let cadence = if self.cadence_days.trim().is_empty() {
            None
        } else {
            Some(
                self.cadence_days
                    .trim()
                    .parse::<i32>()
                    .map_err(|_| "invalid cadence days".to_string())?,
            )
        };

        let next_touchpoint_at = if self.next_touchpoint_at.trim().is_empty() {
            None
        } else if self.contact_id.is_some()
            && self.original_next_touchpoint_display == self.next_touchpoint_at
        {
            self.original_next_touchpoint_at
        } else {
            let (parsed, precision) =
                knotter_core::time::parse_local_timestamp_with_precision(&self.next_touchpoint_at)
                    .map_err(|err| err.to_string())?;
            let now = knotter_core::time::now_utc();
            Some(
                ensure_future_timestamp_with_precision(now, parsed, precision).map_err(|err| {
                    match err {
                        knotter_core::CoreError::TimestampInPast => {
                            "next touchpoint must be now or later".to_string()
                        }
                        _ => err.to_string(),
                    }
                })?,
            )
        };

        let email = normalize_optional(&self.email);
        let phone = normalize_optional(&self.phone);
        let handle = normalize_optional(&self.handle);
        let timezone = normalize_optional(&self.timezone);

        if let Some(contact_id) = self.contact_id {
            let update = knotter_store::repo::ContactUpdate {
                display_name: Some(name.to_string()),
                email: Some(email),
                phone: Some(phone),
                handle: Some(handle),
                timezone: Some(timezone),
                next_touchpoint_at: Some(next_touchpoint_at),
                cadence_days: Some(cadence),
                archived_at: None,
            };
            Ok(Action::UpdateContact(contact_id, update))
        } else {
            let input = knotter_store::repo::ContactNew {
                display_name: name.to_string(),
                email,
                phone,
                handle,
                timezone,
                next_touchpoint_at,
                cadence_days: cadence,
                archived_at: None,
            };
            Ok(Action::CreateContact(input))
        }
    }
}

#[derive(Debug, Clone)]
pub struct NoteForm {
    pub(crate) focus: usize,
    pub contact_id: ContactId,
    pub kind: String,
    pub when: String,
    pub note: String,
}

impl NoteForm {
    const FIELD_COUNT: usize = 3;

    pub fn new(contact_id: ContactId) -> Self {
        Self {
            focus: 0,
            contact_id,
            kind: "other:note".to_string(),
            when: String::new(),
            note: String::new(),
        }
    }

    pub fn focus_next(&mut self) {
        let total = Self::FIELD_COUNT + 2;
        self.focus = (self.focus + 1) % total;
    }

    pub fn focus_prev(&mut self) {
        let total = Self::FIELD_COUNT + 2;
        if self.focus == 0 {
            self.focus = total - 1;
        } else {
            self.focus -= 1;
        }
    }

    pub fn is_save_focus(&self) -> bool {
        self.focus == Self::FIELD_COUNT
    }

    pub fn is_cancel_focus(&self) -> bool {
        self.focus == Self::FIELD_COUNT + 1
    }

    pub fn is_note_focus(&self) -> bool {
        self.focus == 2
    }

    pub fn active_field_mut(&mut self) -> Option<&mut String> {
        match self.focus {
            0 => Some(&mut self.kind),
            1 => Some(&mut self.when),
            2 => Some(&mut self.note),
            _ => None,
        }
    }

    pub fn to_action(&self) -> Result<Action, String> {
        let kind =
            crate::util::parse_interaction_kind(&self.kind).map_err(|err| err.to_string())?;
        let occurred_at = if self.when.trim().is_empty() {
            knotter_core::time::now_utc()
        } else {
            knotter_core::time::parse_local_timestamp(&self.when).map_err(|err| err.to_string())?
        };

        let input = knotter_store::repo::InteractionNew {
            contact_id: self.contact_id,
            occurred_at,
            created_at: knotter_core::time::now_utc(),
            kind,
            note: self.note.clone(),
            follow_up_at: None,
        };

        Ok(Action::AddInteraction(input))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagEditorFocus {
    Filter,
    List,
    Save,
    Cancel,
}

#[derive(Debug, Clone)]
pub struct TagChoice {
    pub name: String,
    pub count: i64,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct TagEditor {
    pub contact_id: ContactId,
    pub focus: TagEditorFocus,
    pub filter: String,
    pub tags: Vec<TagChoice>,
    pub filtered: Vec<usize>,
    pub selected_index: usize,
}

impl TagEditor {
    pub fn new(contact_id: ContactId) -> Self {
        Self {
            contact_id,
            focus: TagEditorFocus::Filter,
            filter: String::new(),
            tags: Vec::new(),
            filtered: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            TagEditorFocus::Filter => TagEditorFocus::List,
            TagEditorFocus::List => TagEditorFocus::Save,
            TagEditorFocus::Save => TagEditorFocus::Cancel,
            TagEditorFocus::Cancel => TagEditorFocus::Filter,
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            TagEditorFocus::Filter => TagEditorFocus::Cancel,
            TagEditorFocus::List => TagEditorFocus::Filter,
            TagEditorFocus::Save => TagEditorFocus::List,
            TagEditorFocus::Cancel => TagEditorFocus::Save,
        };
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.filtered.is_empty() {
            self.selected_index = 0;
            return;
        }
        let len = self.filtered.len() as i32;
        let mut next = self.selected_index as i32 + delta;
        if next < 0 {
            next = 0;
        }
        if next >= len {
            next = len - 1;
        }
        self.selected_index = next as usize;
    }

    pub fn refresh_filter(&mut self) {
        let needle = self.filter.trim().to_ascii_lowercase();
        self.filtered = self
            .tags
            .iter()
            .enumerate()
            .filter_map(|(idx, tag)| {
                if needle.is_empty() || tag.name.to_ascii_lowercase().contains(&needle) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();
        if self.selected_index >= self.filtered.len() {
            self.selected_index = self.filtered.len().saturating_sub(1);
        }
    }

    pub fn toggle_selected(&mut self) {
        if let Some(idx) = self.filtered.get(self.selected_index).copied() {
            if let Some(tag) = self.tags.get_mut(idx) {
                tag.selected = !tag.selected;
            }
        }
    }

    pub fn toggle_tag(&mut self, name: &str) {
        if let Some(existing) = self.tags.iter_mut().find(|tag| tag.name == name) {
            existing.selected = !existing.selected;
            self.refresh_filter();
            return;
        }
        self.tags.push(TagChoice {
            name: name.to_string(),
            count: 0,
            selected: true,
        });
        self.refresh_filter();
    }

    pub fn set_tags(&mut self, tags: Vec<TagChoice>) {
        self.tags = tags;
        self.refresh_filter();
    }

    pub fn to_action(&self) -> Result<Action, String> {
        let mut out = Vec::new();
        for tag in &self.tags {
            if tag.selected {
                let name = TagName::new(&tag.name).map_err(|err| err.to_string())?;
                out.push(name);
            }
        }
        Ok(Action::SetTags(self.contact_id, out))
    }
}

#[derive(Debug, Clone)]
pub struct ScheduleForm {
    pub(crate) focus: usize,
    pub contact_id: ContactId,
    pub date: String,
    pub time: String,
}

impl ScheduleForm {
    const FIELD_COUNT: usize = 2;

    pub fn new(contact_id: ContactId) -> Self {
        Self {
            focus: 0,
            contact_id,
            date: String::new(),
            time: String::new(),
        }
    }

    pub fn focus_next(&mut self) {
        let total = Self::FIELD_COUNT + 2;
        self.focus = (self.focus + 1) % total;
    }

    pub fn focus_prev(&mut self) {
        let total = Self::FIELD_COUNT + 2;
        if self.focus == 0 {
            self.focus = total - 1;
        } else {
            self.focus -= 1;
        }
    }

    pub fn is_save_focus(&self) -> bool {
        self.focus == Self::FIELD_COUNT
    }

    pub fn is_cancel_focus(&self) -> bool {
        self.focus == Self::FIELD_COUNT + 1
    }

    pub fn active_field_mut(&mut self) -> Option<&mut String> {
        match self.focus {
            0 => Some(&mut self.date),
            1 => Some(&mut self.time),
            _ => None,
        }
    }

    pub fn set_now(&mut self, now_utc: i64) {
        self.date = knotter_core::time::format_timestamp_date(now_utc);
        self.time = knotter_core::time::format_timestamp_time(now_utc);
    }

    pub fn to_action(&self) -> Result<Action, String> {
        let date = self.date.trim();
        if date.is_empty() {
            return Err("date is required".to_string());
        }
        let time = if self.time.trim().is_empty() {
            None
        } else {
            Some(self.time.trim())
        };
        let (timestamp, precision) =
            knotter_core::time::parse_local_date_time_with_precision(date, time)
                .map_err(|err| err.to_string())?;
        let now = knotter_core::time::now_utc();
        let timestamp =
            ensure_future_timestamp_with_precision(now, timestamp, precision).map_err(|err| {
                match err {
                    knotter_core::CoreError::TimestampInPast => {
                        "scheduled time must be now or later".to_string()
                    }
                    _ => err.to_string(),
                }
            })?;
        Ok(Action::ScheduleContact(self.contact_id, timestamp))
    }
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    ClearSchedule(ContactId),
    ArchiveContact(ContactId),
    UnarchiveContact(ContactId),
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub message: String,
    pub action: ConfirmAction,
}

impl ConfirmState {
    pub fn new(message: String, action: ConfirmAction) -> Self {
        Self { message, action }
    }

    pub fn to_action(&self) -> Option<Action> {
        match self.action {
            ConfirmAction::ClearSchedule(id) => Some(Action::ClearSchedule(id)),
            ConfirmAction::ArchiveContact(id) => Some(Action::ArchiveContact(id)),
            ConfirmAction::UnarchiveContact(id) => Some(Action::UnarchiveContact(id)),
        }
    }
}

fn normalize_optional(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
