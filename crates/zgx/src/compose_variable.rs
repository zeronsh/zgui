//! Reactive variable-height virtualization, sharing retained component ownership.
use super::*;
use crate::virtual_list::{HeightError, HeightIndex};
use std::cell::Cell;

/// Cloneable indexed heights with logarithmic point updates and no whole-index clones.
///
/// Heights belong to positions; callers must reorder them with their data. A point
/// update preserves the viewport's first visible index and intra-row offset.
/// Count changes preserve/clamp the pixel offset. `variable_virtual_list` uses
/// supplied allocations; `measured_virtual_list` updates this cache from mounted
/// natural content and permanently applies a one-pixel allocation floor.
#[derive(Clone)]
pub struct VariableHeights {
    index: Rc<RefCell<HeightIndex>>,
    revision: Signal<u64>,
    end: Rc<Cell<bool>>,
}
impl VariableHeights {
    /// `count` rows, each `estimate` tall until measured. Panics if `estimate`
    /// isn't finite and positive.
    pub fn new(runtime: &Runtime, count: usize, estimate: f32) -> Self {
        Self::try_new(runtime, count, estimate).expect("valid variable row heights")
    }
    /// `count` rows, each `estimate` tall until measured, or an error if
    /// `estimate` isn't finite and positive.
    pub fn try_new(runtime: &Runtime, count: usize, estimate: f32) -> Result<Self, HeightError> {
        Ok(Self {
            index: Rc::new(RefCell::new(HeightIndex::try_new(count, estimate)?)),
            revision: runtime.signal(0),
            end: Default::default(),
        })
    }
    /// Keep a viewport resting at the end there through height and count
    /// changes, instead of anchoring its first visible row: a chat
    /// transcript that follows new messages and stays put while offscreen
    /// estimates above it are measured.
    pub fn anchor_end(self) -> Self {
        self.end.set(true);
        self
    }
    fn changed(&self) {
        self.revision
            .update(|revision| *revision = revision.wrapping_add(1));
    }
    /// Update one finite positive height. Once mounted in measured mode, values
    /// below one logical pixel clamp to one, including offscreen estimates.
    pub fn set_height(&self, index: usize, height: f32) -> Result<bool, HeightError> {
        let changed = self.index.borrow_mut().set_height(index, height)?;
        if changed {
            self.changed();
        }
        Ok(changed)
    }
    /// Change the number of rows: existing heights stay, added rows take the
    /// estimate. Returns whether the count changed.
    pub fn resize(&self, count: usize) -> Result<bool, HeightError> {
        let changed = self.index.borrow_mut().resize(count)?;
        if changed {
            self.changed();
        }
        Ok(changed)
    }
    /// Replace every height at once, in linear time: for data replaced or
    /// reordered wholesale.
    pub fn replace(&self, heights: impl IntoIterator<Item = f32>) -> Result<(), HeightError> {
        self.index.borrow_mut().replace(heights)?;
        self.changed();
        Ok(())
    }
    /// The number of rows, read reactively.
    pub fn len(&self) -> usize {
        self.revision.get();
        self.index.borrow().count()
    }
    /// Whether there are no rows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// Row `index`'s height, if it exists.
    pub fn row_height(&self, index: usize) -> Option<f32> {
        self.revision.get();
        self.index.borrow().row_height(index)
    }
    /// How far down the list row `index` starts.
    pub fn row_offset(&self, index: usize) -> f32 {
        self.revision.get();
        self.index.borrow().row_offset(index)
    }
    /// The height of all the rows together.
    pub fn content_height(&self) -> f32 {
        self.revision.get();
        self.index.borrow().content_height()
    }
    /// The row at `offset` down the list (see [`HeightIndex::row_at`]).
    pub fn row_at(&self, offset: f32) -> Option<usize> {
        self.revision.get();
        self.index.borrow().row_at(offset)
    }
}
struct Anchor {
    revision: u64,
    length: usize,
    position: f32,
    index: usize,
    /// How far the view's top is below the anchor row's (negative when
    /// the row starts lower, in view).
    within: f32,
    at_end: bool,
}

