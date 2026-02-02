# Keybindings

## Overview

knotter’s TUI is built around an explicit **mode/state machine**:

- `Mode::List`
- `Mode::FilterEditing`
- `Mode::Detail(ContactId)`
- `Mode::MergeList`
- `Mode::ModalAddContact`
- `Mode::ModalEditContact(ContactId)`
- `Mode::ModalAddNote(ContactId)`
- `Mode::ModalEditTags(ContactId)`
- `Mode::ModalSchedule(ContactId)`

Keybindings are designed to be:
- fast (single-key actions where safe)
- predictable (same keys mean the same thing across modes)
- accessible (arrow keys work everywhere; vim keys are optional)

The TUI should show a small “hint footer” with the most relevant keys for the current mode.

For manual validation after UI changes, see [TUI Smoke Checklist](tui-smoke.md).

Launch the TUI with:

```
knotter tui
```

Or run the binary directly:

```
knotter-tui
```

---

## Global keys (work in all modes)

- `?`  
  Toggle help overlay (shows this cheat sheet in-app).

- `Ctrl+C`  
  Quit immediately (must restore terminal state).  
  If a modal has unsaved changes, knotter may confirm before exiting.

- `q`  
  Quit (may confirm if there are unsaved changes).

- `Esc`  
  “Back / cancel” depending on context:
  - in modals: cancel/close modal
  - in detail: back to list
  - in list: if no modal is open, `Esc` does nothing (filter clearing is explicit; see below)

- `r`  
  Refresh list/detail from the database (safe “get me back to known good state”).

---

## Common navigation keys

These apply in list-like panels (contact list, tag list, interaction list):

- `↑` / `↓`  
  Move selection up/down.

- `j` / `k`  
  Vim-style move down/up.

- `PageUp` / `PageDown`  
  Scroll one page.

- `g`  
  Jump to top.

- `G`  
  Jump to bottom.

- `Home` / `End`  
  Alternative jump to top/bottom.

---

## Mode: List (`Mode::List`)

This is the default view: a scrollable contact list, with due indicators and tags.

### Navigation
- `↑`/`↓`, `j`/`k` move selection
- `PageUp`/`PageDown` scroll
- `g`/`G` jump top/bottom

### Open detail
- `Enter`  
  Open selected contact detail (`Mode::Detail(contact_id)`).

### Filtering
- `/`  
  Enter filter editing (`Mode::FilterEditing`) with the current filter string.
- `c`  
  Clear filter (sets filter string to empty and reloads list).

### Contact actions
- `a`  
  Open “Add Contact” modal (`Mode::ModalAddContact`).
- `e`  
  Open “Edit Contact” modal for selected (`Mode::ModalEditContact`).
- `n`  
  Add note for selected (`Mode::ModalAddNote`).
- `t`  
  Edit tags for selected (`Mode::ModalEditTags`).
- `s`  
  Schedule next touchpoint (`Mode::ModalSchedule`).
- `x`  
  Clear scheduled next touchpoint for selected (should confirm).
- `A`  
  Archive/unarchive selected contact (confirm required).
- `v`  
  Toggle showing archived contacts in the list.
- `m`  
  Open merge candidate list (`Mode::MergeList`).
- `M`  
  Open merge picker for selected contact (`Mode::ModalMergePicker`).

### Optional (only if implemented)
- `d`  
  Delete contact (dangerous; must confirm).

---

## Mode: Filter editing (`Mode::FilterEditing`)

This is a single-line editor for the filter query.

### Editing
- Type characters to edit the filter
- `Backspace` delete character
- `Ctrl+U` clear the entire line
- `Ctrl+W` delete previous word (optional, but very nice)

### Apply / cancel
- `Enter`  
  Apply filter (parse in core; if parse error, stay in FilterEditing and show error).
- `Esc`  
  Cancel filter editing (revert to previous filter string, return to list).

### Quick reference: filter syntax (MVP)
- `#designer` → require tag designer  
- `due:overdue` | `due:today` | `due:soon` | `due:any` | `due:none`  
- plain words match name/email/phone/handle

---

## Mode: Contact detail (`Mode::Detail(contact_id)`)

The detail view shows:
- contact fields
- tags
- next touchpoint + cadence
- recent interactions (scrollable)

### Navigation inside detail
- `↑`/`↓`, `j`/`k` scroll interactions list
- `PageUp`/`PageDown` scroll faster
- `g`/`G` top/bottom of interactions

### Back
- `Esc` or `Backspace`  
  Return to list (`Mode::List`).

### Actions
- `e`  
  Edit contact (`Mode::ModalEditContact`).
- `n`  
  Add note (`Mode::ModalAddNote`).
- `t`  
  Edit tags (`Mode::ModalEditTags`).
- `s`  
  Schedule next touchpoint (`Mode::ModalSchedule`).
- `x`  
  Clear schedule (confirm).
- `m`  
  Open merge candidate list (`Mode::MergeList`).
- `M`  
  Open merge picker for this contact (`Mode::ModalMergePicker`).

### Optional
- `d` delete contact (confirm)
- `D` delete selected interaction (confirm) if interaction deletion exists

---

## Mode: Merge list (`Mode::MergeList`)

Shows open merge candidates created during import/sync.

### Navigation
- `↑`/`↓`, `j`/`k` move selection
- `PageUp`/`PageDown` scroll
- `g`/`G` jump top/bottom

### Actions
- `Enter`  
  Merge selected candidate (confirm required).
- `p`  
  Toggle which contact is preferred for merge.
