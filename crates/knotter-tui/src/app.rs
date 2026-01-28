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
    MergeList,
    ModalMergePicker(MergePicker),
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
    pub merge_candidates: Vec<MergeCandidateView>,
    pub merge_selected: usize,
    actions: VecDeque<Action>,
    pub(crate) pending_select: Option<ContactId>,
}

#[derive(Debug, Clone)]
pub struct MergeCandidateView {
    pub id: knotter_core::domain::MergeCandidateId,
    pub reason: String,
    pub contact_a_id: ContactId,
    pub contact_b_id: ContactId,
    pub preferred_contact_id: Option<ContactId>,
    pub contact_a_name: String,
    pub contact_b_name: String,
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
            merge_candidates: Vec::new(),
            merge_selected: 0,
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

    pub fn apply_merge_candidates(&mut self, items: Vec<MergeCandidateView>) {
        self.merge_candidates = items;
        if self.merge_selected >= self.merge_candidates.len() {
            self.merge_selected = self.merge_candidates.len().saturating_sub(1);
        }
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
            Mode::MergeList => {
                if let Some(next) = self.handle_merge_list_key(key) {
                    mode = next;
                }
            }
            Mode::ModalMergePicker(picker) => {
                if let Some(next) = self.handle_merge_picker_key(picker, key) {
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
            KeyCode::Char('m') => {
                self.enqueue(Action::LoadMerges);
                return Some(Mode::MergeList);
            }
            KeyCode::Char('M') => {
                if let Some(item) = self.contacts.get(self.selected) {
                    let contact_id = item.id;
                    let contact_name = item.display_name.clone();
                    self.enqueue(Action::LoadMergePicker(contact_id));
                    return Some(Mode::ModalMergePicker(MergePicker::new(
                        contact_id,
                        contact_name,
                        MergePickerReturn::List,
                    )));
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
            KeyCode::Char('m') => {
                self.enqueue(Action::LoadMerges);
                return Some(Mode::MergeList);
            }
            KeyCode::Char('M') => {
                if let Some(detail) = &self.detail {
                    let contact_name = detail.display_name.clone();
                    self.enqueue(Action::LoadMergePicker(contact_id));
                    return Some(Mode::ModalMergePicker(MergePicker::new(
                        contact_id,
                        contact_name,
                        MergePickerReturn::Detail(contact_id),
                    )));
                }
            }
            KeyCode::Char('r') => {
                self.enqueue(Action::LoadDetail(contact_id));
            }
            _ => {}
        }
        None
    }

    fn handle_merge_list_key(&mut self, key: KeyEvent) -> Option<Mode> {
        match key.code {
            KeyCode::Esc => return Some(Mode::List),
            KeyCode::Down | KeyCode::Char('j') => self.move_merge_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_merge_selection(-1),
            KeyCode::PageDown => self.move_merge_selection(5),
            KeyCode::PageUp => self.move_merge_selection(-5),
            KeyCode::Home | KeyCode::Char('g') => self.merge_selected = 0,
            KeyCode::End | KeyCode::Char('G') => {
                if !self.merge_candidates.is_empty() {
                    self.merge_selected = self.merge_candidates.len() - 1;
                }
            }
            KeyCode::Char('p') => {
                if let Some(candidate) = self.merge_candidates.get(self.merge_selected) {
                    let current = candidate
                        .preferred_contact_id
                        .unwrap_or(candidate.contact_a_id);
                    let preferred_contact_id = if current == candidate.contact_a_id {
                        candidate.contact_b_id
                    } else {
                        candidate.contact_a_id
                    };
                    self.enqueue(Action::SetMergePreferred {
                        candidate_id: candidate.id,
                        preferred_contact_id,
                    });
                }
            }
            KeyCode::Char('d') => {
                if let Some(candidate) = self.merge_candidates.get(self.merge_selected) {
                    let message = format!("Dismiss merge candidate {}? (y/n)", candidate.id);
                    return Some(Mode::Confirm(ConfirmState::new(
                        message,
                        ConfirmAction::DismissMerge(candidate.id),
                    )));
                }
            }
            KeyCode::Enter => {
                if let Some(candidate) = self.merge_candidates.get(self.merge_selected) {
                    let primary_id = candidate
                        .preferred_contact_id
                        .unwrap_or(candidate.contact_a_id);
                    let secondary_id = if primary_id == candidate.contact_a_id {
                        candidate.contact_b_id
                    } else {
                        candidate.contact_a_id
                    };
                    let primary_name = if primary_id == candidate.contact_a_id {
                        &candidate.contact_a_name
                    } else {
                        &candidate.contact_b_name
                    };
                    let secondary_name = if secondary_id == candidate.contact_a_id {
                        &candidate.contact_a_name
                    } else {
                        &candidate.contact_b_name
                    };
                    let message = format!("Merge {} into {}? (y/n)", secondary_name, primary_name);
                    return Some(Mode::Confirm(ConfirmState::new(
                        message,
                        ConfirmAction::ApplyMerge {
                            primary_id,
                            secondary_id,
                        },
                    )));
                }
            }
            KeyCode::Char('r') => self.enqueue(Action::LoadMerges),
            _ => {}
        }
        None
    }

    fn handle_merge_picker_key(&mut self, picker: &mut MergePicker, key: KeyEvent) -> Option<Mode> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('r')) {
            self.enqueue(Action::LoadMergePicker(picker.primary_id));
            return None;
        }
        match key.code {
            KeyCode::Esc => return Some(picker.return_mode.to_mode()),
            KeyCode::Tab => picker.focus_next(),
            KeyCode::BackTab => picker.focus_prev(),
            KeyCode::Up => {
                if picker.focus == MergePickerFocus::List {
                    picker.move_selection(-1);
                }
            }
            KeyCode::Down => {
                if picker.focus == MergePickerFocus::List {
                    picker.move_selection(1);
                }
            }
            KeyCode::Char('k') if picker.focus == MergePickerFocus::List => {
                picker.move_selection(-1);
            }
            KeyCode::Char('j') if picker.focus == MergePickerFocus::List => {
                picker.move_selection(1);
            }
            KeyCode::PageDown => {
                if picker.focus == MergePickerFocus::List {
                    picker.move_selection(5);
                }
            }
            KeyCode::PageUp => {
                if picker.focus == MergePickerFocus::List {
                    picker.move_selection(-5);
                }
            }
            KeyCode::Home => {
                if picker.focus == MergePickerFocus::List {
                    picker.selected_index = 0;
                }
            }
            KeyCode::End => {
                if picker.focus == MergePickerFocus::List && !picker.filtered.is_empty() {
                    picker.selected_index = picker.filtered.len() - 1;
                }
            }
            KeyCode::Char('g') if picker.focus == MergePickerFocus::List => {
                picker.selected_index = 0;
            }
            KeyCode::Char('G') if picker.focus == MergePickerFocus::List => {
                if !picker.filtered.is_empty() {
                    picker.selected_index = picker.filtered.len() - 1;
                }
            }
            KeyCode::Char('r') if picker.focus != MergePickerFocus::Filter => {
                self.enqueue(Action::LoadMergePicker(picker.primary_id));
            }
            KeyCode::Enter => match picker.focus {
                MergePickerFocus::Filter => picker.focus_next(),
                MergePickerFocus::List | MergePickerFocus::Merge => {
                    if let Some(target) = picker.selected_item() {
                        let message = format!(
                            "Merge {} into {}? (y/n)",
                            target.display_name, picker.primary_name
                        );
                        let confirm = ConfirmState::new(
                            message,
                            ConfirmAction::ApplyMerge {
                                primary_id: picker.primary_id,
                                secondary_id: target.id,
                            },
                        )
                        .with_return_modes(
                            picker.return_mode.to_confirm_return(),
                            ConfirmReturn::MergePicker(picker.clone()),
                        );
                        return Some(Mode::Confirm(confirm));
                    }
                    self.set_error("no merge target selected");
                }
                MergePickerFocus::Cancel => return Some(picker.return_mode.to_mode()),
            },
            _ => {
                if picker.focus == MergePickerFocus::Filter {
                    apply_text_input(&mut picker.filter, key);
                    picker.refresh_filter();
                }
            }
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
                let return_mode = state
                    .return_on_confirm
                    .clone()
                    .unwrap_or_else(|| default_confirm_return_mode(&state.action));
                return Some(return_mode.into_mode());
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                let return_mode = state
                    .return_on_cancel
                    .clone()
                    .unwrap_or_else(|| default_confirm_return_mode(&state.action));
                return Some(return_mode.into_mode());
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

    fn move_merge_selection(&mut self, delta: i32) {
        if self.merge_candidates.is_empty() {
            self.merge_selected = 0;
            return;
        }
        let len = self.merge_candidates.len() as i32;
        let mut next = self.merge_selected as i32 + delta;
        if next < 0 {
            next = 0;
        }
        if next >= len {
            next = len - 1;
        }
        self.merge_selected = next as usize;
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
    pub emails: String,
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
            emails: String::new(),
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
        let mut emails = detail.emails.clone();
        if emails.is_empty() {
            if let Some(email) = detail.email.as_deref() {
                emails.push(email.to_string());
            }
        }
        let emails = emails.join(", ");
        Self {
            focus: 0,
            contact_id: Some(detail.id),
            name: detail.display_name.clone(),
            emails,
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
            1 => Some(&mut self.emails),
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

        let emails = parse_emails(&self.emails);
        let primary_email = emails.first().cloned();
        let phone = normalize_optional(&self.phone);
        let handle = normalize_optional(&self.handle);
        let timezone = normalize_optional(&self.timezone);

        if let Some(contact_id) = self.contact_id {
            let update = knotter_store::repo::ContactUpdate {
                display_name: Some(name.to_string()),
                email: Some(primary_email),
                email_source: Some("tui".to_string()),
                phone: Some(phone),
                handle: Some(handle),
                timezone: Some(timezone),
                next_touchpoint_at: Some(next_touchpoint_at),
                cadence_days: Some(cadence),
                archived_at: None,
            };
            Ok(Action::UpdateContact(contact_id, update, emails))
        } else {
            let input = knotter_store::repo::ContactNew {
                display_name: name.to_string(),
                email: primary_email,
                phone,
                handle,
                timezone,
                next_touchpoint_at,
                cadence_days: cadence,
                archived_at: None,
            };
            Ok(Action::CreateContact(input, emails))
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
pub struct MergePickerItem {
    pub id: ContactId,
    pub display_name: String,
    pub email: Option<String>,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergePickerFocus {
    Filter,
    List,
    Merge,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergePickerReturn {
    List,
    Detail(ContactId),
}

impl MergePickerReturn {
    pub fn to_mode(self) -> Mode {
        match self {
            MergePickerReturn::List => Mode::List,
            MergePickerReturn::Detail(contact_id) => Mode::Detail(contact_id),
        }
    }

    pub fn to_confirm_return(self) -> ConfirmReturn {
        match self {
            MergePickerReturn::List => ConfirmReturn::List,
            MergePickerReturn::Detail(contact_id) => ConfirmReturn::Detail(contact_id),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MergePicker {
    pub primary_id: ContactId,
    pub primary_name: String,
    pub return_mode: MergePickerReturn,
    pub focus: MergePickerFocus,
    pub filter: String,
    pub items: Vec<MergePickerItem>,
    pub filtered: Vec<usize>,
    pub selected_index: usize,
}

impl MergePicker {
    pub fn new(
        primary_id: ContactId,
        primary_name: String,
        return_mode: MergePickerReturn,
    ) -> Self {
        Self {
            primary_id,
            primary_name,
            return_mode,
            focus: MergePickerFocus::Filter,
            filter: String::new(),
            items: Vec::new(),
            filtered: Vec::new(),
            selected_index: 0,
        }
    }

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            MergePickerFocus::Filter => MergePickerFocus::List,
            MergePickerFocus::List => MergePickerFocus::Merge,
            MergePickerFocus::Merge => MergePickerFocus::Cancel,
            MergePickerFocus::Cancel => MergePickerFocus::Filter,
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            MergePickerFocus::Filter => MergePickerFocus::Cancel,
            MergePickerFocus::List => MergePickerFocus::Filter,
            MergePickerFocus::Merge => MergePickerFocus::List,
            MergePickerFocus::Cancel => MergePickerFocus::Merge,
        };
    }

    pub fn is_merge_focus(&self) -> bool {
        self.focus == MergePickerFocus::Merge
    }

    pub fn is_cancel_focus(&self) -> bool {
        self.focus == MergePickerFocus::Cancel
    }

    pub fn set_items(&mut self, items: Vec<MergePickerItem>) {
        let selected_id = self.selected_item().map(|item| item.id);
        self.items = items;
        self.refresh_filter();
        if let Some(id) = selected_id {
            if let Some(pos) = self
                .filtered
                .iter()
                .position(|idx| self.items[*idx].id == id)
            {
                self.selected_index = pos;
            }
        }
    }

    pub fn refresh_filter(&mut self) {
        let needle = self.filter.trim().to_lowercase();
        self.filtered = if needle.is_empty() {
            (0..self.items.len()).collect()
        } else {
            self.items
                .iter()
                .enumerate()
                .filter_map(|(idx, item)| {
                    let mut haystack = item.display_name.to_lowercase();
                    if let Some(email) = item.email.as_deref() {
                        haystack.push(' ');
                        haystack.push_str(&email.to_lowercase());
                    }
                    if haystack.contains(&needle) {
                        Some(idx)
                    } else {
                        None
                    }
                })
                .collect()
        };
        if self.selected_index >= self.filtered.len() {
            self.selected_index = self.filtered.len().saturating_sub(1);
        }
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

    pub fn selected_item(&self) -> Option<&MergePickerItem> {
        let idx = self.filtered.get(self.selected_index)?;
        self.items.get(*idx)
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
    ApplyMerge {
        primary_id: ContactId,
        secondary_id: ContactId,
    },
    DismissMerge(knotter_core::domain::MergeCandidateId),
}

#[derive(Debug, Clone)]
pub struct ConfirmState {
    pub message: String,
    pub action: ConfirmAction,
    pub return_on_confirm: Option<ConfirmReturn>,
    pub return_on_cancel: Option<ConfirmReturn>,
}

impl ConfirmState {
    pub fn new(message: String, action: ConfirmAction) -> Self {
        Self {
            message,
            action,
            return_on_confirm: None,
            return_on_cancel: None,
        }
    }

    pub fn with_return_modes(mut self, confirm: ConfirmReturn, cancel: ConfirmReturn) -> Self {
        self.return_on_confirm = Some(confirm);
        self.return_on_cancel = Some(cancel);
        self
    }

    pub fn to_action(&self) -> Option<Action> {
        match self.action {
            ConfirmAction::ClearSchedule(id) => Some(Action::ClearSchedule(id)),
            ConfirmAction::ArchiveContact(id) => Some(Action::ArchiveContact(id)),
            ConfirmAction::UnarchiveContact(id) => Some(Action::UnarchiveContact(id)),
            ConfirmAction::ApplyMerge {
                primary_id,
                secondary_id,
            } => Some(Action::ApplyMerge {
                primary_id,
                secondary_id,
            }),
            ConfirmAction::DismissMerge(id) => Some(Action::DismissMerge(id)),
        }
    }
}

fn default_confirm_return_mode(action: &ConfirmAction) -> ConfirmReturn {
    match action {
        ConfirmAction::ApplyMerge { .. } | ConfirmAction::DismissMerge(_) => {
            ConfirmReturn::MergeList
        }
        _ => ConfirmReturn::List,
    }
}

#[derive(Debug, Clone)]
pub enum ConfirmReturn {
    List,
    MergeList,
    Detail(ContactId),
    MergePicker(MergePicker),
}

impl ConfirmReturn {
    pub fn into_mode(self) -> Mode {
        match self {
            ConfirmReturn::List => Mode::List,
            ConfirmReturn::MergeList => Mode::MergeList,
            ConfirmReturn::Detail(contact_id) => Mode::Detail(contact_id),
            ConfirmReturn::MergePicker(picker) => Mode::ModalMergePicker(picker),
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

fn parse_emails(raw: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for part in raw.split([',', ';', '\n']) {
        if let Some(email) = knotter_core::domain::normalize_email(part) {
            if !out
                .iter()
                .any(|value| value.as_str().eq_ignore_ascii_case(&email))
            {
                out.push(email);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{MergePicker, MergePickerItem, MergePickerReturn};
    use knotter_core::domain::ContactId;

    fn item(name: &str, email: Option<&str>) -> MergePickerItem {
        MergePickerItem {
            id: ContactId::new(),
            display_name: name.to_string(),
            email: email.map(|value| value.to_string()),
            archived_at: None,
        }
    }

    #[test]
    fn merge_picker_filters_by_name_and_email_case_insensitive() {
        let mut picker = MergePicker::new(
            ContactId::new(),
            "Primary".to_string(),
            MergePickerReturn::List,
        );
        picker.set_items(vec![
            item("Alice Example", Some("alice@example.com")),
            item("Bob Builder", None),
            item("Charlie", Some("charlie@work.com")),
        ]);

        picker.filter = "ali".to_string();
        picker.refresh_filter();
        assert_eq!(picker.filtered.len(), 1);
        assert_eq!(
            picker
                .selected_item()
                .map(|item| item.display_name.as_str()),
            Some("Alice Example")
        );

        picker.filter = "WORK".to_string();
        picker.refresh_filter();
        assert_eq!(picker.filtered.len(), 1);
        assert_eq!(
            picker
                .selected_item()
                .map(|item| item.display_name.as_str()),
            Some("Charlie")
        );

        picker.filter = "bob".to_string();
        picker.refresh_filter();
        assert_eq!(picker.filtered.len(), 1);
        assert_eq!(
            picker
                .selected_item()
                .map(|item| item.display_name.as_str()),
            Some("Bob Builder")
        );

        picker.filter = "nomatch".to_string();
        picker.refresh_filter();
        assert!(picker.filtered.is_empty());
        assert!(picker.selected_item().is_none());
    }

    #[test]
    fn merge_picker_clamps_selection_after_filtering() {
        let mut picker = MergePicker::new(
            ContactId::new(),
            "Primary".to_string(),
            MergePickerReturn::List,
        );
        picker.set_items(vec![
            item("Alice", None),
            item("Bob", None),
            item("Cara", None),
        ]);
        picker.selected_index = 2;

        picker.filter = "ali".to_string();
        picker.refresh_filter();
        assert_eq!(picker.filtered.len(), 1);
        assert_eq!(picker.selected_index, 0);
        assert_eq!(
            picker
                .selected_item()
                .map(|item| item.display_name.as_str()),
            Some("Alice")
        );

        picker.filter = "zzz".to_string();
        picker.refresh_filter();
        assert!(picker.filtered.is_empty());
        assert_eq!(picker.selected_index, 0);
        assert!(picker.selected_item().is_none());
    }

    #[test]
    fn merge_picker_preserves_selection_by_id_on_set_items() {
        let mut picker = MergePicker::new(
            ContactId::new(),
            "Primary".to_string(),
            MergePickerReturn::List,
        );
        let a = item("Alice", None);
        let b = item("Bob", None);
        let c = item("Cara", None);
        let b_id = b.id;
        picker.set_items(vec![a.clone(), b.clone(), c]);
        picker.selected_index = 1;

        picker.set_items(vec![b.clone(), a]);
        assert_eq!(picker.filtered.len(), 2);
        assert_eq!(picker.selected_item().map(|item| item.id), Some(b_id));
        assert_eq!(picker.selected_index, 0);
    }

    #[test]
    fn merge_picker_move_selection_respects_bounds() {
        let mut picker = MergePicker::new(
            ContactId::new(),
            "Primary".to_string(),
            MergePickerReturn::List,
        );
        picker.set_items(vec![item("Alice", None), item("Bob", None)]);

        picker.move_selection(-1);
        assert_eq!(picker.selected_index, 0);

        picker.move_selection(5);
        assert_eq!(picker.selected_index, 1);
    }
}
