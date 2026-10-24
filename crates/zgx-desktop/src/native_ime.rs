//! Route composition only to the editor owning the current native IME session.
use winit::event::Ime;
use zgx::{
    input::InputEvent,
    scene::NodeId,
    widgets::{LayoutFeedbackError, Ui},
};

/// Winit clears its marked-text buffer on disable, but macOS also retains
/// dead-key/candidate state in NSTextInputContext. Reset that native state before
/// enabling composition for another retained editor sharing the same NSView.
pub(crate) fn discard_native_composition(window: &winit::window::Window) {
    #[cfg(target_os = "macos")]
    {
        use objc2::{MainThreadMarker, rc::autoreleasepool};
        use objc2_app_kit::NSView;
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let Some(_main_thread) = MainThreadMarker::new() else {
            return;
        };
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        autoreleasepool(|_| {
            // SAFETY: The borrowed winit Window/handle keep the NSView alive;
            // winit hands out an AppKit handle only on the main thread.
            let view = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
            if let Some(context) = view.inputContext() {
                context.discardMarkedText();
            }
        });
    }
    #[cfg(not(target_os = "macos"))]
    let _ = window;
}

/// Native composition permission follows editability, not logical focus.
pub(crate) fn editable_editor(ui: &Ui, native_focused: bool) -> Option<zgx::widgets::EditorHandle> {
    native_focused
        .then(|| ui.focused_editor())
        .flatten()
        .filter(|editor| !editor.is_read_only())
}

/// A reset must cross both native context notifications before accepting text.
/// Winit's X11 and Wayland backends emit these when IME is toggled; macOS
/// emits Disabled on cancellation and Enabled at the next marked-text session.
/// This filters events queued before the reset boundary, not a protocol epoch:
/// winit's Wayland notifications are local and its Done handling does not expose
/// a serial. A delayed server commit arriving after Enabled cannot be identified
/// here without backend support. Do not require preedit before the next commit:
/// input methods may legitimately commit without sending marked text first.
#[derive(Default)]
pub(crate) enum RestartBarrier {
    #[default]
    Open,
    AwaitDisabled,
    AwaitEnabled,
}
impl RestartBarrier {
    pub(crate) fn target_changed(
        &mut self,
        previous: Option<NodeId>,
        next: Option<NodeId>,
        composing: bool,
    ) {
        if composing && previous.is_some() && previous != next {
            *self = Self::AwaitDisabled;
        }
        // macOS emits no Disabled when no marked-text session existed. Do not
        // introduce a barrier in that case. Re-enabling preserves a pending reset.
    }
    pub(crate) fn accepts(&mut self, event: &Ime) -> bool {
        match self {
            Self::Open => true,
            Self::AwaitDisabled => {
                if matches!(event, Ime::Disabled) {
                    *self = Self::AwaitEnabled;
                }
                false
            }
            Self::AwaitEnabled => {
                if matches!(event, Ime::Enabled) {
                    *self = Self::Open;
                }
                false
            }
        }
    }
}

/// Tracks explicit editor cancellation independently of native preedit lifecycle.
/// Empty native preedit and successful commit do not change this revision.
#[derive(Default)]
pub(crate) struct CancellationTracker {
    observed: Option<(NodeId, u64)>,
}
impl CancellationTracker {
    pub(crate) fn sync(
        &mut self,
        ui: &Ui,
        native_focused: bool,
        session_target: Option<NodeId>,
        barrier: &mut RestartBarrier,
        active: &mut bool,
    ) -> bool {
        let current = editable_editor(ui, true).map(|editor| {
            (
                editor.node,
                editor.editor.borrow().composition_cancel_revision(),
            )
        });
        if current.is_none() {
            *active = false;
        }
        let previous = std::mem::replace(&mut self.observed, current);
        let cancelled = native_focused
            && current.is_some_and(|(node, revision)| {
                session_target == Some(node)
                    && previous
                        .is_some_and(|(old, old_revision)| old == node && old_revision != revision)
            });
        if cancelled {
            *active = false;
            *barrier = RestartBarrier::AwaitDisabled;
        }
        cancelled
    }
}

