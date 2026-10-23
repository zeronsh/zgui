//! anime.js `stagger(value, params)`: a value that differs per target by
//! its distance from a starting index, in a line or a grid.
use super::ease::Ease;
use std::{cell::RefCell, rc::Rc};

pub(crate) enum From {
    Index(f64),
    Center,
    Last,
    Random,
}

pub(crate) struct Stagger {
    /// One value per step, or a range spread over all targets.
    pub value: f64,
    pub range_end: Option<f64>,
    pub unit: Option<Rc<str>>,
    /// Replaces the range's first value as the base, as in anime.js.
    pub start: Option<f64>,
    pub from: From,
    pub reversed: bool,
    pub ease: Option<Ease>,
    pub grid: Option<(usize, usize)>,
    pub axis: Option<char>,
    pub total: Option<usize>,
    /// Script-provided index per target (`use`).
    pub index: Option<mlua::Function>,
    pub modifier: Option<mlua::Function>,
    /// Distances per index, computed once for a target count.
    cache: RefCell<Option<(usize, Vec<f64>, f64)>>,
}
impl Stagger {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        value: f64,
        range_end: Option<f64>,
        unit: Option<Rc<str>>,
        start: Option<f64>,
        from: From,
        reversed: bool,
        ease: Option<Ease>,
        grid: Option<(usize, usize)>,
        axis: Option<char>,
        total: Option<usize>,
        index: Option<mlua::Function>,
        modifier: Option<mlua::Function>,
    ) -> Self {
        Self {
            value,
            range_end,
            unit,
            start,
            from,
            reversed,
            ease,
            grid,
            axis,
            total,
            index,
            modifier,
            cache: RefCell::default(),
        }
    }
    /// Each target's distance and the largest, kept for `total`: every
    /// target of an animation reads them, so they are lent, not copied.
    fn distances(&self, total: usize) -> std::cell::Ref<'_, (usize, Vec<f64>, f64)> {
        if self
            .cache
            .borrow()
            .as_ref()
            .is_none_or(|(n, ..)| *n != total)
        {
            let (values, max) = self.compute(total);
            *self.cache.borrow_mut() = Some((total, values, max));
        }
        std::cell::Ref::map(self.cache.borrow(), |cache| {
            cache.as_ref().expect("filled above")
        })
    }
    fn compute(&self, total: usize) -> (Vec<f64>, f64) {
        let center = matches!(self.from, From::Center);
        let from_index = match self.from {
            From::Index(i) => i,
            From::Center => (total as f64 - 1.) / 2.,
            From::Last => total as f64 - 1.,
            From::Random => 0.,
        };
        let mut values: Vec<f64> = (0..total)
            .map(|index| match self.grid {
                None => (from_index - index as f64).abs(),
                Some((cols, rows)) => {
                    let cols = cols.max(1) as f64;
                    let (from_x, from_y) = if center {
                        ((cols - 1.) / 2., (rows as f64 - 1.) / 2.)
                    } else {
                        (from_index % cols, (from_index / cols).floor())
                    };
                    let (to_x, to_y) = (index as f64 % cols, (index as f64 / cols).floor());
                    let (dx, dy) = (from_x - to_x, from_y - to_y);
                    match self.axis {
                        Some('x') => -dx,
                        Some('y') => -dy,
                        _ => (dx * dx + dy * dy).sqrt(),
                    }
                }
            })
            .collect();
        let max = values.iter().copied().fold(0., f64::max);
        if let Some(ease) = &self.ease
            && max > 0.
        {
            for v in &mut values {
                *v = ease.at(*v / max, None) * max;
            }
        }
        if self.reversed {
            for v in &mut values {
                *v = if self.axis.is_some() {
                    -*v
                } else {
                    (max - *v).abs()
                };
            }
        }
        if matches!(self.from, From::Random) {
            // Deterministic per count: a reload staggers the same way.
            let mut seed = 0x9e37_79b9_7f4a_7c15_u64 ^ total as u64;
            for i in (1..values.len()).rev() {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                values.swap(i, (seed % (i as u64 + 1)) as usize);
            }
        }
        (values, max)
    }
    /// The value for target `index` of `total`, before unit conversion.
    pub fn resolve(&self, index: usize, total: usize, custom_index: Option<f64>) -> f64 {
        let total = self.total.unwrap_or(total).max(1);
        let distances = self.distances(total);
        let (_, values, max) = &*distances;
        let max = *max;
        let index = custom_index
            .map_or(index, |i| i.max(0.) as usize)
            .min(total - 1);
        let spacing = match self.range_end {
            Some(end) if max > 0. => (end - self.value) / max,
            Some(_) => 0.,
            None => self.value,
        };
        let start = self
            .start
            .filter(|s| *s != 0.)
            .unwrap_or(if self.range_end.is_some() {
                self.value
            } else {
                0.
            });
        start + spacing * (values[index] * 100.).round() / 100.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn stagger(
        value: f64,
        range_end: Option<f64>,
        from: From,
        grid: Option<(usize, usize)>,
        axis: Option<char>,
    ) -> Stagger {
        Stagger::new(
            value, range_end, None, None, from, false, None, grid, axis, None, None, None,
        )
    }
    #[test]
    fn matches_anime_js_stagger() {
        let s = stagger(100., None, From::Index(0.), None, None);
        assert_eq!(
            (0..4).map(|i| s.resolve(i, 4, None)).collect::<Vec<_>>(),
            [0., 100., 200., 300.]
        );
        let s = stagger(100., None, From::Center, None, None);
        assert_eq!(
            (0..5).map(|i| s.resolve(i, 5, None)).collect::<Vec<_>>(),
            [200., 100., 0., 100., 200.]
        );
        let s = stagger(100., None, From::Last, None, None);
        assert_eq!(s.resolve(0, 3, None), 200.);
        let s = stagger(-10., Some(10.), From::Index(0.), None, None);
        assert_eq!(
            (0..5).map(|i| s.resolve(i, 5, None)).collect::<Vec<_>>(),
            [-10., -5., 0., 5., 10.]
        );
        let s = stagger(10., None, From::Center, Some((3, 3)), None);
        assert_eq!(s.resolve(4, 9, None), 0.);
        assert_eq!(s.resolve(0, 9, None), 14.1);
        let s = stagger(10., None, From::Index(0.), Some((3, 3)), Some('x'));
        assert_eq!(s.resolve(2, 9, None), 20.);
    }
}
