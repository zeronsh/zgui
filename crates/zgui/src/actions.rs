//! Typed actions and scoped keyboard bindings.
//!
//! Bindings resolve from the focused component outwards before ordinary key
//! handling. Context predicates support flags, attributes and ancestor relations.
//! Ambiguous chords have one demand-driven one-second deadline. Unmatched or
//! expired prefixes replay their original native key/text events; handled actions
//! consume them. Focus, ownership and input-method changes cancel pending input.
pub use crate::action_context::{ContextParseError, ContextPredicate, KeyContext};
use crate::{
    input::{InputEvent, Key, Modifiers},
    scene::NodeId,
};
use std::{
    any::Any,
    cell::RefCell,
    fmt,
    rc::{Rc, Weak},
    time::{Duration, Instant},
};

/// A typed command value: key bindings dispatch it and `View::on_action`
/// handlers for its type receive it. Clones are equal; separately made actions
/// are not.
#[derive(Clone)]
pub struct Action {
    value: Rc<dyn Any>,
    name: &'static str,
}
impl Action {
    /// An action carrying `value`.
    pub fn new<T: 'static>(value: T) -> Self {
        Self {
            value: Rc::new(value),
            name: std::any::type_name::<T>(),
        }
    }
    /// The value, if it is a `T`.
    pub fn downcast_ref<T: 'static>(&self) -> Option<&T> {
        self.value.downcast_ref()
    }
}
impl fmt::Debug for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Action").field(&self.name).finish()
    }
}
impl PartialEq for Action {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.value, &other.value)
    }
}

/// A key with the modifiers held. Bindings match it exactly: the same key and
/// exactly these modifiers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keystroke {
    /// The key.
    pub key: Key,
    /// The modifiers held with it.
    pub modifiers: Modifiers,
}
impl Keystroke {
    /// `key` with `modifiers`.
    pub fn new(key: Key, modifiers: Modifiers) -> Self {
        Self { key, modifiers }
    }
}