pub(crate) fn composition_target(ui: &Ui) -> Option<NodeId> {
    editable_editor(ui, true).and_then(|editor| {
        editor
            .editor
            .borrow()
            .preedit()
            .is_some()
            .then_some(editor.node)
    })
}

/// Returns a new composing state only when an event belonged to this session.
/// Native focus loss keeps logical focus for restoration, so checking the
/// focused editor alone would admit late input from a deactivated native window.
pub(crate) fn dispatch(
    ui: &mut Ui,
    native_focused: bool,
    session_target: Option<NodeId>,
    event: Ime,
) -> Result<Option<bool>, LayoutFeedbackError> {
    if !native_focused
        || session_target.is_none()
        || session_target != editable_editor(ui, true).map(|editor| editor.node)
    {
        return Ok(None);
    }
    let event = match event {
        Ime::Enabled => return Ok(None),
        Ime::Disabled => InputEvent::ImePreedit {
            text: String::new(),
            cursor: None,
        },
        Ime::Preedit(text, cursor) => InputEvent::ImePreedit {
            cursor: preedit_cursor(&text, cursor),
            text,
        },
        Ime::Commit(text) => InputEvent::ImeCommit(text),
    };
    let result = ui.try_dispatch(event)?;
    // Custom handlers may reject or replace preedit. Keyboard suppression must
    // follow the editor's actual state, not the text of the incoming event.
    Ok(Some(
        !result.focus_changed && composition_target(ui) == session_target,
    ))
}