- `A`  
  Apply all auto-merge safe candidates (confirm required).
- `d`  
  Dismiss selected candidate (confirm required).
- `r`  
  Refresh merge list.
- `Esc`  
  Return to contact list.

---

## Mode: Merge picker (`Mode::ModalMergePicker`)

Pick a contact to merge into the selected primary contact.

### Navigation
- `Tab`/`Shift+Tab` move focus (filter → list → buttons)
- `↑`/`↓`, `j`/`k` move selection (when list is focused)
- `PageUp`/`PageDown` scroll
- `g`/`G` jump top/bottom

### Actions
- `Enter`  
  Merge selected contact into primary (confirm required).
- `Ctrl+R`  
  Refresh contact list.
- `Esc`  
  Return to the previous view.

---

## Modal conventions (all modal modes)

All modals share consistent behavior.

### Universal modal keys
- `Tab`  
  Move focus to next field/control.
- `Shift+Tab`  
  Move focus to previous field/control.
- `Ctrl+N`  
  Set date/time fields to “now” in contact/schedule modals.
- `Esc`  
  Cancel/close (confirm if there are unsaved changes).
- `Enter`
  - if focus is on a button: activate it (`Save`, `Cancel`)
  - if focus is on a single-line input: may move to next field (implementation choice)
- Arrow keys move within lists when a list control is focused.

### Buttons pattern (recommended)
Every modal has explicit buttons at the bottom:
- `[Save] [Cancel]`
This avoids unreliable “Ctrl+S” behavior across terminals.

---

## Mode: Add contact (`Mode::ModalAddContact`)

### Fields (recommended)
- Name (required)
- Email (optional)
- Phone (optional)
- Handle (optional)
- Cadence days (optional)
- Next touchpoint date/time (optional)

### Keys
- `Tab` / `Shift+Tab` navigate fields and buttons
- `Enter` on `[Save]` saves
- `Enter` on `[Cancel]` cancels
- `Esc` cancels

Validation behavior:
- If name is empty, show inline error and keep the modal open.

---

## Mode: Edit contact (`Mode::ModalEditContact(contact_id)`)

Same keys as Add Contact.

Additional behavior:
- Show current values prefilled
- Keep “save” disabled until a change is made (optional)

---

## Mode: Add note (`Mode::ModalAddNote(contact_id)`)

This modal adds an `Interaction` entry.

### Controls (recommended)
- Kind selector (call/text/hangout/email/other)
- Occurred-at timestamp (defaults to now)
- Optional follow-up date/time
- Note editor (multi-line)

### Keys
- `Tab` / `Shift+Tab` move focus among:
  - kind
  - occurred-at
  - follow-up-at
  - note editor
  - buttons
- In the multi-line note editor:
  - `Enter` inserts newline
  - `Backspace` deletes
  - `PageUp`/`PageDown` scroll note if needed
- To save:
  - `Tab` to `[Save]`, then `Enter`
- `Esc` cancels (confirm if note has content)

---

## Mode: Edit tags (`Mode::ModalEditTags(contact_id)`)

This modal manages the tag set for a contact.

### Layout (recommended)
- Left: existing tags list (with optional counts)
- Top or bottom: tag search/create input
- Bottom: buttons `[Save] [Cancel]`

### Keys
- Navigation:
  - `↑`/`↓`, `j`/`k` move in the tag list
- Toggle tag attachment:
  - `Space` or `Enter` toggles the selected tag on/off for this contact
- Create new tag:
  - Type in tag input
  - `Enter` creates the tag (normalized) and toggles it on
  - Invalid tags surface an error and leave the input intact
- Save:
  - `Tab` to `[Save]`, `Enter`
- Cancel:
  - `Esc` or `[Cancel]`

Behavior rules:
- Tags are normalized the same way as everywhere else.
- Saving should perform “replace tag set” (set_contact_tags) to keep semantics simple.

---

## Mode: Schedule touchpoint (`Mode::ModalSchedule(contact_id)`)

This modal edits `next_touchpoint_at` (and optionally cadence).

### Controls (recommended)
- Date input (required for scheduling)
- Optional time input (defaults to a sensible time or “all-day-ish”)
- Optional cadence days (if you want to set it here too)
- Quick picks (optional): `+7d`, `+30d`

### Keys
- `Tab` / `Shift+Tab` move between inputs and buttons
- `Enter` on `[Save]` sets `next_touchpoint_at`
- `Esc` cancels

Optional quick picks (if implemented):
- `1` → schedule +7 days from today
- `2` → schedule +30 days from today

---

## Suggested on-screen hint footer (by mode)

knotter should display mode-appropriate hints such as:

- List:
  - `Enter: Detail  /: Filter  a: Add  e: Edit  n: Note  t: Tags  s: Schedule  m: Merges  q: Quit`
- Detail:
  - `Esc: Back  n: Note  t: Tags  s: Schedule  e: Edit  m: Merges`
- Filter:
  - `Enter: Apply  Esc: Cancel`
- Merge:
  - `Enter: Merge  p: Prefer  d: Dismiss  r: Refresh  Esc: Back`
- Modals:
  - `Tab: Next  Shift+Tab: Prev  Enter: Activate  Esc: Cancel`

---

## Notes on portability

- Avoid making critical actions depend on terminal-specific combos like `Ctrl+Enter`.
- Prefer explicit `[Save]` / `[Cancel]` buttons with `Tab` navigation.
- Always restore terminal state on exit, panic, or ctrl-c.