/// A sequence of one to four keystrokes bound to an action, optionally only
/// where a context predicate holds.
#[derive(Clone, Debug)]
pub struct KeyBinding {
    sequence: Vec<Keystroke>,
    action: Action,
    context: Option<ContextPredicate>,
}
impl KeyBinding {
    /// Bind one through four strokes. Invalid lengths are rejected without registration.
    pub fn new(
        sequence: impl IntoIterator<Item = Keystroke>,
        action: impl Into<Action>,
    ) -> Result<Self, &'static str> {
        let sequence: Vec<_> = sequence.into_iter().take(5).collect();
        if sequence.is_empty() || sequence.len() > 4 {
            return Err("key sequences require one through four strokes");
        }
        Ok(Self {
            sequence,
            action: action.into(),
            context: None,
        })
    }
    /// Apply only where a context in the stack has `context` (see
    /// [`ContextPredicate::Flag`]).
    pub fn when(mut self, context: impl Into<String>) -> Self {
        self.context = Some(ContextPredicate::Flag(context.into()));
        self
    }
    /// Bind using a parsed context expression, including attributes and ancestors.
    pub fn when_predicate(mut self, predicate: ContextPredicate) -> Self {
        self.context = Some(predicate);
        self
    }
}
/// The key bindings and key context a component adds (`View::keymap`).
#[derive(Clone, Debug, Default)]
pub struct Keymap {
    pub(crate) bindings: Vec<KeyBinding>,
    pub(crate) contexts: Vec<String>,
    pub(crate) context: KeyContext,
}
impl Keymap {
    /// An empty keymap.
    pub fn new() -> Self {
        Self::default()
    }
    /// Add `binding`. Bindings added later are tried first.
    pub fn bind(mut self, binding: KeyBinding) -> Self {
        self.bindings.push(binding);
        self
    }
    /// Set the flag `context` on the component's key context.
    pub fn context(mut self, context: impl Into<String>) -> Self {
        self.contexts.push(context.into());
        self
    }
    /// Add `context`'s flags and attributes to the component's key context.
    pub fn key_context(mut self, context: KeyContext) -> Self {
        self.context.extend(&context);
        self
    }
    pub(crate) fn extend(&mut self, other: Self) {
        self.bindings.extend(other.bindings);
        self.contexts.extend(other.contexts);
        self.context.extend(&other.context);
    }
}
#[derive(Default)]
pub(crate) struct Registry {
    next: u64,
    maps: crate::scene::NodeMap<Vec<(u64, Keymap)>>,
    pending: Option<Pending>,
}
struct Pending {
    path: Vec<NodeId>,
    strokes: Vec<Keystroke>,
    events: Vec<InputEvent>,
    candidates: Vec<Action>,
    expires: Instant,
}
/// Explicitly disable a key binding without dispatching its ordinary input.
#[derive(Clone, Copy, Debug)]
pub struct NoAction;
pub(crate) struct Replay {
    pub path: Vec<NodeId>,
    pub events: Vec<InputEvent>,
    pub candidates: Vec<Action>,
}
impl From<Pending> for Replay {
    fn from(pending: Pending) -> Self {
        Self {
            path: pending.path,
            events: pending.events,
            candidates: pending.candidates,
        }
    }
}
/// Keeps a keymap registered (`InputDispatcher::bind_keys`); dropping it
/// unregisters the keymap.
#[must_use = "Dropping the binding unregisters the keymap"]
pub struct KeymapBinding {
    registry: Weak<RefCell<Registry>>,
    node: NodeId,
    token: u64,
}
impl Drop for KeymapBinding {
    fn drop(&mut self) {
        if let Some(registry) = self.registry.upgrade() {
            let removed = {
                let mut registry = registry.borrow_mut();
                registry.pending = None;
                let removed = registry.maps.get_mut(&self.node).and_then(|maps| {
                    maps.iter()
                        .position(|(token, _)| *token == self.token)
                        .map(|index| maps.remove(index))
                });
                if registry.maps.get(&self.node).is_some_and(Vec::is_empty) {
                    registry.maps.remove(&self.node);
                }
                removed
            };
            drop(removed);
        }
    }
}
pub(crate) enum Resolution {
    None,
    Prefix,
    Matched(Replay),
    Retry(Replay),
}
impl Registry {
    pub(crate) fn bind(registry: &Rc<RefCell<Self>>, node: NodeId, map: Keymap) -> KeymapBinding {
        let token = {
            let mut state = registry.borrow_mut();
            state.next = state
                .next
                .checked_add(1)
                .expect("keymap registration exhausted");
            let token = state.next;
            state.maps.entry(node).or_default().push((token, map));
            state.pending = None;
            token
        };
        KeymapBinding {
            registry: Rc::downgrade(registry),
            node,
            token,
        }
    }
    pub(crate) fn cancel(&mut self) {
        self.pending = None;
    }
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.pending.as_ref().map(|p| p.expires)
    }
    pub(crate) fn expired(&mut self, now: Instant) -> Option<Replay> {
        if self.deadline().is_some_and(|deadline| deadline <= now) {
            self.pending.take().map(Into::into)
        } else {
            None
        }
    }
    pub(crate) fn retain_event(&mut self, event: &InputEvent) -> bool {
        let Some(pending) = &mut self.pending else {
            return false;
        };
        match event {
            InputEvent::Text(_) => {
                // One native text payload per accepted stroke; repeats must not
                // grow pending memory or duplicate text on eventual replay.
                let texts = pending
                    .events
                    .iter()
                    .filter(|e| matches!(e, InputEvent::Text(_)))
                    .count();
                if texts < pending.strokes.len() {
                    pending.events.push(event.clone());
                }
                true
            }
            InputEvent::KeyUp { key, .. } if pending.strokes.iter().any(|s| &s.key == key) => {
                let ups = pending
                    .events
                    .iter()
                    .filter(|e| matches!(e,InputEvent::KeyUp{key:old,..} if old==key))
                    .count();
                let downs = pending.strokes.iter().filter(|s| &s.key == key).count();
                if ups < downs {
                    pending.events.push(event.clone());
                }
                true
            }
            _ => false,
        }
    }
    pub(crate) fn resolve(
        &mut self,
        path: Vec<NodeId>,
        stroke: Keystroke,
        repeat: bool,
        now: Instant,
    ) -> Resolution {
        if repeat && self.pending.is_some() {
            return Resolution::Prefix;
        }
        let previous = self.pending.take().filter(|pending| pending.path == path);
        let mut strokes = previous
            .as_ref()
            .map_or_else(Vec::new, |p| p.strokes.clone());
        strokes.push(stroke.clone());
        let contexts: Vec<KeyContext> = path
            .iter()
            .rev()
            .filter_map(|node| self.maps.get(node))
            .map(|maps| {
                let mut context = KeyContext::new();
                for (_, map) in maps {
                    context.extend(&map.context);
                    for flag in &map.contexts {
                        context = context.flag(flag);
                    }
                }
                context
            })
            .collect();
        let mut candidates = Vec::new();
        let mut prefix = false;
        for node in &path {
            if let Some(maps) = self.maps.get(node) {
                for (_, map) in maps.iter().rev() {
                    for binding in map.bindings.iter().rev() {
                        if binding
                            .context
                            .as_ref()
                            .is_some_and(|context| !context.matches(&contexts))
                            || !binding.sequence.starts_with(&strokes)
                        {
                            continue;
                        }
                        if binding.sequence.len() == strokes.len() {
                            candidates.push(binding.action.clone());
                            if binding.action.downcast_ref::<NoAction>().is_some() {
                                break;
                            }
                        } else if candidates.is_empty() {
                            prefix = true;
                        }
                    }
                }
            }
            // An inner prefix has precedence over ancestor commands; exact
            // candidates still retain ancestor fallbacks when no prefix wins.
            if prefix {
                break;
            }
        }
        if !prefix && candidates.is_empty() {
            return previous.map_or(Resolution::None, |previous| {
                Resolution::Retry(previous.into())
            });
        }
        let mut events = previous.map_or_else(Vec::new, |p| p.events);
        events.push(InputEvent::KeyDown {
            key: stroke.key,
            modifiers: stroke.modifiers,
            repeat,
        });
        let pending = Pending {
            path,
            strokes,
            events,
            candidates,
            expires: now + Duration::from_secs(1),
        };
        if prefix {
            self.pending = Some(pending);
            Resolution::Prefix
        } else {
            Resolution::Matched(pending.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stroke(number: u8) -> Keystroke {
        Keystroke::new(Key::Function(number), Modifiers::default())
    }
    #[test]
    fn pending_chords_expose_one_deadline_and_newer_exact_binding_wins() {
        let node = crate::scene::Scene::new(100., 100.).root();
        let registry = Rc::new(RefCell::new(Registry::default()));
        let _guard = Registry::bind(
            &registry,
            node,
            Keymap::new()
                .bind(KeyBinding::new([stroke(1), stroke(2)], Action::new(12u8)).unwrap())
                .bind(KeyBinding::new([stroke(3), stroke(4)], Action::new(34u8)).unwrap())
                .bind(KeyBinding::new([stroke(3)], Action::new(3u8)).unwrap()),
        );
        let now = Instant::now();
        assert!(registry.borrow().deadline().is_none());
        assert!(matches!(
            registry
                .borrow_mut()
                .resolve(vec![node], stroke(1), false, now),
            Resolution::Prefix
        ));
        assert_eq!(
            registry.borrow().deadline(),
            Some(now + Duration::from_secs(1))
        );
        assert!(registry.borrow_mut().expired(now).is_none());
        assert_eq!(
            registry
                .borrow_mut()
                .expired(now + Duration::from_secs(1))
                .unwrap()
                .events
                .len(),
            1
        );
        assert!(registry.borrow().deadline().is_none());
        let Resolution::Matched(replay) =
            registry
                .borrow_mut()
                .resolve(vec![node], stroke(3), false, now)
        else {
            panic!("exact binding did not win")
        };
        assert_eq!(replay.candidates[0].downcast_ref::<u8>(), Some(&3));
    }
    #[test]
    fn keymap_guard_releases_captured_actions_outside_registry_borrow() {
        struct Reenter(Weak<RefCell<Registry>>);
        impl Drop for Reenter {
            fn drop(&mut self) {
                if let Some(state) = self.0.upgrade() {
                    state.borrow_mut().cancel();
                }
            }
        }
        let scene = crate::scene::Scene::new(100., 100.);
        let registry = Rc::new(RefCell::new(Registry::default()));
        let guard = Registry::bind(
            &registry,
            scene.root(),
            Keymap::new().bind(
                KeyBinding::new([stroke(1)], Action::new(Reenter(Rc::downgrade(&registry))))
                    .unwrap(),
            ),
        );
        drop(guard);
        assert!(registry.borrow().maps.is_empty());
        assert!(KeyBinding::new([], Action::new(())).is_err());
        assert!(
            KeyBinding::new(
                [stroke(1), stroke(2), stroke(3), stroke(4), stroke(5)],
                Action::new(())
            )
            .is_err()
        );
    }
}