/// Mount only visible variable-height keyed rows and an overscan margin.
///
/// Rows take their allocated height from `heights`; child constructors run once
/// per mounted key. Point height updates keep the first visible index anchored,
/// unless the caller also changes `offset`. Keys are unique among mounted rows.
/// Reordering data must also reorder its indexed heights; it does not relocate
/// the viewport by key. Persistent offscreen row state belongs in the model.
pub fn variable_virtual_list<K: Eq + std::hash::Hash + Clone + 'static>(
    offset: Signal<f32>,
    heights: VariableHeights,
    overscan: usize,
    key: impl FnMut(usize) -> K + 'static,
    build: impl FnMut(Signal<usize>, K, &mut Context) -> View + 'static,
) -> View {
    variable_list(offset, heights, overscan, key, build, false)
}

/// Virtualize naturally sized rows, updating a positional height estimate cache.
///
/// Mounted rows measure at the allocated viewport width, including their child
/// margins and padding, with a minimum allocation of one logical pixel. Cached
/// offscreen heights remain estimates until their rows are mounted again; the
/// total extent is therefore approximate. Mounting applies the one-pixel floor
/// to this shared cache, including future height writes and added estimates.
/// Reorder cached heights with the data, as for `variable_virtual_list`.
/// Use a separate cache for independently sized lists. The same anchor and
/// ownership rules apply.
pub fn measured_virtual_list<K: Eq + std::hash::Hash + Clone + 'static>(
    offset: Signal<f32>,
    heights: VariableHeights,
    overscan: usize,
    key: impl FnMut(usize) -> K + 'static,
    build: impl FnMut(Signal<usize>, K, &mut Context) -> View + 'static,
) -> View {
    variable_list(offset, heights, overscan, key, build, true)
}
/// Virtualize keyed rows of natural height, keeping each row's height by key.
///
/// `rows` lists the keys in order (reactively); a key's row is built with
/// `build` when it nears the viewport and measured once mounted. Rows not yet
/// mounted take `estimate(rows, index)`, which apps compute from what they
/// know (text measured with [`Context::text_measure`], image sizes): it runs
/// off the critical path, a few milliseconds at a time between frames, and
/// rows use `heights`' estimate until then. Heights survive reordering,
/// insertion and removal by key. Configure end anchoring and the default
/// height on `heights`; mounted rows follow the `measured_virtual_list` rules.
pub fn measured_rows<K: Eq + std::hash::Hash + Clone + 'static>(
    offset: Signal<f32>,
    heights: VariableHeights,
    overscan: usize,
    mut rows: impl FnMut() -> Vec<K> + 'static,
    estimate: impl FnMut(&[K], usize) -> f32 + 'static,
    mut build: impl FnMut(Signal<usize>, K, &mut Context) -> View + 'static,
) -> View {
    component(move |cx| {
        // The keys `heights` describes, in order, and where each is: kept
        // with them, as a slice of estimates can't afford to index them all.
        let described: Rc<RefCell<Vec<K>>> = Rc::default();
        let positions: Rc<RefCell<HashMap<K, usize>>> = Rc::default();
        // Keys still at the default height, awaiting their estimate.
        let pending: Rc<RefCell<Vec<K>>> = Rc::default();
        let estimate = Rc::new(RefCell::new(estimate));
        // Estimate pending rows, newest first, while `slice` allows.
        let refine = {
            let (heights, described, positions, pending, estimate) = (
                heights.clone(),
                described.clone(),
                positions.clone(),
                pending.clone(),
                estimate.clone(),
            );
            let runtime = cx.runtime();
            move |slice: Option<std::time::Duration>| {
                let started = std::time::Instant::now();
                let default = heights.index.borrow().estimate();
                let mut exact = Vec::new();
                {
                    let described = described.borrow();
                    let index = positions.borrow();
                    let mut pending = pending.borrow_mut();
                    let mut estimate = estimate.borrow_mut();
                    while slice.is_none_or(|slice| started.elapsed() < slice)
                        && let Some(key) = pending.pop()
                    {
                        // Rows mounted since were measured.
                        if let Some(&i) = index.get(&key)
                            && heights.index.borrow().row_height(i) == Some(default)
                        {
                            let height = estimate(&described, i);
                            if height.is_finite() && height > 0. {
                                exact.push((i, height));
                            }
                        }
                    }
                }
                runtime.batch(|| {
                    for (i, height) in exact {
                        let _ = heights.set_height(i, height);
                    }
                });
                !pending.borrow().is_empty()
            }
        };
        let runner = cx.try_service::<TaskRunner>().is_some().then(|| cx.tasks());
        let refining = Rc::new(Cell::new(false));
        let refine = Rc::new(refine);
        let schedule = {
            let (refine, refining) = (refine.clone(), refining.clone());
            move || match &runner {
                Some(tasks) if !refining.replace(true) => {
                    let (refine, refining) = (refine.clone(), refining.clone());
                    tasks.spawn(async move {
                        loop {
                            crate::timer::sleep(std::time::Duration::from_millis(1)).await;
                            if !refine(Some(std::time::Duration::from_millis(4))) {
                                break;
                            }
                        }
                        refining.set(false);
                    });
                }
                Some(_) => {}
                // Without a task runner (headless), estimate at once.
                None => {
                    refine(None);
                }
            }
        };
        {
            let (heights, described, positions, pending) = (
                heights.clone(),
                described.clone(),
                positions.clone(),
                pending.clone(),
            );
            let runtime = cx.runtime();
            let effect = cx.runtime().effect(move || {
                let current = rows();
                runtime.untracked(|| {
                    let default = heights.index.borrow().estimate();
                    let old = described.replace(current.clone());
                    let known: HashMap<&K, f32> = {
                        let index = heights.index.borrow();
                        old.iter()
                            .enumerate()
                            .filter_map(|(i, key)| Some((key, index.row_height(i)?)))
                            .collect()
                    };
                    let mut added = Vec::new();
                    let rows: Vec<f32> = current
                        .iter()
                        .map(|key| {
                            known.get(key).copied().unwrap_or_else(|| {
                                added.push(key.clone());
                                default
                            })
                        })
                        .collect();
                    heights.replace(rows).expect("finite heights");
                    {
                        let mut positions = positions.borrow_mut();
                        positions.clear();
                        positions.extend(current.iter().cloned().zip(0..));
                    }
                    if !added.is_empty() {
                        pending.borrow_mut().extend(added);
                        schedule();
                    }
                });
            });
            cx.retain(effect);
        }
        let key = move |index: usize| described.borrow()[index].clone();
        measured_virtual_list(offset, heights, overscan, key, move |index, key, cx| {
            build(index, key, cx)
        })
    })
}
struct Entry {
    node: NodeId,
    index: Signal<usize>,
    measurement: Option<Signal<(f32, f32)>>,
}
fn row_style(width: f32, height: f32, measured: bool) -> crate::scene::Style {
    if measured {
        crate::scene::Style {
            width: Some(width),
            min_height: Some(1.),
            ..Default::default()
        }
    } else {
        crate::widgets::fixed(width, height)
    }
}
fn variable_list<K: Eq + std::hash::Hash + Clone + 'static>(
    offset: Signal<f32>,
    heights: VariableHeights,
    overscan: usize,
    key: impl FnMut(usize) -> K + 'static,
    build: impl FnMut(Signal<usize>, K, &mut Context) -> View + 'static,
    measured: bool,
) -> View {
    let rows = RowSource::new(key, build);
    let text = rows.text({
        let heights = heights.clone();
        move || heights.index.try_borrow().map_or(0, |index| index.count())
    });
    let (mut key, mut build, mounted_in) = (rows.key(), rows.build(), rows.mounted_in.clone());
    View::new(Kind::VirtualList(Box::new(
        move |ui, root, environment, scrollbar, keyboard_navigation| {
            *mounted_in.borrow_mut() = Some((ui.runtime.clone(), environment.clone()));
            if measured {
                let changed = heights.index.borrow_mut().enforce_minimum(1.);
                if changed {
                    heights.changed();
                }
            }
            let size = ui.observe_content_size(root);
            let extent = ui.signal(0.0_f32);
            // Clip rows to the allocated content box, keeping root padding clear.
            let viewport = ui.container(root, Layout::Overlay, Default::default());
            let content = ui.container(viewport, Layout::Overlay, Default::default());
            ui.scene.borrow_mut().set_scroll_copy(content, true);
            let navigation = keyboard_navigation.then(|| {
                crate::compose_virtual_keyboard::Navigation::new_variable(
                    ui,
                    root,
                    offset.clone(),
                    size.clone(),
                    heights.clone(),
                )
            });
            let wheel_offset = offset.clone();
            let wheel_size = size.clone();
            let wheel_extent = extent.clone();
            ui.on_event(root, false, move |cx| {
                if cx.phase == EventPhase::Capture || cx.default_prevented() {
                    return;
                }
                if let InputEvent::Scroll { delta_y, .. } = cx.event {
                    if !delta_y.is_finite() {
                        return;
                    }
                    let before = wheel_offset.get();
                    let after = (before + delta_y)
                        .max(0.0)
                        .min((wheel_extent.get() - wheel_size.get().1).max(0.0));
                    if before != after {
                        wheel_offset.set(after);
                        cx.scrolled(false);
                    }
                }
            });
            if scrollbar {
                crate::compose_scrollbar::mount(
                    ui,
                    root,
                    offset.clone(),
                    size.clone(),
                    extent.clone(),
                    false,
                );
            }
            let weak = ui.downgrade();
            let runtime = ui.runtime.clone();
            let mut entries: HashMap<K, Entry> = HashMap::new();
            let mounted_revision = ui.signal(0_u64);
            let mut discovery: Option<(usize, usize)> = None;
            let mut handled_request = 0;
            // Scroll-to-row requests (`NodeHandle::scroll_to_row`).
            let reveal = crate::compose::list_reveal(ui, root);
            let mut handled_reveal = 0;
            // A destination followed through measurement: the row, where it
            // shows, the position it was put at, the focus then, and whether
            // focus goes with it (the keyboard's, not a scroll's).
            let mut pending_reveal: Option<(usize, RowAlign, f32, Option<NodeId>, bool)> = None;
            let mut anchor: Option<Anchor> = None;
            ui.bind(root, move || {
                heights.revision.get();
                mounted_revision.get();
                let (width, height) = size.get();
                let requested = offset.get();
                let mut pending_measurement = false;
                let mut measurement_changed = false;
                if measured {
                    // Read every mounted observation in this effect so one
                    // batched layout publication produces one measurement pass.
                    let mut updates = Vec::new();
                    for (entry_key, entry) in &entries {
                        let index = entry.index.get();
                        let (observed_width, observed_height) =
                            entry.measurement.as_ref().unwrap().get();
                        if observed_width != width || width <= 0. || !observed_height.is_finite() {
                            pending_measurement = true;
                            continue;
                        }
                        let observed_height = observed_height.max(1.);
                        let previous = heights.index.borrow().row_height(index);
                        if previous.is_some_and(|old| old != observed_height)
                            // Do not assign an old keyed row's observation to a
                            // newly reordered item at its former position.
                            && key(index) == *entry_key
                        {
                            updates.push((index, observed_height));
                        }
                    }
                    {
                        let mut model = heights.index.borrow_mut();
                        for (index, height) in updates {
                            measurement_changed |= model.set_height(index, height).unwrap_or(false);
                        }
                    }
                    if measurement_changed {
                        heights.changed();
                    }
                }
                let revision = heights.revision.get();
                let pending_target = pending_reveal
                    .filter(|(_, _, expected, focus, _)| {
                        expected.to_bits() == requested.to_bits()
                            && weak
                                .upgrade()
                                .is_some_and(|ui| ui.input.focused() == *focus)
                    })
                    .map(|(index, align, _, _, keys)| (index, align, keys));
                let request = navigation
                    .as_ref()
                    .map(|navigation| navigation.request.get());
                let (serial, row, align) = reveal.get();
                let asked = (serial != handled_reveal).then_some((row, align, false));
                handled_reveal = serial;
                let (length, total, position, target, geometry, next_anchor) = {
                    let model = heights.index.borrow();
                    let length = model.count();
                    let total = model.content_height();
                    let limit = (total - height).max(0.);
                    let mut position = if requested.is_finite() {
                        requested.max(0.)
                    } else {
                        0.
                    }
                    .min(limit);
                    if let Some(previous) = &anchor
                        && previous.revision != revision
                        && requested == previous.position
                        && previous.at_end
                        && heights.end.get()
                    {
                        position = limit;
                    } else if let Some(previous) = &anchor
                        && previous.revision != revision
                        && previous.length == length
                        && requested == previous.position
                        && let Some(row_height) = model.row_height(previous.index)
                    {
                        let top = model.row_offset(previous.index);
                        let bottom = (top as f64 + row_height as f64).min(f32::MAX as f64) as f32;
                        // Clamp the final coordinate, too: adding a sub-height
                        // intra-row offset can round up to the following row.
                        let last = bottom.next_down().max(top);
                        position =
                            ((top as f64 + previous.within as f64).clamp(0., f32::MAX as f64)
                                as f32)
                                .min(last)
                                .min(limit);
                    }
                    // A row asked for, else the keyboard's, else one followed.
                    let target = asked
                        .or(request
                            .filter(|(serial, _)| *serial != handled_request)
                            .map(|(_, index)| (index, RowAlign::Nearest, true)))
                        .or(pending_target)
                        .and_then(|(index, align, keys)| {
                            (length > 0).then_some((
                                index.min(length.saturating_sub(1)),
                                align,
                                keys,
                            ))
                        });
                    if let Some((index, align, _)) = target {
                        let top = model.row_offset(index);
                        let row_height = model.row_height(index).unwrap();
                        position = align
                            .position(top, row_height, height, position)
                            .max(0.)
                            .min(limit);
                    }
                    let mut range = model.visible_range(position, height, overscan);
                    if measured && !range.is_empty() {
                        let first = model.row_at(position).unwrap();
                        if let Some((previous_first, previous_end)) = discovery
                            && previous_first == first
                        {
                            if range.end > previous_end {
                                // Large overestimates otherwise discover only
                                // one tiny row per layout feedback pass.
                                range.end = range
                                    .end
                                    .max(first.saturating_add(
                                        previous_end.saturating_sub(first).max(1).saturating_mul(2),
                                    ))
                                    .min(length);
                            } else if pending_measurement {
                                // Keep speculative rows until layout has
                                // measured them; pruning early defeats doubling.
                                range.end = range.end.max(previous_end).min(length);
                            }
                        }
                        discovery = Some((first, range.end));
                    } else {
                        discovery = None;
                    }
                    let geometry: Vec<_> = model.rows(range).collect();
                    let next_anchor = model.row_at(position).map(|index| Anchor {
                        revision,
                        length,
                        position,
                        index,
                        within: (position - model.row_offset(index)).max(0.),
                        at_end: limit - position <= 0.5,
                    });
                    (length, total, position, target, geometry, next_anchor)
                };
                // Release the index borrow before application key/build callbacks.
                let keys: Vec<_> = geometry
                    .into_iter()
                    .map(|(index, top, row_height)| (key(index), index, top, row_height))
                    .collect();
                let unique: std::collections::HashSet<_> =
                    keys.iter().map(|(key, ..)| key).collect();
                assert_eq!(unique.len(), keys.len(), "duplicate virtual list child key");
                // Measured, anchor on the first row in view measured at this
                // width: rows coming into view above it, estimates until
                // measured, grow above it rather than push it down.
                let next_anchor = next_anchor.map(|mut anchor| {
                    let measured_row = |key: &K| {
                        entries
                            .get(key)
                            .and_then(|entry| entry.measurement.as_ref())
                            .is_some_and(|m| {
                                m.with_untracked(|(w, h)| {
                                    *w == width && width > 0. && h.is_finite()
                                })
                            })
                    };
                    if measured
                        && let Some((_, index, top, _)) =
                            keys.iter().find(|(key, _, top, row_height)| {
                                top + row_height > anchor.position && measured_row(key)
                            })
                    {
                        anchor.index = *index;
                        anchor.within = anchor.position - top;
                    }
                    anchor
                });
                let Some(mut ui) = weak.upgrade() else {
                    return;
                };
                if !ui.scene.borrow().contains(root) {
                    return;
                }
                runtime.untracked(|| {
                    let mut transaction = MountTransaction::new(&ui, content);
                    // Construct first: a failed row constructor leaves old rows intact.
                    let mut added = Vec::new();
                    for (key, index, _, row_height) in &keys {
                        if entries.contains_key(key) {
                            continue;
                        }
                        let index_signal = ui.signal(*index);
                        let mut cx = Context::new(runtime.clone(), environment.clone());
                        let view = build(index_signal.clone(), key.clone(), &mut cx);
                        let row = ui.container(
                            content,
                            Layout::Overlay,
                            row_style(width, *row_height, measured),
                        );
                        crate::selection::mount_row(&mut ui, row, &index_signal);
                        let node = mount(&mut ui, row, view, environment.clone());
                        cx.finish(&mut ui, node);
                        if let Some(navigation) = &navigation {
                            navigation.mount_row(&mut ui, row, *row_height);
                        }
                        let measurement = measured.then(|| ui.observe_content_size_deferred(row));
                        added.push((
                            key.clone(),
                            Entry {
                                node: row,
                                index: index_signal,
                                measurement,
                            },
                        ));
                    }
                    let added_measurements = measured && !added.is_empty();
                    entries.extend(added);
                    let mut removed_focus = false;
                    entries.retain(|key, entry| {
                        let node = &entry.node;
                        if unique.contains(key) {
                            true
                        } else {
                            if navigation.is_some()
                                && let Some(focused) = ui.input.focused()
                            {
                                let scene = ui.scene.borrow();
                                removed_focus |= focused == *node
                                    || (scene.contains(focused)
                                        && scene
                                            .ancestors(focused)
                                            .any(|ancestor| ancestor == *node));
                            }
                            ui.remove(*node);
                            false
                        }
                    });
                    for (key, index, ..) in &keys {
                        entries[key].index.set(*index);
                    }
                    {
                        let mut scene = ui.scene.borrow_mut();
                        scene.set_style(
                            viewport,
                            crate::scene::Style {
                                clip: true,
                                ..crate::widgets::fixed(width, height)
                            },
                        );
                        scene.set_style(content, crate::widgets::fixed(width, total));
                        scene.set_transform(
                            content,
                            crate::scene::Transform {
                                x: 0.0,
                                y: -position,
                            },
                        );
                        for (key, _, top, row_height) in &keys {
                            let node = entries[key].node;
                            scene.set_style(node, row_style(width, *row_height, measured));
                            scene.set_transform(node, crate::scene::Transform { x: 0.0, y: *top });
                        }
                        let order: Vec<_> =
                            keys.iter().map(|(key, ..)| entries[key].node).collect();
                        assert!(scene.reorder_children(content, &order));
                    }
                    transaction.commit();
                    if added_measurements {
                        // Subscribe to observations registered during this
                        // untracked mount transaction on the next effect pass.
                        mounted_revision.update(|value| *value = value.wrapping_add(1));
                    }
                    extent.set(total);
                    // Application key/build callbacks may request a new scroll
                    // position. Let the queued pass handle it instead of
                    // replacing that request with this pass's old geometry.
                    let request_unchanged = offset.get().to_bits() == requested.to_bits();
                    if request_unchanged {
                        anchor = next_anchor;
                        offset.set(position);
                    } else {
                        anchor = None;
                    }
                    let keep_reveal = measured
                        && request_unchanged
                        && (added_measurements
                            || pending_measurement
                            || measurement_changed
                            || asked.is_some()
                            || request.is_some_and(|(serial, _)| serial != handled_request));
                    let mut requested_focus = None;
                    if let Some(navigation) = &navigation {
                        navigation.update(
                            &ui,
                            root,
                            length,
                            keys.iter()
                                .map(|(key, index, ..)| (*index, entries[key].node))
                                .collect(),
                        );
                        if let Some((serial, _)) = request {
                            handled_request = serial;
                        }
                        if let Some((index, _, true)) = target.filter(|_| request_unchanged) {
                            if let Some((key, ..)) =
                                keys.iter().find(|(_, candidate, ..)| *candidate == index)
                            {
                                requested_focus = Some(entries[key].node);
                                ui.input.focus(&ui.scene, requested_focus);
                            }
                        } else if removed_focus && ui.input.focused().is_none() {
                            ui.input.focus(&ui.scene, Some(root));
                        }
                    }
                    // Follow a destination through measurement, but never
                    // override a newer pixel request or focus owner.
                    pending_reveal = target
                        .filter(|_| {
                            keep_reveal
                                && requested_focus
                                    .is_none_or(|node| ui.input.focused() == Some(node))
                        })
                        .map(|(index, align, keys)| {
                            (index, align, position, ui.input.focused(), keys)
                        });
                });
            });
        },
    )))
    .row_text(text)
}
