use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};

use serde::{Deserialize, Serialize};
use shiro_compatibility::{BookLocatorOutput, ClientRuntime, EpubSessionId};
use shiro_error_catalog::{CommandErrorPayload, ErrorCode};
use tauri::{AppHandle, Emitter};
#[cfg(desktop)]
use tauri::{Manager, PhysicalPosition, Position, WebviewUrl, WebviewWindowBuilder};
use uuid::Uuid;

pub const READER_WINDOW_STATE_EVENT: &str = "shiro://reader-window-state";
pub const READER_TAB_DROP_PREVIEW_EVENT: &str = "shiro://reader-tab-drop-preview";

const DEFAULT_TAB_STRIP_HEIGHT_PX: f64 = 42.0;
const DEFAULT_TAB_STRIP_DRAG_WIDTH_PX: f64 = 720.0;
const ESTIMATED_TAB_SLOT_WIDTH_PX: f64 = 160.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderTabRecord {
    pub tab_id: String,
    pub book_id: String,
    pub book_hash: String,
    pub title: String,
    pub format: String,
    pub session_id: Option<EpubSessionId>,
    pub current_locator: Option<BookLocatorOutput>,
    pub fraction: f64,
    pub translation_mode: String,
    pub target_language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OtherReaderWindowSummaryDto {
    pub window_id: String,
    pub active_title: String,
    pub tab_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderWindowSnapshotDto {
    pub window_id: String,
    pub window_label: String,
    pub tabs: Vec<ReaderTabRecord>,
    pub active_tab_id: Option<String>,
    pub other_windows: Vec<OtherReaderWindowSummaryDto>,
    pub can_cross_window_pointer_hit_test: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabDropPreviewEventDto {
    pub window_id: String,
    pub insert_index: Option<usize>,
    pub dragging_tab_title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReaderWindowRecord {
    pub window_id: String,
    pub window_label: String,
    pub tabs: Vec<ReaderTabRecord>,
    pub active_tab_id: Option<String>,
    pub last_focused_tick: u64,
    pub tab_strip_height_px: f64,
    pub tab_strip_drag_width_px: f64,
    pub pending_creation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveTabDrag {
    pub source_window_id: String,
    pub tab_id: String,
    pub tab_title: String,
    pub last_preview_target_window_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowHitRect {
    pub inner_x: f64,
    pub inner_y: f64,
    pub inner_width: f64,
    pub scale_factor: f64,
    pub is_minimized: bool,
}

#[derive(Debug, Clone)]
pub struct BookOpenSpec {
    pub book_id: String,
    pub book_hash: String,
    pub title: String,
    pub format: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OpenBookOutcome {
    pub window_id: String,
    pub window_label: String,
    pub created_new_window: bool,
    pub focused_existing_window: bool,
}

#[derive(Debug, Clone, Default)]
pub struct TabStateUpdate {
    pub session_id: Option<EpubSessionId>,
    pub current_locator: Option<BookLocatorOutput>,
    pub fraction: Option<f64>,
    pub translation_mode: Option<String>,
    pub target_language: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CloseOrEvictEffect {
    pub sessions_to_close: Vec<EpubSessionId>,
    pub windows_to_close: Vec<String>,
    pub affected_window_ids: Vec<String>,
    pub clear_preview: Option<(String, String)>,
}

#[derive(Debug, Clone, Default)]
pub struct TabStateUpdateOutcome {
    pub updated: bool,
    pub orphaned_session: Option<EpubSessionId>,
}

#[derive(Debug, Clone, Default)]
pub struct WindowDestroyEffect {
    pub removed: bool,
    pub sessions_to_close: Vec<EpubSessionId>,
    pub clear_preview: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TearOffOutcome {
    MovedSingleTabWindow {
        window_id: String,
        window_label: String,
    },
    CreatedNewWindow {
        source_window_id: String,
        new_window_id: String,
        new_window_label: String,
        tab_title: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TearOffRollbackInfo {
    pub source_window_id: String,
    pub new_window_id: String,
    pub tab_pos: usize,
    pub prev_active_tab_id: Option<String>,
}

pub type TearOffWithRollbackResult = (
    TearOffOutcome,
    Option<TearOffRollbackInfo>,
    Option<(String, String)>,
);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DragHitResult {
    pub target_window_id: String,
    pub insert_index: usize,
}

#[derive(Debug)]
pub struct CoordinatorCore {
    windows: HashMap<String, ReaderWindowRecord>,
    label_to_id: HashMap<String, String>,
    active_drag: Option<ActiveTabDrag>,
    focus_tick: u64,
    reset_generation: u64,
    can_cross_window_pointer_hit_test: bool,
}

impl Default for CoordinatorCore {
    fn default() -> Self {
        Self::new(false)
    }
}

impl CoordinatorCore {
    pub fn new(can_cross_window_pointer_hit_test: bool) -> Self {
        Self {
            windows: HashMap::new(),
            label_to_id: HashMap::new(),
            active_drag: None,
            focus_tick: 0,
            reset_generation: 0,
            can_cross_window_pointer_hit_test,
        }
    }

    pub fn reset_generation(&self) -> u64 {
        self.reset_generation
    }

    pub fn mark_window_created(&mut self, window_id: &str) -> bool {
        let Some(win) = self.windows.get_mut(window_id) else {
            return false;
        };
        win.pending_creation = false;
        true
    }

    pub fn can_cross_window_pointer_hit_test(&self) -> bool {
        self.can_cross_window_pointer_hit_test
    }

    pub fn set_can_cross_window_pointer_hit_test(&mut self, value: bool) {
        self.can_cross_window_pointer_hit_test = value;
    }

    pub fn all_window_id_labels(&self) -> Vec<(String, String)> {
        self.windows
            .values()
            .map(|w| (w.window_id.clone(), w.window_label.clone()))
            .collect()
    }

    pub fn active_tab_title(&self, window_id: &str) -> Option<String> {
        let win = self.windows.get(window_id)?;
        win.active_tab_id
            .as_ref()
            .and_then(|aid| win.tabs.iter().find(|t| &t.tab_id == aid))
            .or_else(|| win.tabs.first())
            .map(|t| t.title.clone())
    }

    fn clear_active_drag_if_matches(
        &mut self,
        source_window_id: &str,
        removed_tab_id: Option<&str>,
    ) -> Option<(String, String)> {
        let should_clear = self.active_drag.as_ref().is_some_and(|drag| {
            drag.source_window_id == source_window_id
                && removed_tab_id.is_none_or(|tid| drag.tab_id == tid)
        });
        if !should_clear {
            return None;
        }
        let drag = self.active_drag.take()?;
        let target_id = drag.last_preview_target_window_id?;
        let target_label = self.window_label(&target_id)?;
        Some((target_id, target_label))
    }

    fn next_tick(&mut self) -> u64 {
        self.focus_tick = self.focus_tick.saturating_add(1);
        self.focus_tick
    }

    fn most_recently_focused_window_id(&self) -> Option<String> {
        self.windows
            .values()
            .filter(|w| !w.pending_creation)
            .max_by_key(|w| w.last_focused_tick)
            .map(|w| w.window_id.clone())
    }

    fn new_tab_record(spec: BookOpenSpec) -> ReaderTabRecord {
        ReaderTabRecord {
            tab_id: format!("tab-{}", Uuid::new_v4()),
            book_id: spec.book_id,
            book_hash: spec.book_hash,
            title: spec.title,
            format: spec.format,
            session_id: None,
            current_locator: None,
            fraction: 0.0,
            translation_mode: "original".to_owned(),
            target_language: None,
        }
    }

    pub fn open_book(
        &mut self,
        spec: BookOpenSpec,
        force_new_window: bool,
    ) -> OpenBookOutcome {
        let tick = self.next_tick();

        if !force_new_window && let Some(target_id) = self.most_recently_focused_window_id() {
            let label = {
                let win = self
                    .windows
                    .get_mut(&target_id)
                    .expect("target window exists");
                win.last_focused_tick = tick;
                if let Some(existing_tab) =
                    win.tabs.iter().find(|tab| tab.book_id == spec.book_id)
                {
                    win.active_tab_id = Some(existing_tab.tab_id.clone());
                } else {
                    let tab = Self::new_tab_record(spec);
                    win.active_tab_id = Some(tab.tab_id.clone());
                    win.tabs.push(tab);
                }
                win.window_label.clone()
            };

            return OpenBookOutcome {
                window_id: target_id,
                window_label: label,
                created_new_window: false,
                focused_existing_window: true,
            };
        }

        let window_id = Uuid::new_v4().to_string();
        let window_label = format!("reader-{window_id}");
        let tab = Self::new_tab_record(spec);
        let active_tab_id = Some(tab.tab_id.clone());

        let record = ReaderWindowRecord {
            window_id: window_id.clone(),
            window_label: window_label.clone(),
            tabs: vec![tab],
            active_tab_id,
            last_focused_tick: tick,
            tab_strip_height_px: DEFAULT_TAB_STRIP_HEIGHT_PX,
            tab_strip_drag_width_px: DEFAULT_TAB_STRIP_DRAG_WIDTH_PX,
            pending_creation: true,
        };
        self.label_to_id
            .insert(window_label.clone(), window_id.clone());
        self.windows.insert(window_id.clone(), record);

        OpenBookOutcome {
            window_id,
            window_label,
            created_new_window: true,
            focused_existing_window: false,
        }
    }

    pub fn snapshot(&self, window_id: &str) -> Option<ReaderWindowSnapshotDto> {
        let win = self.windows.get(window_id)?;
        let mut other_windows: Vec<OtherReaderWindowSummaryDto> = self
            .windows
            .values()
            .filter(|other| other.window_id != window_id)
            .map(|other| {
                let active_title = other
                    .active_tab_id
                    .as_ref()
                    .and_then(|aid| other.tabs.iter().find(|t| &t.tab_id == aid))
                    .or_else(|| other.tabs.first())
                    .map(|t| t.title.clone())
                    .unwrap_or_default();
                OtherReaderWindowSummaryDto {
                    window_id: other.window_id.clone(),
                    active_title,
                    tab_count: other.tabs.len(),
                }
            })
            .collect();
        other_windows.sort_by(|a, b| a.window_id.cmp(&b.window_id));

        Some(ReaderWindowSnapshotDto {
            window_id: win.window_id.clone(),
            window_label: win.window_label.clone(),
            tabs: win.tabs.clone(),
            active_tab_id: win.active_tab_id.clone(),
            other_windows,
            can_cross_window_pointer_hit_test: self.can_cross_window_pointer_hit_test,
        })
    }

    pub fn all_window_ids(&self) -> Vec<String> {
        self.windows.keys().cloned().collect()
    }

    pub fn window_label(&self, window_id: &str) -> Option<String> {
        self.windows.get(window_id).map(|w| w.window_label.clone())
    }

    pub fn mark_focused_by_label(&mut self, window_label: &str) -> bool {
        let Some(window_id) = self.label_to_id.get(window_label).cloned() else {
            return false;
        };
        let tick = self.next_tick();
        if let Some(win) = self.windows.get_mut(&window_id) {
            win.last_focused_tick = tick;
            true
        } else {
            false
        }
    }

    pub fn activate_tab(&mut self, window_id: &str, tab_id: &str) -> bool {
        let Some(win) = self.windows.get_mut(window_id) else {
            return false;
        };
        if !win.tabs.iter().any(|t| t.tab_id == tab_id) {
            return false;
        }
        win.active_tab_id = Some(tab_id.to_owned());
        true
    }

    pub fn reorder_tabs(&mut self, window_id: &str, tab_ids: &[String]) -> bool {
        let Some(win) = self.windows.get_mut(window_id) else {
            return false;
        };
        let original = std::mem::take(&mut win.tabs);
        let mut slots: Vec<Option<ReaderTabRecord>> = original.into_iter().map(Some).collect();
        let index_by_id: HashMap<String, usize> = slots
            .iter()
            .enumerate()
            .filter_map(|(idx, slot)| slot.as_ref().map(|t| (t.tab_id.clone(), idx)))
            .collect();
        let mut reordered = Vec::with_capacity(slots.len());
        for id in tab_ids {
            if let Some(&idx) = index_by_id.get(id)
                && let Some(tab) = slots[idx].take()
            {
                reordered.push(tab);
            }
        }
        for leftover in slots.into_iter().flatten() {
            reordered.push(leftover);
        }
        win.tabs = reordered;
        true
    }

    pub fn close_tab(&mut self, window_id: &str, tab_id: &str) -> CloseOrEvictEffect {
        let mut effect = CloseOrEvictEffect::default();
        let (removed, emptied_label) = {
            let Some(win) = self.windows.get_mut(window_id) else {
                return effect;
            };
            let Some(pos) = win.tabs.iter().position(|t| t.tab_id == tab_id) else {
                return effect;
            };

            let removed = win.tabs.remove(pos);
            let emptied_label = if win.tabs.is_empty() {
                Some(win.window_label.clone())
            } else {
                if win.active_tab_id.as_deref() == Some(tab_id) {
                    let next_idx = pos.min(win.tabs.len().saturating_sub(1));
                    win.active_tab_id = win.tabs.get(next_idx).map(|t| t.tab_id.clone());
                }
                None
            };
            (removed, emptied_label)
        };

        if let Some(sid) = removed.session_id {
            effect.sessions_to_close.push(sid);
        }
        effect.clear_preview = self.clear_active_drag_if_matches(window_id, Some(tab_id));

        if let Some(label) = emptied_label {
            self.windows.remove(window_id);
            self.label_to_id.remove(&label);
            effect.windows_to_close.push(label);
        } else {
            effect.affected_window_ids.push(window_id.to_owned());
        }

        effect
    }

    pub fn update_tab_state(
        &mut self,
        window_id: &str,
        tab_id: &str,
        update: TabStateUpdate,
    ) -> bool {
        self.update_tab_state_with_orphan(window_id, tab_id, update)
            .updated
    }

    pub fn update_tab_state_with_orphan(
        &mut self,
        window_id: &str,
        tab_id: &str,
        update: TabStateUpdate,
    ) -> TabStateUpdateOutcome {
        let target_in_window = self
            .windows
            .get(window_id)
            .is_some_and(|win| win.tabs.iter().any(|t| t.tab_id == tab_id));

        if !target_in_window {
            let Some(incoming_sid) = update.session_id else {
                return TabStateUpdateOutcome {
                    updated: false,
                    orphaned_session: None,
                };
            };
            // A late update from the source window after cross-window move/tear-off
            // must never close a session that now belongs to a live tab in another window.
            let session_owned_elsewhere = self.windows.values().any(|w| {
                w.tabs
                    .iter()
                    .any(|t| t.session_id.as_ref() == Some(&incoming_sid))
            });
            if session_owned_elsewhere {
                return TabStateUpdateOutcome {
                    updated: false,
                    orphaned_session: None,
                };
            }
            // If the tab migrated before its initial session_id arrived, attach it to the
            // migrated tab instead of prematurely killing the live backend session.
            for win in self.windows.values_mut() {
                if let Some(migrated_tab) = win.tabs.iter_mut().find(|t| t.tab_id == tab_id)
                    && migrated_tab.session_id.is_none()
                {
                    migrated_tab.session_id = Some(incoming_sid);
                    return TabStateUpdateOutcome {
                        updated: true,
                        orphaned_session: None,
                    };
                }
            }
            return TabStateUpdateOutcome {
                updated: false,
                orphaned_session: Some(incoming_sid),
            };
        }

        let Some(win) = self.windows.get_mut(window_id) else {
            return TabStateUpdateOutcome {
                updated: false,
                orphaned_session: None,
            };
        };
        let Some(tab) = win.tabs.iter_mut().find(|t| t.tab_id == tab_id) else {
            return TabStateUpdateOutcome {
                updated: false,
                orphaned_session: None,
            };
        };

        let mut prev_to_check = None;
        if let Some(sid) = update.session_id {
            if let Some(prev) = tab.session_id.take()
                && prev != sid
            {
                prev_to_check = Some(prev);
            }
            tab.session_id = Some(sid);
        }
        if let Some(locator) = update.current_locator {
            tab.current_locator = Some(locator);
        }
        if let Some(fraction) = update.fraction
            && fraction.is_finite()
        {
            tab.fraction = fraction.clamp(0.0, 1.0);
        }
        if let Some(mode) = update.translation_mode
            && matches!(mode.as_str(), "original" | "bilingual" | "translated_only")
        {
            tab.translation_mode = mode;
        }
        if let Some(lang) = update.target_language {
            let trimmed = lang.trim();
            tab.target_language = if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_owned())
            };
        }
        let orphaned_session = prev_to_check.filter(|prev| {
            !self
                .windows
                .values()
                .any(|w| w.tabs.iter().any(|t| t.session_id.as_ref() == Some(prev)))
        });
        TabStateUpdateOutcome {
            updated: true,
            orphaned_session,
        }
    }

    pub fn report_tab_strip_metrics(
        &mut self,
        window_id: &str,
        height_px: f64,
        drag_width_px: f64,
    ) -> bool {
        let Some(win) = self.windows.get_mut(window_id) else {
            return false;
        };
        if height_px.is_finite() && height_px > 0.0 {
            win.tab_strip_height_px = height_px.clamp(24.0, 120.0);
        }
        if drag_width_px.is_finite() && drag_width_px > 0.0 {
            win.tab_strip_drag_width_px = drag_width_px.max(120.0);
        }
        true
    }

    pub fn move_tab_to_window(
        &mut self,
        source_window_id: &str,
        tab_id: &str,
        target_window_id: &str,
        target_index: Option<usize>,
    ) -> Option<CloseOrEvictEffect> {
        if source_window_id == target_window_id {
            return None;
        }
        if !self.windows.contains_key(target_window_id) {
            return None;
        }

        let (moved_tab, emptied_source_label) = {
            let src = self.windows.get_mut(source_window_id)?;
            let pos = src.tabs.iter().position(|t| t.tab_id == tab_id)?;
            let removed = src.tabs.remove(pos);
            let emptied_label = if src.tabs.is_empty() {
                Some(src.window_label.clone())
            } else {
                if src.active_tab_id.as_deref() == Some(tab_id) {
                    let next_idx = pos.min(src.tabs.len().saturating_sub(1));
                    src.active_tab_id = src.tabs.get(next_idx).map(|t| t.tab_id.clone());
                }
                None
            };
            (removed, emptied_label)
        };

        let mut effect = CloseOrEvictEffect {
            clear_preview: self.clear_active_drag_if_matches(source_window_id, Some(tab_id)),
            ..Default::default()
        };
        if let Some(label) = emptied_source_label {
            self.windows.remove(source_window_id);
            self.label_to_id.remove(&label);
            effect.windows_to_close.push(label);
        } else {
            effect
                .affected_window_ids
                .push(source_window_id.to_owned());
        }

        let tick = self.next_tick();
        if let Some(target) = self.windows.get_mut(target_window_id) {
            let idx = target_index
                .unwrap_or(target.tabs.len())
                .min(target.tabs.len());
            let moved_id = moved_tab.tab_id.clone();
            target.tabs.insert(idx, moved_tab);
            target.active_tab_id = Some(moved_id);
            target.last_focused_tick = tick;
            effect
                .affected_window_ids
                .push(target_window_id.to_owned());
        }

        Some(effect)
    }

    pub fn tear_off_tab(
        &mut self,
        source_window_id: &str,
        tab_id: &str,
    ) -> Option<TearOffOutcome> {
        self.tear_off_tab_with_rollback(source_window_id, tab_id)
            .map(|(outcome, _, _)| outcome)
    }

    pub fn tear_off_tab_with_rollback(
        &mut self,
        source_window_id: &str,
        tab_id: &str,
    ) -> Option<TearOffWithRollbackResult> {
        let src = self.windows.get(source_window_id)?;
        let tab_pos = src.tabs.iter().position(|t| t.tab_id == tab_id)?;
        let is_single_tab = src.tabs.len() == 1;
        let single_label = is_single_tab.then(|| src.window_label.clone());

        let clear_preview = self.clear_active_drag_if_matches(source_window_id, Some(tab_id));

        if let Some(label) = single_label {
            return Some((
                TearOffOutcome::MovedSingleTabWindow {
                    window_id: source_window_id.to_owned(),
                    window_label: label,
                },
                None,
                clear_preview,
            ));
        }

        let (removed, prev_active_tab_id) = {
            let src_mut = self.windows.get_mut(source_window_id)?;
            let prev_active = src_mut.active_tab_id.clone();
            let removed = src_mut.tabs.remove(tab_pos);
            if src_mut.active_tab_id.as_deref() == Some(tab_id) {
                let next_idx = tab_pos.min(src_mut.tabs.len().saturating_sub(1));
                src_mut.active_tab_id =
                    src_mut.tabs.get(next_idx).map(|t| t.tab_id.clone());
            }
            (removed, prev_active)
        };

        let tick = self.next_tick();
        let new_window_id = Uuid::new_v4().to_string();
        let new_window_label = format!("reader-{new_window_id}");
        let tab_title = removed.title.clone();
        let active_tab_id = Some(removed.tab_id.clone());

        let record = ReaderWindowRecord {
            window_id: new_window_id.clone(),
            window_label: new_window_label.clone(),
            tabs: vec![removed],
            active_tab_id,
            last_focused_tick: tick,
            tab_strip_height_px: DEFAULT_TAB_STRIP_HEIGHT_PX,
            tab_strip_drag_width_px: DEFAULT_TAB_STRIP_DRAG_WIDTH_PX,
            pending_creation: true,
        };
        self.label_to_id
            .insert(new_window_label.clone(), new_window_id.clone());
        self.windows.insert(new_window_id.clone(), record);

        let rollback = TearOffRollbackInfo {
            source_window_id: source_window_id.to_owned(),
            new_window_id: new_window_id.clone(),
            tab_pos,
            prev_active_tab_id,
        };

        Some((
            TearOffOutcome::CreatedNewWindow {
                source_window_id: source_window_id.to_owned(),
                new_window_id,
                new_window_label,
                tab_title,
            },
            Some(rollback),
            clear_preview,
        ))
    }

    pub fn rollback_tear_off(&mut self, rollback: TearOffRollbackInfo) -> Option<EpubSessionId> {
        let new_win = self.windows.remove(&rollback.new_window_id)?;
        self.label_to_id.remove(&new_win.window_label);
        let tab = new_win.tabs.into_iter().next()?;
        if let Some(src) = self.windows.get_mut(&rollback.source_window_id) {
            let idx = rollback.tab_pos.min(src.tabs.len());
            src.tabs.insert(idx, tab);
            src.active_tab_id = rollback.prev_active_tab_id;
            None
        } else {
            tab.session_id
        }
    }

    pub fn drag_start(&mut self, window_id: &str, tab_id: &str) -> bool {
        let Some(win) = self.windows.get(window_id) else {
            return false;
        };
        let Some(tab) = win.tabs.iter().find(|t| t.tab_id == tab_id) else {
            return false;
        };
        self.active_drag = Some(ActiveTabDrag {
            source_window_id: window_id.to_owned(),
            tab_id: tab_id.to_owned(),
            tab_title: tab.title.clone(),
            last_preview_target_window_id: None,
        });
        true
    }

    pub fn take_active_drag(&mut self) -> Option<ActiveTabDrag> {
        self.active_drag.take()
    }

    pub fn active_drag(&self) -> Option<&ActiveTabDrag> {
        self.active_drag.as_ref()
    }

    pub fn set_last_preview_target(&mut self, target_window_id: Option<String>) {
        if let Some(drag) = self.active_drag.as_mut() {
            drag.last_preview_target_window_id = target_window_id;
        }
    }

    pub fn hit_test_drag(
        &self,
        source_window_id: &str,
        cursor_phys_x: f64,
        cursor_phys_y: f64,
        rects_by_window_id: &HashMap<String, WindowHitRect>,
    ) -> Option<DragHitResult> {
        if !cursor_phys_x.is_finite() || !cursor_phys_y.is_finite() {
            return None;
        }

        let mut candidates: Vec<&ReaderWindowRecord> = self
            .windows
            .values()
            .filter(|w| w.window_id != source_window_id && !w.pending_creation)
            .collect();
        // Sort descending by last_focused_tick so topmost overlapping window wins.
        candidates.sort_by_key(|w| std::cmp::Reverse(w.last_focused_tick));

        for win in candidates {
            let Some(rect) = rects_by_window_id.get(&win.window_id) else {
                continue;
            };
            if rect.is_minimized || rect.scale_factor <= 0.0 || rect.inner_width <= 0.0 {
                continue;
            }

            let strip_width_phys =
                (win.tab_strip_drag_width_px * rect.scale_factor).min(rect.inner_width);
            let strip_height_phys = win.tab_strip_height_px * rect.scale_factor;

            let within_x = cursor_phys_x >= rect.inner_x
                && cursor_phys_x <= rect.inner_x + strip_width_phys;
            let within_y = cursor_phys_y >= rect.inner_y
                && cursor_phys_y <= rect.inner_y + strip_height_phys;

            if within_x && within_y {
                let local_logical_x =
                    ((cursor_phys_x - rect.inner_x) / rect.scale_factor).max(0.0);
                let slot_width = if win.tabs.is_empty() {
                    ESTIMATED_TAB_SLOT_WIDTH_PX
                } else {
                    (win.tab_strip_drag_width_px / win.tabs.len() as f64)
                        .clamp(72.0, ESTIMATED_TAB_SLOT_WIDTH_PX)
                };
                let raw_idx = (local_logical_x / slot_width).round() as usize;
                let insert_index = raw_idx.min(win.tabs.len());
                return Some(DragHitResult {
                    target_window_id: win.window_id.clone(),
                    insert_index,
                });
            }
        }

        None
    }

    pub fn evict_books(&mut self, book_ids: &[String]) -> CloseOrEvictEffect {
        let mut effect = CloseOrEvictEffect::default();
        if book_ids.is_empty() {
            return effect;
        }
        let evicted: HashSet<&str> = book_ids.iter().map(String::as_str).collect();
        let window_ids: Vec<String> = self.windows.keys().cloned().collect();
        let active_drag_key = self
            .active_drag
            .as_ref()
            .map(|d| (d.source_window_id.clone(), d.tab_id.clone()));
        let mut evicted_active_drag = false;

        for wid in window_ids {
            let mut emptied_label: Option<String> = None;
            let mut modified = false;

            if let Some(win) = self.windows.get_mut(&wid) {
                let before = win.tabs.len();
                let mut kept = Vec::with_capacity(before);
                for tab in win.tabs.drain(..) {
                    if evicted.contains(tab.book_id.as_str()) {
                        modified = true;
                        if active_drag_key
                            .as_ref()
                            .is_some_and(|(drag_wid, drag_tid)| drag_wid == &wid && drag_tid == &tab.tab_id)
                        {
                            evicted_active_drag = true;
                        }
                        if let Some(sid) = tab.session_id {
                            effect.sessions_to_close.push(sid);
                        }
                    } else {
                        kept.push(tab);
                    }
                }
                win.tabs = kept;
                if modified {
                    if win.tabs.is_empty() {
                        emptied_label = Some(win.window_label.clone());
                    } else if let Some(active) = win.active_tab_id.as_deref()
                        && !win.tabs.iter().any(|t| t.tab_id == active)
                    {
                        win.active_tab_id = win.tabs.first().map(|t| t.tab_id.clone());
                    }
                }
            }

            if let Some(label) = emptied_label {
                self.windows.remove(&wid);
                self.label_to_id.remove(&label);
                effect.windows_to_close.push(label);
            } else if modified {
                effect.affected_window_ids.push(wid);
            }
        }

        if evicted_active_drag && let Some((drag_wid, drag_tid)) = active_drag_key {
            effect.clear_preview = self.clear_active_drag_if_matches(&drag_wid, Some(&drag_tid));
        }

        effect
    }

    pub fn remove_window_on_destroy(&mut self, window_label: &str) -> Vec<EpubSessionId> {
        self.remove_window_on_destroy_effect(window_label)
            .sessions_to_close
    }

    pub fn remove_window_on_destroy_effect(&mut self, window_label: &str) -> WindowDestroyEffect {
        let Some(window_id) = self.label_to_id.remove(window_label) else {
            return WindowDestroyEffect::default();
        };
        let Some(win) = self.windows.remove(&window_id) else {
            return WindowDestroyEffect::default();
        };
        let clear_preview = self.clear_active_drag_if_matches(&window_id, None);
        WindowDestroyEffect {
            removed: true,
            sessions_to_close: win.tabs.into_iter().filter_map(|tab| tab.session_id).collect(),
            clear_preview,
        }
    }

    pub fn take_all_windows(&mut self) -> CloseOrEvictEffect {
        let mut effect = CloseOrEvictEffect::default();
        self.reset_generation = self.reset_generation.wrapping_add(1);
        self.active_drag = None;
        self.label_to_id.clear();
        for (_, win) in self.windows.drain() {
            effect.windows_to_close.push(win.window_label);
            for tab in win.tabs {
                if let Some(sid) = tab.session_id {
                    effect.sessions_to_close.push(sid);
                }
            }
        }
        effect
    }
}

pub struct ReaderWindowCoordinator {
    core: Mutex<CoordinatorCore>,
    open_lock: tokio::sync::Mutex<()>,
}

impl Default for ReaderWindowCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

impl ReaderWindowCoordinator {
    pub fn new() -> Self {
        Self {
            core: Mutex::new(CoordinatorCore::new(Self::detect_pointer_hit_test_support())),
            open_lock: tokio::sync::Mutex::new(()),
        }
    }

    fn detect_pointer_hit_test_support() -> bool {
        #[cfg(target_os = "linux")]
        {
            let is_wayland = std::env::var_os("WAYLAND_DISPLAY")
                .is_some_and(|v| !v.is_empty())
                || std::env::var("XDG_SESSION_TYPE")
                    .is_ok_and(|v| v.eq_ignore_ascii_case("wayland"));
            !is_wayland
        }
        #[cfg(target_os = "windows")]
        {
            true
        }
        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            false
        }
    }

    fn emit_snapshots_for(&self, app: &AppHandle, window_ids: &[String]) {
        let snapshots: Vec<ReaderWindowSnapshotDto> = {
            let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            window_ids
                .iter()
                .filter_map(|id| core.snapshot(id))
                .collect()
        };
        for snap in snapshots {
            let _ = app.emit_to(&snap.window_label, READER_WINDOW_STATE_EVENT, &snap);
        }
    }

    fn emit_all_snapshots(&self, app: &AppHandle) {
        let ids = {
            let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.all_window_ids()
        };
        self.emit_snapshots_for(app, &ids);
    }

    fn emit_clear_preview(app: &AppHandle, clear_preview: Option<(String, String)>) {
        if let Some((target_id, target_label)) = clear_preview {
            let _ = app.emit_to(
                &target_label,
                READER_TAB_DROP_PREVIEW_EVENT,
                TabDropPreviewEventDto {
                    window_id: target_id,
                    insert_index: None,
                    dragging_tab_title: None,
                },
            );
        }
    }

    async fn close_sessions(runtime: &ClientRuntime, sessions: Vec<EpubSessionId>) {
        for sid in sessions {
            let _ = runtime.close_epub(sid).await;
        }
    }

    fn spawn_close_sessions(runtime: &std::sync::Arc<ClientRuntime>, sessions: Vec<EpubSessionId>) {
        if sessions.is_empty() {
            return;
        }
        let runtime = std::sync::Arc::clone(runtime);
        tauri::async_runtime::spawn(async move {
            Self::close_sessions(&runtime, sessions).await;
        });
    }

    #[cfg(desktop)]
    fn focus_existing_window(
        &self,
        app: &AppHandle,
        window_label: &str,
        title: &str,
        position_phys: Option<(i32, i32)>,
    ) -> bool {
        let Some(existing) = app.get_webview_window(window_label) else {
            return false;
        };
        let _ = existing.set_title(title);
        let _ = existing.unminimize();
        let _ = existing.set_focus();
        if let Some((x, y)) = position_phys {
            let _ = existing.set_position(Position::Physical(PhysicalPosition::new(x, y)));
        }
        true
    }

    #[cfg(not(desktop))]
    fn focus_existing_window(
        &self,
        _app: &AppHandle,
        _window_label: &str,
        _title: &str,
        _position_phys: Option<(i32, i32)>,
    ) -> bool {
        true
    }

    #[cfg(desktop)]
    fn create_new_window(
        &self,
        app: &AppHandle,
        window_id: &str,
        window_label: &str,
        title: &str,
        position_phys: Option<(i32, i32)>,
    ) -> Result<(), CommandErrorPayload> {
        let url = WebviewUrl::App(
            format!("index.html?window=reader&windowId={window_id}").into(),
        );
        let builder = WebviewWindowBuilder::new(app, window_label, url)
            .title(title)
            .decorations(false)
            .resizable(true)
            .inner_size(1120.0, 760.0)
            .min_inner_size(640.0, 480.0);

        let created = builder.build().map_err(|err| {
            tracing::warn!(window_label, error = %err, "failed to create reader window");
            app.state::<crate::ClientAppState>()
                .runtime
                .error_dispatcher()
                .command_only(ErrorCode::CommonInternalServerError)
        })?;

        if let Some((x, y)) = position_phys {
            let _ = created.set_position(Position::Physical(PhysicalPosition::new(x, y)));
        }

        if created.inner_position().is_err() {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.set_can_cross_window_pointer_hit_test(false);
        }
        Ok(())
    }

    #[cfg(not(desktop))]
    fn create_new_window(
        &self,
        _app: &AppHandle,
        _window_id: &str,
        _window_label: &str,
        _title: &str,
        _position_phys: Option<(i32, i32)>,
    ) -> Result<(), CommandErrorPayload> {
        Ok(())
    }

    fn close_window_labels(app: &AppHandle, labels: &[String]) {
        #[cfg(desktop)]
        for label in labels {
            if let Some(win) = app.get_webview_window(label) {
                let _ = win.close();
            }
        }
        #[cfg(not(desktop))]
        let _ = (app, labels);
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn open_book(
        &self,
        app: &AppHandle,
        runtime: &std::sync::Arc<ClientRuntime>,
        book_id: String,
        book_hash: Option<String>,
        title: Option<String>,
        format: Option<String>,
        new_window: bool,
    ) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
        let start_gen = {
            let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.reset_generation()
        };
        let validated_hash =
            runtime.validate_reader_open_target(&book_id, book_hash.as_deref())?;

        let resolved = if let (Some(hash), Some(book_title), Some(fmt)) =
            (validated_hash, title, format)
        {
            BookOpenSpec {
                book_id,
                book_hash: hash,
                title: book_title,
                format: fmt,
            }
        } else {
            let books = runtime.list_books().await?;
            let found = books.into_iter().find(|b| b.id == book_id).ok_or_else(|| {
                runtime
                    .error_dispatcher()
                    .dispatch_operation(ErrorCode::BookNotFound)
            })?;
            BookOpenSpec {
                book_id: found.id,
                book_hash: found.hash,
                title: found.title,
                format: found.file_type,
            }
        };

        if !resolved.format.trim().eq_ignore_ascii_case("EPUB") {
            return Err(runtime
                .error_dispatcher()
                .dispatch_operation(ErrorCode::ReaderUnsupportedFormat));
        }

        let resolved = BookOpenSpec {
            format: "EPUB".to_owned(),
            ..resolved
        };

        let _open_guard = self.open_lock.lock().await;
        let window_title = format!("{} - ShiroReader", resolved.title);
        let (mut outcome, mut effective_title) = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            runtime.ensure_reader_ready()?;
            if core.reset_generation() != start_gen {
                return Err(runtime
                    .error_dispatcher()
                    .dispatch_operation(ErrorCode::RuntimeNotReady));
            }
            let outcome = core.open_book(resolved.clone(), new_window);
            let title = core
                .active_tab_title(&outcome.window_id)
                .map(|t| format!("{t} - ShiroReader"))
                .unwrap_or_else(|| window_title.clone());
            (outcome, title)
        };

        if !outcome.created_new_window {
            let focused = self.focus_existing_window(
                app,
                &outcome.window_label,
                &effective_title,
                None,
            );
            if !focused {
                // Existing OS window closed concurrently before focus; retire the stale record
                // and allocate a fresh window with a brand-new label so a queued Destroyed event
                // from the old window cannot wipe the newly created window.
                let (stale_effect, next_result) = {
                    let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                    let stale = core.remove_window_on_destroy_effect(&outcome.window_label);
                    let next = match runtime.ensure_reader_ready() {
                        Ok(()) if core.reset_generation() == start_gen => {
                            let fresh = core.open_book(resolved, true);
                            let title = core
                                .active_tab_title(&fresh.window_id)
                                .map(|t| format!("{t} - ShiroReader"))
                                .unwrap_or_else(|| window_title.clone());
                            Ok((fresh, title))
                        }
                        Ok(()) => Err(runtime
                            .error_dispatcher()
                            .dispatch_operation(ErrorCode::RuntimeNotReady)),
                        Err(err) => Err(err),
                    };
                    (stale, next)
                };
                if !stale_effect.sessions_to_close.is_empty() {
                    Self::spawn_close_sessions(runtime, stale_effect.sessions_to_close);
                }
                Self::emit_clear_preview(app, stale_effect.clear_preview);
                let (fresh_outcome, fresh_title) = next_result?;
                outcome = fresh_outcome;
                effective_title = fresh_title;
            }
        }

        if outcome.created_new_window {
            if let Err(err) = self.create_new_window(
                app,
                &outcome.window_id,
                &outcome.window_label,
                &effective_title,
                None,
            ) {
                let effect = {
                    let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                    core.remove_window_on_destroy_effect(&outcome.window_label)
                };
                Self::emit_clear_preview(app, effect.clear_preview);
                Self::spawn_close_sessions(runtime, effect.sessions_to_close);
                return Err(err);
            }

            let invalid_effect = {
                let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                let ready_ok = runtime.ensure_reader_ready().is_ok();
                let still_valid = ready_ok
                    && core.reset_generation() == start_gen
                    && core.mark_window_created(&outcome.window_id);
                if still_valid {
                    None
                } else {
                    Some(core.remove_window_on_destroy_effect(&outcome.window_label))
                }
            };
            if let Some(effect) = invalid_effect {
                Self::emit_clear_preview(app, effect.clear_preview);
                Self::spawn_close_sessions(runtime, effect.sessions_to_close);
                Self::close_window_labels(app, std::slice::from_ref(&outcome.window_label));
                return Err(runtime
                    .error_dispatcher()
                    .dispatch_operation(ErrorCode::RuntimeNotReady));
            }
        }

        self.emit_all_snapshots(app);

        let snapshot = {
            let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.snapshot(&outcome.window_id)
        };
        let Some(snapshot) = snapshot else {
            if outcome.created_new_window {
                Self::close_window_labels(app, std::slice::from_ref(&outcome.window_label));
            }
            return Err(runtime
                .error_dispatcher()
                .command_only(ErrorCode::CommonInternalServerError));
        };

        Ok(snapshot)
    }

    pub fn get_window_snapshot(
        &self,
        runtime: &ClientRuntime,
        window_id: &str,
    ) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
        let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        core.snapshot(window_id).ok_or_else(|| {
            runtime
                .error_dispatcher()
                .command_only(ErrorCode::CommonInternalServerError)
        })
    }

    pub fn activate_tab(
        &self,
        app: &AppHandle,
        runtime: &ClientRuntime,
        window_id: &str,
        tab_id: &str,
    ) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
        let (snapshot, active_title) = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            if !core.activate_tab(window_id, tab_id) {
                return Err(runtime
                    .error_dispatcher()
                    .command_only(ErrorCode::CommonInternalServerError));
            }
            let title = core.active_tab_title(window_id);
            let snap = core.snapshot(window_id).ok_or_else(|| {
                runtime
                    .error_dispatcher()
                    .command_only(ErrorCode::CommonInternalServerError)
            })?;
            (snap, title)
        };
        #[cfg(desktop)]
        if let Some(title) = active_title
            && let Some(win) = app.get_webview_window(&snapshot.window_label)
        {
            let _ = win.set_title(&format!("{title} - ShiroReader"));
        }
        #[cfg(not(desktop))]
        let _ = active_title;
        self.emit_all_snapshots(app);
        Ok(snapshot)
    }

    pub fn reorder_tabs(
        &self,
        app: &AppHandle,
        runtime: &ClientRuntime,
        window_id: &str,
        tab_ids: &[String],
    ) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
        let snapshot = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            if !core.reorder_tabs(window_id, tab_ids) {
                return Err(runtime
                    .error_dispatcher()
                    .command_only(ErrorCode::CommonInternalServerError));
            }
            core.snapshot(window_id).ok_or_else(|| {
                runtime
                    .error_dispatcher()
                    .command_only(ErrorCode::CommonInternalServerError)
            })?
        };
        self.emit_snapshots_for(app, &[window_id.to_owned()]);
        Ok(snapshot)
    }

    pub async fn close_tab(
        &self,
        app: &AppHandle,
        runtime: &ClientRuntime,
        window_id: &str,
        tab_id: &str,
    ) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
        let (effect, snapshot_opt, can_hit_test) = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            let effect = core.close_tab(window_id, tab_id);
            let snap = core.snapshot(window_id);
            let can_hit_test = core.can_cross_window_pointer_hit_test();
            (effect, snap, can_hit_test)
        };

        Self::emit_clear_preview(app, effect.clear_preview);
        Self::close_sessions(runtime, effect.sessions_to_close).await;
        Self::close_window_labels(app, &effect.windows_to_close);
        self.emit_all_snapshots(app);

        Ok(snapshot_opt.unwrap_or_else(|| ReaderWindowSnapshotDto {
            window_id: window_id.to_owned(),
            window_label: format!("reader-{window_id}"),
            tabs: Vec::new(),
            active_tab_id: None,
            other_windows: Vec::new(),
            can_cross_window_pointer_hit_test: can_hit_test,
        }))
    }

    pub fn update_tab_state(
        &self,
        runtime: &std::sync::Arc<ClientRuntime>,
        window_id: &str,
        tab_id: &str,
        update: TabStateUpdate,
    ) {
        let outcome = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.update_tab_state_with_orphan(window_id, tab_id, update)
        };
        if let Some(sid) = outcome.orphaned_session {
            Self::spawn_close_sessions(runtime, vec![sid]);
        }
    }

    pub fn report_tab_strip_metrics(
        &self,
        window_id: &str,
        height_px: f64,
        drag_width_px: f64,
    ) {
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        let _ = core.report_tab_strip_metrics(window_id, height_px, drag_width_px);
    }

    pub fn move_tab_to_window(
        &self,
        app: &AppHandle,
        runtime: &ClientRuntime,
        source_window_id: &str,
        tab_id: &str,
        target_window_id: &str,
        target_index: Option<usize>,
    ) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
        let (effect, target_label, target_title, snapshot) = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            let effect = core
                .move_tab_to_window(source_window_id, tab_id, target_window_id, target_index)
                .ok_or_else(|| {
                    runtime
                        .error_dispatcher()
                        .command_only(ErrorCode::CommonInternalServerError)
                })?;
            let target_label = core.window_label(target_window_id);
            let target_title = core.active_tab_title(target_window_id);
            let snapshot = core.snapshot(target_window_id).ok_or_else(|| {
                runtime
                    .error_dispatcher()
                    .command_only(ErrorCode::CommonInternalServerError)
            })?;
            (effect, target_label, target_title, snapshot)
        };

        Self::emit_clear_preview(app, effect.clear_preview);
        Self::close_window_labels(app, &effect.windows_to_close);
        #[cfg(desktop)]
        if let Some(label) = target_label
            && let Some(win) = app.get_webview_window(&label)
        {
            if let Some(title) = target_title {
                let _ = win.set_title(&format!("{title} - ShiroReader"));
            }
            let _ = win.unminimize();
            let _ = win.set_focus();
        }
        #[cfg(not(desktop))]
        let _ = (target_label, target_title);

        self.emit_all_snapshots(app);
        Ok(snapshot)
    }

    pub fn tear_off_tab(
        &self,
        app: &AppHandle,
        runtime: &std::sync::Arc<ClientRuntime>,
        source_window_id: &str,
        tab_id: &str,
        screen_x: Option<f64>,
        screen_y: Option<f64>,
    ) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
        self.tear_off_tab_with_scale(
            app,
            runtime,
            source_window_id,
            tab_id,
            screen_x,
            screen_y,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn tear_off_tab_with_scale(
        &self,
        app: &AppHandle,
        runtime: &std::sync::Arc<ClientRuntime>,
        source_window_id: &str,
        tab_id: &str,
        screen_x: Option<f64>,
        screen_y: Option<f64>,
        source_scale_override: Option<f64>,
    ) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
        let source_label = {
            let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.window_label(source_window_id)
        };
        #[cfg(desktop)]
        let source_scale = source_scale_override.unwrap_or_else(|| {
            source_label
                .as_deref()
                .and_then(|label| app.get_webview_window(label))
                .and_then(|win| win.scale_factor().ok())
                .filter(|scale| scale.is_finite() && *scale > 0.0)
                .unwrap_or(1.0)
        });
        #[cfg(not(desktop))]
        let _ = (source_label, source_scale_override);

        let (start_gen, outcome, rollback, clear_preview) = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            let start_gen = core.reset_generation();
            let (outcome, rollback, clear_preview) = core
                .tear_off_tab_with_rollback(source_window_id, tab_id)
                .ok_or_else(|| {
                    runtime
                        .error_dispatcher()
                        .command_only(ErrorCode::CommonInternalServerError)
                })?;
            (start_gen, outcome, rollback, clear_preview)
        };
        Self::emit_clear_preview(app, clear_preview);

        let target_window_id = match outcome {
            TearOffOutcome::MovedSingleTabWindow {
                window_id,
                window_label,
            } => {
                #[cfg(desktop)]
                if let (Some(x), Some(y)) = (screen_x, screen_y)
                    && x.is_finite()
                    && y.is_finite()
                    && let Some(win) = app.get_webview_window(&window_label)
                {
                    if win.is_maximized().unwrap_or(false) {
                        let _ = win.unmaximize();
                    }
                    let scale = win.scale_factor().unwrap_or(source_scale);
                    let px = (x - 80.0 * scale).round() as i32;
                    let py = (y - 18.0 * scale).round() as i32;
                    let _ = win.set_position(Position::Physical(PhysicalPosition::new(px, py)));
                }
                #[cfg(not(desktop))]
                let _ = (window_label, screen_x, screen_y);
                window_id
            }
            TearOffOutcome::CreatedNewWindow {
                new_window_id,
                new_window_label,
                tab_title,
                ..
            } => {
                #[cfg(desktop)]
                let pos = match (screen_x, screen_y) {
                    (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Some((
                        (x - 80.0 * source_scale).round() as i32,
                        (y - 18.0 * source_scale).round() as i32,
                    )),
                    _ => None,
                };
                #[cfg(not(desktop))]
                let pos = None;

                if let Err(err) = self.create_new_window(
                    app,
                    &new_window_id,
                    &new_window_label,
                    &format!("{tab_title} - ShiroReader"),
                    pos,
                ) {
                    if let Some(rollback_info) = rollback {
                        let orphan_sid = {
                            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                            core.rollback_tear_off(rollback_info)
                        };
                        if let Some(sid) = orphan_sid {
                            Self::spawn_close_sessions(runtime, vec![sid]);
                        }
                    }
                    return Err(err);
                }

                let invalid_effect = {
                    let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                    if core.reset_generation() != start_gen
                        || !core.mark_window_created(&new_window_id)
                    {
                        Some(core.remove_window_on_destroy_effect(&new_window_label))
                    } else {
                        None
                    }
                };
                if let Some(effect) = invalid_effect {
                    Self::emit_clear_preview(app, effect.clear_preview);
                    Self::spawn_close_sessions(runtime, effect.sessions_to_close);
                    Self::close_window_labels(app, std::slice::from_ref(&new_window_label));
                    return Err(runtime
                        .error_dispatcher()
                        .command_only(ErrorCode::CommonInternalServerError));
                }
                new_window_id
            }
        };

        self.emit_all_snapshots(app);
        self.get_window_snapshot(runtime, &target_window_id)
    }

    pub fn drag_start(&self, window_id: &str, tab_id: &str) {
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        let _ = core.drag_start(window_id, tab_id);
    }

    #[cfg(desktop)]
    fn collect_window_hit_rects_for_labels(
        app: &AppHandle,
        window_labels: &[(String, String)],
    ) -> HashMap<String, WindowHitRect> {
        let mut rects = HashMap::new();
        for (wid, label) in window_labels {
            let Some(win) = app.get_webview_window(label) else {
                continue;
            };
            let Ok(pos) = win.inner_position() else {
                continue;
            };
            let Ok(size) = win.inner_size() else {
                continue;
            };
            let scale = win.scale_factor().unwrap_or(1.0);
            let is_minimized = win.is_minimized().unwrap_or(false);
            rects.insert(
                wid.clone(),
                WindowHitRect {
                    inner_x: f64::from(pos.x),
                    inner_y: f64::from(pos.y),
                    inner_width: f64::from(size.width),
                    scale_factor: scale,
                    is_minimized,
                },
            );
        }
        rects
    }

    #[cfg(desktop)]
    fn client_to_physical_for_label(
        app: &AppHandle,
        source_label: &str,
        client_x: f64,
        client_y: f64,
    ) -> Option<(f64, f64)> {
        if !client_x.is_finite() || !client_y.is_finite() {
            return None;
        }
        let win = app.get_webview_window(source_label)?;
        let pos = win.inner_position().ok()?;
        let scale = win.scale_factor().unwrap_or(1.0);
        Some((
            f64::from(pos.x) + client_x * scale,
            f64::from(pos.y) + client_y * scale,
        ))
    }

    pub fn drag_over(&self, app: &AppHandle, window_id: &str, client_x: f64, client_y: f64) {
        #[cfg(desktop)]
        {
            let (source_label, window_labels) = {
                let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                let Some(drag) = core.active_drag() else {
                    return;
                };
                if drag.source_window_id != window_id {
                    return;
                }
                let Some(source_label) = core.window_label(window_id) else {
                    return;
                };
                (source_label, core.all_window_id_labels())
            };

            let phys = Self::client_to_physical_for_label(app, &source_label, client_x, client_y);
            let rects = Self::collect_window_hit_rects_for_labels(app, &window_labels);

            let (prev_to_clear, hit_to_emit, tab_title) = {
                let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                let Some(drag) = core.active_drag() else {
                    return;
                };
                if drag.source_window_id != window_id {
                    return;
                }
                let tab_title = drag.tab_title.clone();
                let prev_id = drag.last_preview_target_window_id.clone();
                let hit = phys.and_then(|(px, py)| core.hit_test_drag(window_id, px, py, &rects));
                core.set_last_preview_target(hit.as_ref().map(|h| h.target_window_id.clone()));
                let prev_to_clear = prev_id.and_then(|pid| {
                    if hit.as_ref().is_none_or(|h| h.target_window_id != pid) {
                        core.window_label(&pid).map(|label| (pid, label))
                    } else {
                        None
                    }
                });
                let hit_to_emit = hit.and_then(|h| {
                    core.window_label(&h.target_window_id)
                        .map(|label| (h, label))
                });
                (prev_to_clear, hit_to_emit, tab_title)
            };

            Self::emit_clear_preview(app, prev_to_clear);

            if let Some((hit, target_label)) = hit_to_emit {
                let _ = app.emit_to(
                    &target_label,
                    READER_TAB_DROP_PREVIEW_EVENT,
                    TabDropPreviewEventDto {
                        window_id: hit.target_window_id,
                        insert_index: Some(hit.insert_index),
                        dragging_tab_title: Some(tab_title),
                    },
                );
            }
        }
        #[cfg(not(desktop))]
        let _ = (app, window_id, client_x, client_y);
    }

    pub fn drag_commit(
        &self,
        app: &AppHandle,
        runtime: &std::sync::Arc<ClientRuntime>,
        window_id: &str,
        client_x: f64,
        client_y: f64,
    ) -> Result<(), CommandErrorPayload> {
        #[cfg(desktop)]
        {
            let (source_label, window_labels) = {
                let core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                let Some(drag) = core.active_drag() else {
                    return Ok(());
                };
                if drag.source_window_id != window_id {
                    return Ok(());
                }
                let Some(source_label) = core.window_label(window_id) else {
                    return Ok(());
                };
                (source_label, core.all_window_id_labels())
            };

            let phys = Self::client_to_physical_for_label(app, &source_label, client_x, client_y);
            let rects = Self::collect_window_hit_rects_for_labels(app, &window_labels);
            let source_scale = app
                .get_webview_window(&source_label)
                .and_then(|w| w.scale_factor().ok())
                .filter(|s| s.is_finite() && *s > 0.0)
                .unwrap_or(1.0);

            let (drag, hit, prev_to_clear) = {
                let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
                if !core
                    .active_drag()
                    .is_some_and(|d| d.source_window_id == window_id)
                {
                    return Ok(());
                }
                let hit = phys.and_then(|(px, py)| core.hit_test_drag(window_id, px, py, &rects));
                let drag = core.take_active_drag();
                let prev_to_clear = drag
                    .as_ref()
                    .and_then(|d| d.last_preview_target_window_id.as_ref())
                    .and_then(|pid| core.window_label(pid).map(|label| (pid.clone(), label)));
                (drag, hit, prev_to_clear)
            };

            Self::emit_clear_preview(app, prev_to_clear);

            let Some(drag) = drag else {
                return Ok(());
            };

            if let Some(hit) = hit {
                let _ = self.move_tab_to_window(
                    app,
                    runtime,
                    &drag.source_window_id,
                    &drag.tab_id,
                    &hit.target_window_id,
                    Some(hit.insert_index),
                )?;
            } else {
                let (sx, sy) = phys.map(|(x, y)| (Some(x), Some(y))).unwrap_or((None, None));
                let _ = self.tear_off_tab_with_scale(
                    app,
                    runtime,
                    &drag.source_window_id,
                    &drag.tab_id,
                    sx,
                    sy,
                    Some(source_scale),
                )?;
            }
        }
        #[cfg(not(desktop))]
        let _ = (app, runtime, window_id, client_x, client_y);

        Ok(())
    }

    pub fn drag_cancel(&self, app: &AppHandle, window_id: &str) {
        let prev_to_clear = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            if !core
                .active_drag()
                .is_some_and(|d| d.source_window_id == window_id)
            {
                return;
            }
            core.take_active_drag()
                .and_then(|d| d.last_preview_target_window_id)
                .and_then(|pid| core.window_label(&pid).map(|label| (pid, label)))
        };
        Self::emit_clear_preview(app, prev_to_clear);
    }

    pub fn on_window_focused(&self, window_label: &str) {
        let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
        let _ = core.mark_focused_by_label(window_label);
    }

    pub fn on_window_destroyed(
        &self,
        app: &AppHandle,
        window_label: &str,
        runtime: &std::sync::Arc<ClientRuntime>,
    ) {
        let effect = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.remove_window_on_destroy_effect(window_label)
        };
        if !effect.removed {
            return;
        }
        Self::emit_clear_preview(app, effect.clear_preview);
        Self::spawn_close_sessions(runtime, effect.sessions_to_close);
        self.emit_all_snapshots(app);
    }

    pub async fn evict_books(
        &self,
        app: &AppHandle,
        runtime: &ClientRuntime,
        book_ids: &[String],
    ) {
        let effect = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.evict_books(book_ids)
        };
        Self::emit_clear_preview(app, effect.clear_preview);
        Self::close_sessions(runtime, effect.sessions_to_close).await;
        Self::close_window_labels(app, &effect.windows_to_close);
        if !effect.affected_window_ids.is_empty() || !effect.windows_to_close.is_empty() {
            self.emit_all_snapshots(app);
        }
    }

    pub async fn close_all_windows_async(&self, app: &AppHandle, runtime: &ClientRuntime) {
        let effect = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.take_all_windows()
        };
        Self::emit_clear_preview(app, effect.clear_preview);
        Self::close_window_labels(app, &effect.windows_to_close);
        Self::close_sessions(runtime, effect.sessions_to_close).await;
    }

    pub fn close_all_windows_sync(
        &self,
        app: &AppHandle,
        runtime: &std::sync::Arc<ClientRuntime>,
    ) {
        let effect = {
            let mut core = self.core.lock().unwrap_or_else(|e| e.into_inner());
            core.take_all_windows()
        };
        Self::emit_clear_preview(app, effect.clear_preview);
        Self::close_window_labels(app, &effect.windows_to_close);
        Self::spawn_close_sessions(runtime, effect.sessions_to_close);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use shiro_compatibility::{BookLocatorOutput, EpubSessionId};

    use super::{
        BookOpenSpec, CoordinatorCore, TabStateUpdate, TearOffOutcome, WindowHitRect,
    };

    fn make_spec(book_id: &str, title: &str) -> BookOpenSpec {
        BookOpenSpec {
            book_id: book_id.to_owned(),
            book_hash: format!("hash-{book_id}"),
            title: title.to_owned(),
            format: "EPUB".to_owned(),
        }
    }

    #[test]
    fn open_book_reuses_most_recent_window_and_deduplicates_same_book_unless_forced() {
        let mut core = CoordinatorCore::new(true);

        let first = core.open_book(make_spec("book-1", "Book One"), false);
        assert!(first.created_new_window);
        assert!(core.mark_window_created(&first.window_id));

        let second = core.open_book(make_spec("book-2", "Book Two"), false);
        assert!(!second.created_new_window);
        assert_eq!(second.window_id, first.window_id);

        let snap = core.snapshot(&first.window_id).unwrap();
        assert_eq!(snap.tabs.len(), 2);
        assert_eq!(snap.active_tab_id.as_deref(), Some(snap.tabs[1].tab_id.as_str()));

        // Clicking book-1 again deduplicates and activates existing tab instead of adding a 3rd tab
        let dedup = core.open_book(make_spec("book-1", "Book One"), false);
        assert!(!dedup.created_new_window);
        let snap_after = core.snapshot(&first.window_id).unwrap();
        assert_eq!(snap_after.tabs.len(), 2);
        assert_eq!(
            snap_after.active_tab_id.as_deref(),
            Some(snap_after.tabs[0].tab_id.as_str())
        );

        // force_new_window = true always spawns a new reader window
        let forced = core.open_book(make_spec("book-1", "Book One"), true);
        assert!(forced.created_new_window);
        assert_ne!(forced.window_id, first.window_id);
    }

    #[test]
    fn hit_test_picks_most_recently_focused_overlapping_window_and_computes_slot_index() {
        let mut core = CoordinatorCore::new(true);
        let src = core.open_book(make_spec("b-src", "Source"), true);
        assert!(core.mark_window_created(&src.window_id));
        let win_low = core.open_book(make_spec("b-low", "Low Z"), true);
        assert!(core.mark_window_created(&win_low.window_id));
        let win_high = core.open_book(make_spec("b-high", "High Z"), true);
        assert!(core.mark_window_created(&win_high.window_id));
        core.open_book(make_spec("b-high-2", "High Z Tab 2"), false);

        // win_high was opened/focused more recently than win_low
        let mut rects = HashMap::new();
        rects.insert(
            src.window_id.clone(),
            WindowHitRect {
                inner_x: 0.0,
                inner_y: 0.0,
                inner_width: 900.0,
                scale_factor: 1.0,
                is_minimized: false,
            },
        );
        rects.insert(
            win_low.window_id.clone(),
            WindowHitRect {
                inner_x: 100.0,
                inner_y: 100.0,
                inner_width: 900.0,
                scale_factor: 1.0,
                is_minimized: false,
            },
        );
        rects.insert(
            win_high.window_id.clone(),
            WindowHitRect {
                inner_x: 100.0,
                inner_y: 100.0,
                inner_width: 900.0,
                scale_factor: 2.0,
                is_minimized: false,
            },
        );

        // Inside overlapping top tab strip of both win_low & win_high -> win_high wins
        let hit = core
            .hit_test_drag(&src.window_id, 620.0, 125.0, &rects)
            .expect("expected hit");
        assert_eq!(hit.target_window_id, win_high.window_id);
        assert!(hit.insert_index <= 2);

        // Re-focusing win_low promotes it above win_high
        assert!(core.mark_focused_by_label(&win_low.window_label));
        let hit_promoted = core
            .hit_test_drag(&src.window_id, 160.0, 120.0, &rects)
            .expect("expected promoted hit");
        assert_eq!(hit_promoted.target_window_id, win_low.window_id);
    }

    #[test]
    fn move_tab_preserves_session_and_locator_and_closes_empty_source_and_single_tab_tear_off_moves() {
        let mut core = CoordinatorCore::new(true);
        let win_a = core.open_book(make_spec("book-a", "Book A"), true);
        let win_b = core.open_book(make_spec("book-b", "Book B"), true);

        let snap_a = core.snapshot(&win_a.window_id).unwrap();
        let tab_a_id = snap_a.tabs[0].tab_id.clone();

        let sid = EpubSessionId::new();
        let locator = BookLocatorOutput::Epub {
            spine_index: Some(2),
            href: "ch2.xhtml".to_owned(),
            fragment: None,
            cfi: Some("epubcfi(/6/6[ch2]!/4/12)".to_owned()),
        };

        assert!(core.update_tab_state(
            &win_a.window_id,
            &tab_a_id,
            TabStateUpdate {
                session_id: Some(sid),
                current_locator: Some(locator.clone()),
                fraction: Some(0.42),
                translation_mode: Some("bilingual".to_owned()),
                target_language: Some("zh".to_owned()),
            }
        ));

        // Single-tab tear-off reuses existing window without recreating
        let tear_single = core.tear_off_tab(&win_a.window_id, &tab_a_id);
        assert_eq!(
            tear_single,
            Some(TearOffOutcome::MovedSingleTabWindow {
                window_id: win_a.window_id.clone(),
                window_label: win_a.window_label.clone(),
            })
        );

        // Moving last tab from win_a to win_b closes win_a and preserves session + locator
        let effect = core
            .move_tab_to_window(&win_a.window_id, &tab_a_id, &win_b.window_id, Some(0))
            .expect("move succeeds");
        assert!(effect.sessions_to_close.is_empty());
        assert_eq!(effect.windows_to_close, vec![win_a.window_label.clone()]);
        assert!(core.snapshot(&win_a.window_id).is_none());

        let snap_b = core.snapshot(&win_b.window_id).unwrap();
        assert_eq!(snap_b.tabs.len(), 2);
        assert_eq!(snap_b.tabs[0].tab_id, tab_a_id);
        assert_eq!(snap_b.tabs[0].session_id, Some(sid));
        assert_eq!(snap_b.tabs[0].current_locator, Some(locator));
        assert_eq!(snap_b.tabs[0].translation_mode, "bilingual");
        assert_eq!(snap_b.tabs[0].target_language.as_deref(), Some("zh"));
    }

    #[test]
    fn evict_books_and_destroy_window_reclaim_sessions_without_leaks() {
        let mut core = CoordinatorCore::new(true);
        let w1 = core.open_book(make_spec("book-del", "Deleted Book"), true);
        assert!(core.mark_window_created(&w1.window_id));
        let w2 = core.open_book(make_spec("book-del", "Deleted Book"), true);
        assert!(core.mark_window_created(&w2.window_id));
        core.open_book(make_spec("book-keep", "Kept Book"), false);

        let sid1 = EpubSessionId::new();
        let sid2 = EpubSessionId::new();
        let sid_keep = EpubSessionId::new();

        let s1 = core.snapshot(&w1.window_id).unwrap();
        let s2 = core.snapshot(&w2.window_id).unwrap();
        core.update_tab_state(
            &w1.window_id,
            &s1.tabs[0].tab_id,
            TabStateUpdate {
                session_id: Some(sid1),
                ..Default::default()
            },
        );
        core.update_tab_state(
            &w2.window_id,
            &s2.tabs[0].tab_id,
            TabStateUpdate {
                session_id: Some(sid2),
                ..Default::default()
            },
        );
        core.update_tab_state(
            &w2.window_id,
            &s2.tabs[1].tab_id,
            TabStateUpdate {
                session_id: Some(sid_keep),
                ..Default::default()
            },
        );

        let evict = core.evict_books(&["book-del".to_owned()]);
        assert_eq!(evict.windows_to_close, vec![w1.window_label.clone()]);
        assert_eq!(evict.sessions_to_close.len(), 2);
        assert!(evict.sessions_to_close.contains(&sid1));
        assert!(evict.sessions_to_close.contains(&sid2));

        // Destroying w2 via OS close reclaims remaining kept session
        let reclaimed = core.remove_window_on_destroy(&w2.window_label);
        assert_eq!(reclaimed, vec![sid_keep]);
    }

    #[test]
    fn take_all_windows_clears_state_and_reclaims_all_windows_and_sessions() {
        let mut core = CoordinatorCore::new(true);
        let w1 = core.open_book(make_spec("b1", "One"), true);
        assert!(core.mark_window_created(&w1.window_id));
        let w2 = core.open_book(make_spec("b2", "Two"), true);
        assert!(core.mark_window_created(&w2.window_id));
        let b3 = core.open_book(make_spec("b3", "Three"), false);
        assert_eq!(b3.window_id, w2.window_id);

        let s1 = core.snapshot(&w1.window_id).unwrap();
        let s2 = core.snapshot(&w2.window_id).unwrap();
        let sid1 = EpubSessionId::new();
        let sid2 = EpubSessionId::new();
        core.update_tab_state(
            &w1.window_id,
            &s1.tabs[0].tab_id,
            TabStateUpdate {
                session_id: Some(sid1),
                ..Default::default()
            },
        );
        core.update_tab_state(
            &w2.window_id,
            &s2.tabs[1].tab_id,
            TabStateUpdate {
                session_id: Some(sid2),
                ..Default::default()
            },
        );
        core.drag_start(&w1.window_id, &s1.tabs[0].tab_id);

        let effect = core.take_all_windows();
        assert_eq!(effect.windows_to_close.len(), 2);
        assert!(effect.windows_to_close.contains(&w1.window_label));
        assert!(effect.windows_to_close.contains(&w2.window_label));
        assert_eq!(effect.sessions_to_close.len(), 2);
        assert!(effect.sessions_to_close.contains(&sid1));
        assert!(effect.sessions_to_close.contains(&sid2));
        assert!(core.all_window_ids().is_empty());
        assert!(core.active_drag().is_none());
    }

    #[test]
    fn close_tab_handles_non_last_and_last_tab_and_clears_matching_drag() {
        let mut core = CoordinatorCore::new(true);
        let w1 = core.open_book(make_spec("b1", "One"), true);
        assert!(core.mark_window_created(&w1.window_id));
        core.open_book(make_spec("b2", "Two"), false);
        let w2 = core.open_book(make_spec("b3", "Three"), true);
        assert!(core.mark_window_created(&w2.window_id));

        let snap = core.snapshot(&w1.window_id).unwrap();
        let t1 = snap.tabs[0].tab_id.clone();
        let t2 = snap.tabs[1].tab_id.clone();
        let sid1 = EpubSessionId::new();
        let sid2 = EpubSessionId::new();

        core.update_tab_state(
            &w1.window_id,
            &t1,
            TabStateUpdate {
                session_id: Some(sid1),
                ..Default::default()
            },
        );
        core.update_tab_state(
            &w1.window_id,
            &t2,
            TabStateUpdate {
                session_id: Some(sid2),
                ..Default::default()
            },
        );

        core.drag_start(&w1.window_id, &t2);
        core.set_last_preview_target(Some(w2.window_id.clone()));

        let non_last = core.close_tab(&w1.window_id, &t2);
        assert_eq!(non_last.sessions_to_close, vec![sid2]);
        assert!(non_last.windows_to_close.is_empty());
        assert_eq!(non_last.affected_window_ids, vec![w1.window_id.clone()]);
        assert_eq!(
            non_last.clear_preview,
            Some((w2.window_id.clone(), w2.window_label.clone()))
        );
        let after_non_last = core.snapshot(&w1.window_id).unwrap();
        assert_eq!(after_non_last.tabs.len(), 1);
        assert_eq!(after_non_last.active_tab_id.as_deref(), Some(t1.as_str()));

        let last = core.close_tab(&w1.window_id, &t1);
        assert_eq!(last.sessions_to_close, vec![sid1]);
        assert_eq!(last.windows_to_close, vec![w1.window_label.clone()]);
        assert!(core.snapshot(&w1.window_id).is_none());
    }

    #[test]
    fn multi_tab_tear_off_creates_new_window_and_supports_rollback() {
        let mut core = CoordinatorCore::new(true);
        let src = core.open_book(make_spec("b1", "One"), true);
        assert!(core.mark_window_created(&src.window_id));
        core.open_book(make_spec("b2", "Two"), false);
        let snap = core.snapshot(&src.window_id).unwrap();
        let t1 = snap.tabs[0].tab_id.clone();
        let t2 = snap.tabs[1].tab_id.clone();

        let (outcome, rollback, _) = core
            .tear_off_tab_with_rollback(&src.window_id, &t1)
            .expect("tear off");
        let new_window_id = match outcome {
            TearOffOutcome::CreatedNewWindow {
                source_window_id,
                new_window_id,
                tab_title,
                ..
            } => {
                assert_eq!(source_window_id, src.window_id);
                assert_eq!(tab_title, "One");
                new_window_id
            }
            TearOffOutcome::MovedSingleTabWindow { .. } => panic!("expected new window"),
        };

        let src_after = core.snapshot(&src.window_id).unwrap();
        assert_eq!(src_after.tabs.len(), 1);
        assert_eq!(src_after.tabs[0].tab_id, t2);
        let dst_after = core.snapshot(&new_window_id).unwrap();
        assert_eq!(dst_after.tabs.len(), 1);
        assert_eq!(dst_after.tabs[0].tab_id, t1);

        // Rolling back failed window creation restores tab order and active tab cleanly
        let orphan = core.rollback_tear_off(rollback.unwrap());
        assert!(orphan.is_none());
        assert!(core.snapshot(&new_window_id).is_none());
        let restored = core.snapshot(&src.window_id).unwrap();
        assert_eq!(restored.tabs.len(), 2);
        assert_eq!(restored.tabs[0].tab_id, t1);
        assert_eq!(restored.active_tab_id.as_deref(), Some(t2.as_str()));
    }

    #[test]
    fn hit_test_respects_minimized_source_exclusion_bounds_and_insert_index() {
        let mut core = CoordinatorCore::new(true);
        let src = core.open_book(make_spec("src", "Source"), true);
        assert!(core.mark_window_created(&src.window_id));
        let min_win = core.open_book(make_spec("min", "Minimized"), true);
        assert!(core.mark_window_created(&min_win.window_id));
        let target = core.open_book(make_spec("t1", "Target 1"), true);
        assert!(core.mark_window_created(&target.window_id));
        core.open_book(make_spec("t2", "Target 2"), false);
        assert!(core.report_tab_strip_metrics(&target.window_id, 40.0, 400.0));

        let mut rects = HashMap::new();
        rects.insert(
            src.window_id.clone(),
            WindowHitRect {
                inner_x: 100.0,
                inner_y: 100.0,
                inner_width: 900.0,
                scale_factor: 1.0,
                is_minimized: false,
            },
        );
        rects.insert(
            min_win.window_id.clone(),
            WindowHitRect {
                inner_x: 100.0,
                inner_y: 100.0,
                inner_width: 900.0,
                scale_factor: 1.0,
                is_minimized: true,
            },
        );
        rects.insert(
            target.window_id.clone(),
            WindowHitRect {
                inner_x: 100.0,
                inner_y: 100.0,
                inner_width: 900.0,
                scale_factor: 1.0,
                is_minimized: false,
            },
        );

        // Y below strip height misses
        assert!(core.hit_test_drag(&src.window_id, 150.0, 145.0, &rects).is_none());
        // X beyond drag_width (window controls zone) misses
        assert!(core.hit_test_drag(&src.window_id, 520.0, 120.0, &rects).is_none());
        // Left edge hits insert_index 0 (source & minimized excluded)
        let left_hit = core
            .hit_test_drag(&src.window_id, 110.0, 120.0, &rects)
            .unwrap();
        assert_eq!(left_hit.target_window_id, target.window_id);
        assert_eq!(left_hit.insert_index, 0);
        // Right edge of drag strip hits end index 2
        let right_hit = core
            .hit_test_drag(&src.window_id, 490.0, 120.0, &rects)
            .unwrap();
        assert_eq!(right_hit.insert_index, 2);
    }

    #[test]
    fn reorder_tabs_preserves_unlisted_tab_order_and_snapshot_serde_preserves_locator_snake_case() {
        let mut core = CoordinatorCore::new(true);
        let win = core.open_book(make_spec("b1", "One"), true);
        assert!(core.mark_window_created(&win.window_id));
        core.open_book(make_spec("b2", "Two"), false);
        core.open_book(make_spec("b3", "Three"), false);
        let initial = core.snapshot(&win.window_id).unwrap();
        let t1 = initial.tabs[0].tab_id.clone();
        let t2 = initial.tabs[1].tab_id.clone();
        let t3 = initial.tabs[2].tab_id.clone();

        assert!(core.reorder_tabs(&win.window_id, std::slice::from_ref(&t3)));
        let reordered = core.snapshot(&win.window_id).unwrap();
        let ids: Vec<&str> = reordered.tabs.iter().map(|t| t.tab_id.as_str()).collect();
        assert_eq!(ids, vec![t3.as_str(), t1.as_str(), t2.as_str()]);

        core.update_tab_state(
            &win.window_id,
            &t3,
            TabStateUpdate {
                current_locator: Some(BookLocatorOutput::Epub {
                    spine_index: Some(1),
                    href: "ch1.xhtml".to_owned(),
                    fragment: Some("p1".to_owned()),
                    cfi: Some("epubcfi(/6/4!/4/2)".to_owned()),
                }),
                ..Default::default()
            },
        );
        let snap = core.snapshot(&win.window_id).unwrap();
        let json: serde_json::Value = serde_json::to_value(&snap).unwrap();
        assert!(json.get("windowId").is_some());
        assert!(json.get("activeTabId").is_some());
        let locator = &json["tabs"][0]["currentLocator"];
        assert_eq!(locator["kind"], "epub");
        assert_eq!(locator["spine_index"], 1);
        assert!(locator.get("spineIndex").is_none());
    }

    #[test]
    fn late_update_tab_state_after_cross_window_move_preserves_migrated_session() {
        let mut core = CoordinatorCore::new(true);
        let w1 = core.open_book(make_spec("b1", "One"), true);
        let w2 = core.open_book(make_spec("b2", "Two"), true);
        let t1 = core.snapshot(&w1.window_id).unwrap().tabs[0].tab_id.clone();
        let sid1 = EpubSessionId::new();

        let initial = core.update_tab_state_with_orphan(
            &w1.window_id,
            &t1,
            TabStateUpdate {
                session_id: Some(sid1),
                ..Default::default()
            },
        );
        assert!(initial.updated);
        assert!(initial.orphaned_session.is_none());

        // Move t1 (holding sid1) from w1 into w2; w1 closes as empty.
        let moved = core
            .move_tab_to_window(&w1.window_id, &t1, &w2.window_id, None)
            .expect("move tab");
        assert_eq!(moved.windows_to_close, vec![w1.window_label.clone()]);

        // Late in-flight update from closed w1 carrying sid1 must NOT orphan sid1.
        let late = core.update_tab_state_with_orphan(
            &w1.window_id,
            &t1,
            TabStateUpdate {
                session_id: Some(sid1),
                ..Default::default()
            },
        );
        assert!(!late.updated);
        assert!(late.orphaned_session.is_none());

        // Verify w2 still owns sid1.
        let w2_snap = core.snapshot(&w2.window_id).unwrap();
        let moved_tab = w2_snap.tabs.iter().find(|t| t.tab_id == t1).unwrap();
        assert_eq!(moved_tab.session_id.as_ref(), Some(&sid1));
    }

    #[test]
    fn late_update_tab_state_attaches_session_to_migrated_tab_without_session() {
        let mut core = CoordinatorCore::new(true);
        let w1 = core.open_book(make_spec("b1", "One"), true);
        assert!(core.mark_window_created(&w1.window_id));
        core.open_book(make_spec("b2", "Two"), false);
        let w2 = core.open_book(make_spec("b3", "Three"), true);
        assert!(core.mark_window_created(&w2.window_id));
        let t1 = core.snapshot(&w1.window_id).unwrap().tabs[0].tab_id.clone();
        let sid1 = EpubSessionId::new();

        // Move t1 from w1 to w2 before its initial session_id arrived.
        core.move_tab_to_window(&w1.window_id, &t1, &w2.window_id, None)
            .expect("move tab");

        // Late update from still-open w1 attaches sid1 to migrated t1 in w2.
        let late = core.update_tab_state_with_orphan(
            &w1.window_id,
            &t1,
            TabStateUpdate {
                session_id: Some(sid1),
                ..Default::default()
            },
        );
        assert!(late.updated);
        assert!(late.orphaned_session.is_none());
        let w2_snap = core.snapshot(&w2.window_id).unwrap();
        let migrated = w2_snap.tabs.iter().find(|t| t.tab_id == t1).unwrap();
        assert_eq!(migrated.session_id.as_ref(), Some(&sid1));
    }

    #[test]
    fn update_tab_state_orphans_session_on_closed_tab_or_replacement() {
        let mut core = CoordinatorCore::new(true);
        let w1 = core.open_book(make_spec("b1", "One"), true);
        let t1 = core.snapshot(&w1.window_id).unwrap().tabs[0].tab_id.clone();
        let sid1 = EpubSessionId::new();
        let sid2 = EpubSessionId::new();

        core.update_tab_state_with_orphan(
            &w1.window_id,
            &t1,
            TabStateUpdate {
                session_id: Some(sid1),
                ..Default::default()
            },
        );
        // Replacing sid1 with sid2 orphans sid1.
        let replaced = core.update_tab_state_with_orphan(
            &w1.window_id,
            &t1,
            TabStateUpdate {
                session_id: Some(sid2),
                ..Default::default()
            },
        );
        assert!(replaced.updated);
        assert_eq!(replaced.orphaned_session, Some(sid1));

        // Closing the tab and receiving a late session update orphans the new session.
        let _ = core.close_tab(&w1.window_id, &t1);
        let sid3 = EpubSessionId::new();
        let late_closed = core.update_tab_state_with_orphan(
            &w1.window_id,
            &t1,
            TabStateUpdate {
                session_id: Some(sid3),
                ..Default::default()
            },
        );
        assert!(!late_closed.updated);
        assert_eq!(late_closed.orphaned_session, Some(sid3));
    }

    #[test]
    fn move_and_tear_off_clear_matching_active_drag_and_preview() {
        let mut core = CoordinatorCore::new(true);
        let w1 = core.open_book(make_spec("b1", "One"), true);
        assert!(core.mark_window_created(&w1.window_id));
        core.open_book(make_spec("b2", "Two"), false);
        let w2 = core.open_book(make_spec("b3", "Three"), true);
        assert!(core.mark_window_created(&w2.window_id));
        let t1 = core.snapshot(&w1.window_id).unwrap().tabs[0].tab_id.clone();

        assert!(core.drag_start(&w1.window_id, &t1));
        core.set_last_preview_target(Some(w2.window_id.clone()));

        let moved = core
            .move_tab_to_window(&w1.window_id, &t1, &w2.window_id, None)
            .expect("move tab");
        assert!(core.active_drag().is_none());
        assert_eq!(
            moved.clear_preview,
            Some((w2.window_id.clone(), w2.window_label.clone()))
        );

        // Start a drag in w2 on t1 and tear it off directly via menu -> clears active drag & preview.
        assert!(core.drag_start(&w2.window_id, &t1));
        core.set_last_preview_target(Some(w1.window_id.clone()));
        let (_, _, clear_preview) = core
            .tear_off_tab_with_rollback(&w2.window_id, &t1)
            .expect("tear off");
        assert!(core.active_drag().is_none());
        assert_eq!(
            clear_preview,
            Some((w1.window_id.clone(), w1.window_label.clone()))
        );
    }

    #[test]
    fn pending_creation_window_is_not_reused_or_hit_tested_until_marked_created() {
        let mut core = CoordinatorCore::new(true);
        let w1 = core.open_book(make_spec("b1", "Pending One"), false);
        assert!(w1.created_new_window);

        let mut rects = HashMap::new();
        rects.insert(
            w1.window_id.clone(),
            WindowHitRect {
                inner_x: 0.0,
                inner_y: 0.0,
                inner_width: 900.0,
                scale_factor: 1.0,
                is_minimized: false,
            },
        );
        // Must not be hit-tested while pending_creation is true.
        assert!(core.hit_test_drag("other", 50.0, 20.0, &rects).is_none());

        // Concurrent open_book(new_window=false) must NOT pick pending w1.
        let w2 = core.open_book(make_spec("b2", "Second"), false);
        assert!(w2.created_new_window);
        assert_ne!(w2.window_id, w1.window_id);
        assert_eq!(core.snapshot(&w1.window_id).unwrap().tabs.len(), 1);

        // Once w1 finishes creation, subsequent open_book reuses w1 and hit_test sees it.
        assert!(core.mark_window_created(&w1.window_id));
        assert!(core.mark_focused_by_label(&w1.window_label));
        let w3 = core.open_book(make_spec("b3", "Third"), false);
        assert!(!w3.created_new_window);
        assert_eq!(w3.window_id, w1.window_id);
        assert_eq!(core.snapshot(&w1.window_id).unwrap().tabs.len(), 2);
        assert_eq!(
            core.hit_test_drag("other", 50.0, 20.0, &rects)
                .map(|h| h.target_window_id),
            Some(w1.window_id)
        );
    }

    #[test]
    fn take_all_windows_advances_reset_generation() {
        let mut core = CoordinatorCore::new(true);
        let gen0 = core.reset_generation();
        let w1 = core.open_book(make_spec("b1", "One"), true);
        assert!(core.mark_window_created(&w1.window_id));
        assert_eq!(core.reset_generation(), gen0);

        let _ = core.take_all_windows();
        let gen1 = core.reset_generation();
        assert_ne!(gen1, gen0);
    }
}