/// Where the caret sits in a composition. Windows' Korean IME reports the
/// syllable it composes with the cursor at its start (`GCS_CURSORPOS` 0),
/// and zgx drew the caret before it; Windows' own fields, and browsers,
/// show it after the syllable.
fn preedit_cursor(text: &str, cursor: Option<(usize, usize)>) -> Option<(usize, usize)> {
    #[cfg(windows)]
    if cursor == Some((0, 0)) && !text.is_empty() {
        use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
        // SAFETY: reads this (the UI) thread's keyboard layout.
        let language = unsafe { GetKeyboardLayout(0) }.0 as usize & 0x3ff;
        // LANG_KOREAN: the primary language of the layout's language id.
        if language == 0x12 {
            return Some((text.len(), text.len()));
        }
    }
    let _ = text;
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus_memory::FocusMemory;

    #[test]
    fn rapid_read_only_toggle_keeps_native_reset_boundary() {
        let ui = Ui::new(100., 100.);
        let node = ui.root();
        let mut ordinary = RestartBarrier::default();
        ordinary.target_changed(Some(node), None, false);
        ordinary.target_changed(None, Some(node), false);
        assert!(ordinary.accepts(&Ime::Enabled));
        assert!(ordinary.accepts(&Ime::Preedit("fresh".into(), None)));
        for disabled_before_enable in [false, true] {
            let mut barrier = RestartBarrier::default();
            barrier.target_changed(Some(node), None, true);
            if disabled_before_enable {
                assert!(!barrier.accepts(&Ime::Disabled));
            }
            barrier.target_changed(None, Some(node), false);
            assert!(!barrier.accepts(&Ime::Commit("old".into())));
            if !disabled_before_enable {
                assert!(!barrier.accepts(&Ime::Disabled));
            }
            assert!(!barrier.accepts(&Ime::Commit("old".into())));
            assert!(!barrier.accepts(&Ime::Enabled));
            assert!(barrier.accepts(&Ime::Commit("new".into())));
        }
    }

    #[test]
    fn read_only_editor_rejects_ime_without_losing_logical_focus() {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(String::from("Read me"));
        let editor = ui.text_input(ui.root(), "Document", value.clone(), 300., false);
        ui.input.focus(&ui.scene, Some(editor.node));
        let mut tracker = CancellationTracker::default();
        let mut barrier = RestartBarrier::default();
        let mut active = false;
        tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active);
        active = dispatch(
            &mut ui,
            true,
            Some(editor.node),
            Ime::Preedit("中".into(), None),
        )
        .unwrap()
        .unwrap();
        assert!(active);
        editor.set_read_only(true);
        tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active);
        assert!(!active);
        assert_eq!(ui.input.focused(), Some(editor.node));
        assert!(editable_editor(&ui, true).is_none());
        assert!(editor.editor.borrow().preedit().is_none());
        for event in [Ime::Preedit("旧".into(), None), Ime::Commit("旧".into())] {
            assert_eq!(
                dispatch(&mut ui, true, Some(editor.node), event).unwrap(),
                None
            );
        }
        assert_eq!(value.get(), "Read me");
        editor.set_read_only(false);
        assert_eq!(editable_editor(&ui, true).unwrap().node, editor.node);
        assert!(editable_editor(&ui, false).is_none());
        assert_eq!(
            dispatch(&mut ui, true, Some(editor.node), Ime::Commit("新".into())).unwrap(),
            Some(false)
        );
        assert_eq!(value.get(), "Read me新");
    }

    #[test]
    fn external_model_and_direct_selection_cancellation_restart_observed_session() {
        for model_write in [false, true] {
            let mut ui = Ui::new(300., 200.);
            let value = ui.signal("original".to_owned());
            let editor = ui.text_input(ui.root(), "Editor", value.clone(), 200., false);
            ui.input.focus(&ui.scene, Some(editor.node));
            let mut tracker = CancellationTracker::default();
            let mut barrier = RestartBarrier::default();
            let mut active = false;
            assert!(!tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active));
            active = dispatch(
                &mut ui,
                true,
                Some(editor.node),
                Ime::Preedit("旧".into(), None),
            )
            .unwrap()
            .unwrap();
            if model_write {
                value.set("external".into());
            } else {
                editor.editor.borrow_mut().set_selection(0, 0);
                editor.refresh();
            }
            assert!(tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active));
            assert!(!active);
            assert!(!barrier.accepts(&Ime::Commit("旧".into())));
            assert!(
                !tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active),
                "same revision cannot restart repeatedly"
            );
            assert!(!barrier.accepts(&Ime::Disabled));
            assert!(!barrier.accepts(&Ime::Enabled));
            let fresh = Ime::Commit("新".into());
            assert!(barrier.accepts(&fresh));
            dispatch(&mut ui, true, Some(editor.node), fresh).unwrap();
            assert!(!tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active));
            assert!(!value.get().contains('旧'));
            assert!(value.get().contains('新'));
        }
    }

    #[test]
    fn cancellation_revision_does_not_restart_native_empty_preedit_commit_or_new_owner() {
        let mut ui = Ui::new(300., 200.);
        let value = ui.signal("".to_owned());
        let editor = ui.text_input(ui.root(), "Editor", value.clone(), 200., false);
        ui.input.focus(&ui.scene, Some(editor.node));
        let mut tracker = CancellationTracker::default();
        let mut barrier = RestartBarrier::default();
        let mut active = false;
        tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active);
        for event in [
            Ime::Preedit("中".into(), None),
            Ime::Preedit("".into(), None),
            Ime::Commit("中".into()),
        ] {
            assert!(barrier.accepts(&event));
            active = dispatch(&mut ui, true, Some(editor.node), event)
                .unwrap()
                .unwrap();
            assert!(!tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active));
        }
        assert_eq!(value.get(), "中");
        let other = ui.text_input(ui.root(), "Other", ui.signal("".into()), 200., false);
        ui.input.focus(&ui.scene, Some(other.node));
        assert!(!tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active));
        dispatch(
            &mut ui,
            true,
            Some(other.node),
            Ime::Preedit("候".into(), None),
        )
        .unwrap();
        other.editor.borrow_mut().cancel_preedit();
        assert!(
            !tracker.sync(&ui, false, Some(other.node), &mut barrier, &mut active),
            "inactive native window uses focus lifecycle reset"
        );
    }

    #[test]
    fn accessible_selection_restarts_composition_and_rejects_old_native_text() {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(String::from("Ada"));
        let editor = ui.text_input(ui.root(), "Name", value.clone(), 200., false);
        ui.input.focus(&ui.scene, Some(editor.node));
        let mut active = dispatch(
            &mut ui,
            true,
            Some(editor.node),
            Ime::Preedit("旧".into(), None),
        )
        .unwrap()
        .unwrap();
        let mut barrier = RestartBarrier::default();
        let mut tracker = CancellationTracker::default();
        tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active);
        assert!(ui.set_accessible_text_selection(editor.node, 0, 3));
        assert!(tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active));
        assert!(!active, "plain keyboard input must no longer be suppressed");
        for event in [
            Ime::Enabled,
            Ime::Commit("旧".into()),
            Ime::Preedit("旧".into(), None),
        ] {
            assert!(!barrier.accepts(&event));
        }
        assert!(!barrier.accepts(&Ime::Disabled));
        assert!(!barrier.accepts(&Ime::Commit("旧".into())));
        assert!(!barrier.accepts(&Ime::Enabled));
        let fresh = Ime::Preedit("新".into(), None);
        assert!(barrier.accepts(&fresh));
        assert_eq!(
            dispatch(&mut ui, true, Some(editor.node), fresh).unwrap(),
            Some(true)
        );
        // macOS may send an empty preedit immediately before a legitimate commit.
        let empty = Ime::Preedit(String::new(), None);
        assert!(barrier.accepts(&empty));
        assert_eq!(
            dispatch(&mut ui, true, Some(editor.node), empty).unwrap(),
            Some(false)
        );
        let commit = Ime::Commit("新".into());
        assert!(barrier.accepts(&commit));
        assert_eq!(
            dispatch(&mut ui, true, Some(editor.node), commit).unwrap(),
            Some(false)
        );
        assert_eq!(value.get(), "新");
    }

    #[test]
    fn accessible_value_cancellation_uses_same_reset_policy() {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(String::from("Ada"));
        let editor = ui.text_input(ui.root(), "Name", value.clone(), 200., false);
        ui.input.focus(&ui.scene, Some(editor.node));
        dispatch(
            &mut ui,
            true,
            Some(editor.node),
            Ime::Preedit("旧".into(), None),
        )
        .unwrap();
        let mut barrier = RestartBarrier::default();
        let mut active = true;
        let mut tracker = CancellationTracker::default();
        tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active);
        assert!(ui.set_accessible_value(editor.node, "Grace"));
        assert!(tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active));
        assert!(!active);
        assert_eq!(value.get(), "Grace");
        assert!(!barrier.accepts(&Ime::Commit("旧".into())));
        // An ordinary selection outside composition must not require a restart.
        let mut barrier = RestartBarrier::default();
        ui.set_accessible_text_selection(editor.node, 0, 5);
        assert!(!tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active));
        assert!(barrier.accepts(&Ime::Commit("new".into())));
    }

    #[test]
    fn prevented_accessible_selection_does_not_restart_composition() {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(String::from("Ada"));
        let editor = ui.text_input(ui.root(), "Name", value, 200., false);
        ui.input.focus(&ui.scene, Some(editor.node));
        dispatch(
            &mut ui,
            true,
            Some(editor.node),
            Ime::Preedit("中".into(), None),
        )
        .unwrap();
        let _listener = ui.input.listen_first(editor.node, |cx| {
            if matches!(cx.event, InputEvent::SetTextSelection { .. }) {
                cx.prevent_default();
            }
        });
        let mut barrier = RestartBarrier::default();
        let mut active = true;
        let mut tracker = CancellationTracker::default();
        tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active);
        // This bool alone cannot distinguish the editor from a cancelling listener.
        assert!(ui.set_accessible_text_selection(editor.node, 0, 3));
        assert!(!tracker.sync(&ui, true, Some(editor.node), &mut barrier, &mut active));
        assert!(active);
        assert!(barrier.accepts(&Ime::Commit("中".into())));
        assert!(editor.editor.borrow().preedit().is_some());
    }

    #[test]
    fn prevented_preedit_tracks_actual_editor_state() {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(String::new());
        let editor = ui.text_input(ui.root(), "Name", value, 200., false);
        ui.input.focus(&ui.scene, Some(editor.node));
        let listener = ui.input.listen_first(editor.node, |cx| {
            if matches!(cx.event, InputEvent::ImePreedit { .. }) {
                cx.prevent_default();
            }
        });
        assert_eq!(
            dispatch(
                &mut ui,
                true,
                Some(editor.node),
                Ime::Preedit("中".into(), None)
            )
            .unwrap(),
            Some(false)
        );
        drop(listener);
        assert_eq!(
            dispatch(
                &mut ui,
                true,
                Some(editor.node),
                Ime::Preedit("中".into(), None)
            )
            .unwrap(),
            Some(true)
        );
        let _listener = ui.input.listen_first(editor.node, |cx| {
            if matches!(cx.event, InputEvent::ImePreedit { .. }) {
                cx.prevent_default();
            }
        });
        assert_eq!(
            dispatch(
                &mut ui,
                true,
                Some(editor.node),
                Ime::Preedit(String::new(), None)
            )
            .unwrap(),
            Some(true)
        );
    }

    #[test]
    fn late_composition_after_native_blur_cannot_edit_remembered_focus() {
        let mut ui = Ui::new(400., 200.);
        let value = ui.signal(String::new());
        let editor = ui.text_input(ui.root(), "First", value.clone(), 200., false);
        ui.input.focus(&ui.scene, Some(editor.node));
        assert_eq!(
            dispatch(
                &mut ui,
                true,
                Some(editor.node),
                Ime::Preedit("中".into(), Some((3, 3)))
            )
            .unwrap(),
            Some(true)
        );
        assert!(editor.editor.borrow().preedit().is_some());
        let mut focus = FocusMemory::default();
        focus.deactivate(&ui);
        assert_eq!(ui.input.focused(), Some(editor.node));
        assert!(editor.editor.borrow().preedit().is_none());
        for event in [
            Ime::Preedit("文".into(), Some((3, 3))),
            Ime::Commit("文".into()),
        ] {
            assert_eq!(
                dispatch(&mut ui, false, Some(editor.node), event).unwrap(),
                None
            );
        }
        assert_eq!(value.get(), "");
        assert!(editor.editor.borrow().preedit().is_none());
        focus.activate(&ui);
        dispatch(&mut ui, true, Some(editor.node), Ime::Commit("新".into())).unwrap();
        assert_eq!(value.get(), "新");
    }

    #[test]
    fn stale_session_cannot_deliver_to_a_different_or_removed_editor() {
        let mut ui = Ui::new(400., 200.);
        let first_value = ui.signal(String::new());
        let second_value = ui.signal(String::new());
        let first = ui.text_input(ui.root(), "First", first_value.clone(), 200., false);
        let second = ui.text_input(ui.root(), "Second", second_value.clone(), 200., false);
        ui.input.focus(&ui.scene, Some(first.node));
        dispatch(
            &mut ui,
            true,
            Some(first.node),
            Ime::Preedit("中".into(), None),
        )
        .unwrap();
        ui.input.focus(&ui.scene, Some(second.node));
        for event in [
            Ime::Preedit("旧".into(), None),
            Ime::Commit("旧".into()),
            Ime::Disabled,
        ] {
            assert_eq!(
                dispatch(&mut ui, true, Some(first.node), event).unwrap(),
                None
            );
        }
        assert_eq!(first_value.get(), "");
        assert_eq!(second_value.get(), "");
        assert!(second.editor.borrow().preedit().is_none());
        dispatch(&mut ui, true, Some(second.node), Ime::Commit("新".into())).unwrap();
        assert_eq!(second_value.get(), "新");
        ui.remove(second.node);
        assert_eq!(
            dispatch(&mut ui, true, Some(second.node), Ime::Commit("旧".into())).unwrap(),
            None
        );
        assert_eq!(second_value.get(), "新");
    }
}
