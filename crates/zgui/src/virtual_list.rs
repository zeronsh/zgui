//! Fixed-height ranges and indexed prefix geometry for variable-height lists.
use std::ops::Range;

/// Which rows of a fixed-height list a viewport shows, in constant space.
#[derive(Clone, Copy, Debug)]
pub struct VirtualList {
    /// The number of rows.
    pub count: usize,
    /// Each row's height.
    pub row_height: f32,
    /// Rows kept mounted beyond each end of the viewport.
    pub overscan: usize,
}

impl VirtualList {
    /// A list of `count` rows, each `row_height` tall. Panics if `row_height`
    /// isn't finite and positive.
    pub fn new(count: usize, row_height: f32, overscan: usize) -> Self {
        assert!(row_height.is_finite() && row_height > 0.0);
        Self {
            count,
            row_height,
            overscan,
        }
    }
    /// The height of all the rows together.
    pub fn content_height(&self) -> f32 {
        self.row_offset(self.count)
    }
    /// The rows a viewport `viewport_height` tall, scrolled `offset` down,
    /// shows, widened by `overscan` each way. The offset is clamped to the
    /// list.
    pub fn visible_range(&self, offset: f32, viewport_height: f32) -> Range<usize> {
        if self.count == 0
            || !self.row_height.is_finite()
            || self.row_height <= 0.0
            || !viewport_height.is_finite()
            || viewport_height <= 0.0
        {
            return 0..0;
        }
        let offset = if offset.is_finite() {
            offset
                .max(0.0)
                .min((self.content_height() - viewport_height).max(0.0))
        } else {
            0.0
        };
        // Widen before adding pixels: finite f32 inputs can overflow their sum
        // and otherwise turn a small viewport into a range of usize::MAX rows.
        let first =
            ((f64::from(offset) / f64::from(self.row_height)).floor() as usize).min(self.count);
        let end = ((f64::from(offset) + f64::from(viewport_height)) / f64::from(self.row_height))
            .ceil() as usize;
        first.saturating_sub(self.overscan)..end.saturating_add(self.overscan).min(self.count)
    }
    /// How far down the list row `index` starts.
    pub fn row_offset(&self, index: usize) -> f32 {
        if !self.row_height.is_finite() || self.row_height <= 0.0 {
            return 0.0;
        }
        (index as f64 * f64::from(self.row_height)).min(f64::from(f32::MAX)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn million_rows_remain_bounded() {
        let list = VirtualList::new(1_000_000, 24.0, 3);
        for offset in [0.0, 5000.0, 24_000_000.0] {
            let range = list.visible_range(offset, 480.0);
            assert!(range.len() <= 27);
            assert!(range.end <= list.count);
        }
        assert_eq!(list.visible_range(0.0, 48.0), 0..5);
    }
    #[test]
    fn handles_empty_and_invalid_viewports() {
        assert_eq!(VirtualList::new(0, 20.0, 2).visible_range(0.0, 40.0), 0..0);
        assert_eq!(
            VirtualList::new(10, 20.0, 2).visible_range(0.0, f32::NAN),
            0..0
        );
    }
    #[test]
    fn extreme_finite_pixels_do_not_expand_range_to_every_row() {
        let list = VirtualList::new(usize::MAX, f32::MAX / 4.0, 1);
        assert_eq!(list.content_height(), f32::MAX);
        assert_eq!(list.row_offset(usize::MAX), f32::MAX);
        let range = list.visible_range(f32::MAX / 2.0, f32::MAX * 0.75);
        assert!(range.len() <= 6, "{range:?}");
    }

    #[test]
    fn count_and_overscan_arithmetic_saturate() {
        let list = VirtualList::new(usize::MAX, 1.0, usize::MAX);
        assert_eq!(list.visible_range(0.0, 1.0), 0..usize::MAX);
        let list = VirtualList::new(usize::MAX, 1.0, 0);
        let range = list.visible_range(f32::MAX, 1.0);
        assert!(range.start <= range.end);
        assert!(range.end <= list.count);
    }

    #[test]
    fn mutated_invalid_row_heights_are_empty_and_finite() {
        for height in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let mut list = VirtualList::new(100, 24.0, 2);
            list.row_height = height;
            assert_eq!(list.visible_range(0.0, 480.0), 0..0);
            assert_eq!(list.content_height(), 0.0);
            assert_eq!(list.row_offset(5), 0.0);
        }
    }
}

/// Failure to construct or update a variable-height index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeightError {
    /// A height that isn't finite and positive.
    InvalidHeight,
    /// A row index past the end of the list.
    Bounds {
        /// The index asked for.
        index: usize,
        /// The number of rows.
        count: usize,
    },
    /// More rows than can be allocated.
    Capacity,
}
impl std::fmt::Display for HeightError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHeight => f.write_str("row height must be positive and finite"),
            Self::Bounds { index, count } => write!(f, "row {index} is outside {count} rows"),
            Self::Capacity => f.write_str("height index allocation exceeds available capacity"),
        }
    }
}
impl std::error::Error for HeightError {}

// Compensated updates retain small contributions when a large measured row is
// later replaced. Adding (new-old) alone loses new when their magnitudes differ.
#[derive(Clone, Copy, Debug, Default)]
struct HeightSum {
    sum: f64,
    correction: f64,
}
impl HeightSum {
    fn add(&mut self, value: f64) {
        let sum = self.sum + value;
        self.correction += if self.sum.abs() >= value.abs() {
            (self.sum - sum) + value
        } else {
            (value - sum) + self.sum
        };
        self.sum = sum;
    }
    fn value(self) -> f64 {
        self.sum + self.correction
    }
}

/// Prefix-sum index for measured, variable-height rows.
///
/// Storage is O(count), unlike the constant-space fixed-height `VirtualList`.
/// Updates, offsets and visible-range searches are O(log count). Growth builds
/// only the added prefix-tree entries (O(added + log count), amortized allocation).
/// Shrinking truncates metadata and occasionally releases excess capacity.
/// Existing heights survive resizing; added rows use the original estimate. Coordinates saturate at `f32::MAX`; f64 prefix arithmetic
/// still has finite precision for extreme height ratios.
#[derive(Clone, Debug)]
pub struct HeightIndex {
    heights: Vec<f32>,
    tree: Vec<HeightSum>,
    estimate: f32,
    minimum: f32,
}
impl HeightIndex {
    /// `count` rows, each `estimate` tall until set. Panics if `estimate` isn't
    /// finite and positive.
    pub fn new(count: usize, estimate: f32) -> Self {
        Self::try_new(count, estimate).expect("valid height index")
    }
    /// `count` rows, each `estimate` tall until set, or an error if `estimate`
    /// isn't finite and positive.
    pub fn try_new(count: usize, estimate: f32) -> Result<Self, HeightError> {
        Self::validate(estimate)?;
        let mut index = Self {
            heights: Vec::new(),
            tree: Vec::new(),
            estimate,
            minimum: 0.,
        };
        index.resize(count)?;
        Ok(index)
    }
    fn validate(height: f32) -> Result<(), HeightError> {
        if height.is_finite() && height > 0. {
            Ok(())
        } else {
            Err(HeightError::InvalidHeight)
        }
    }
    /// The number of rows.
    pub fn count(&self) -> usize {
        self.heights.len()
    }
    /// The height rows take until they are given one.
    pub(crate) fn estimate(&self) -> f32 {
        self.estimate
    }
    /// Row `index`'s height, if it exists.
    pub fn row_height(&self, index: usize) -> Option<f32> {
        self.heights.get(index).copied()
    }
    /// Set row `index`'s height. Returns whether it changed.
    pub fn set_height(&mut self, index: usize, height: f32) -> Result<bool, HeightError> {
        Self::validate(height)?;
        let height = height.max(self.minimum);
        let count = self.count();
        let old = self
            .heights
            .get_mut(index)
            .ok_or(HeightError::Bounds { index, count })?;
        if *old == height {
            return Ok(false);
        }
        let previous = *old;
        *old = height;
        let mut node = index + 1;
        while node < self.tree.len() {
            self.tree[node].add(-f64::from(previous));
            self.tree[node].add(f64::from(height));
            node = node.saturating_add(node & node.wrapping_neg());
        }
        Ok(true)
    }
    /// Measured rows need a positive allocation floor, including unvisited
    /// estimates. Apply it once in linear time, then enforce it on point writes
    /// and growth so tiny estimates cannot materialize an entire collection.
    pub(crate) fn enforce_minimum(&mut self, minimum: f32) -> bool {
        if self.minimum >= minimum {
            return false;
        }
        self.minimum = minimum;
        self.estimate = self.estimate.max(minimum);
        let mut changed = false;
        for height in &mut self.heights {
            if *height < minimum {
                *height = minimum;
                changed = true;
            }
        }
        if changed {
            self.rebuild();
        }
        changed
    }
    /// Replace every height in linear time, for data replaced or reordered
    /// wholesale. Heights below an enforced minimum clamp to it.
    pub fn replace(&mut self, heights: impl IntoIterator<Item = f32>) -> Result<(), HeightError> {
        let minimum = self.minimum;
        let heights = heights
            .into_iter()
            .map(|height| Self::validate(height).map(|()| height.max(minimum)))
            .collect::<Result<Vec<_>, _>>()?;
        self.heights = heights;
        self.rebuild();
        Ok(())
    }
    fn rebuild(&mut self) {
        self.tree.clear();
        self.tree.resize(self.count() + 1, HeightSum::default());
        for index in 1..=self.count() {
            self.tree[index].add(f64::from(self.heights[index - 1]));
            let parent = index.saturating_add(index & index.wrapping_neg());
            if parent <= self.count() {
                let value = self.tree[index];
                self.tree[parent].add(value.sum);
                self.tree[parent].add(value.correction);
            }
        }
    }
    /// Change the number of rows: existing heights stay, added rows take the
    /// estimate. Returns whether the count changed.
    pub fn resize(&mut self, count: usize) -> Result<bool, HeightError> {
        if count == self.count() {
            return Ok(false);
        }
        let slots = count.checked_add(1).ok_or(HeightError::Capacity)?;
        // Reserve both buffers before changing logical contents.
        self.heights
            .try_reserve(count.saturating_sub(self.heights.len()))
            .map_err(|_| HeightError::Capacity)?;
        self.tree
            .try_reserve(slots.saturating_sub(self.tree.len()))
            .map_err(|_| HeightError::Capacity)?;
        let previous = self.count();
        self.heights.resize(count, self.estimate);
        if count < previous {
            self.tree.truncate(slots);
        } else {
            if self.tree.is_empty() {
                self.tree.push(HeightSum::default());
            }
            for index in previous + 1..=count {
                let mut value = HeightSum::default();
                value.add(f64::from(self.estimate));
                let start = index - (index & index.wrapping_neg());
                let mut child = index - 1;
                while child > start {
                    value.add(self.tree[child].sum);
                    value.add(self.tree[child].correction);
                    child &= child - 1;
                }
                self.tree.push(value);
            }
        }
        // Explicit shrink releases obsolete metadata, rather than retaining the
        // largest historical collection forever.
        if self.heights.capacity() / 2 > count {
            self.heights.shrink_to_fit();
        }
        if self.tree.capacity() / 2 > slots {
            self.tree.shrink_to_fit();
        }
        Ok(true)
    }
    fn prefix(&self, index: usize) -> f64 {
        let mut index = index.min(self.count());
        let mut sum = HeightSum::default();
        while index > 0 {
            sum.add(self.tree[index].sum);
            sum.add(self.tree[index].correction);
            index &= index - 1;
        }
        sum.value().max(0.)
    }
    /// How far down the list row `index` starts.
    pub fn row_offset(&self, index: usize) -> f32 {
        self.prefix(index).min(f64::from(f32::MAX)) as f32
    }
    /// The height of all the rows together.
    pub fn content_height(&self) -> f32 {
        self.row_offset(self.count())
    }
    /// Row containing an offset, without viewport clamping. Nonfinite or
    /// negative offsets select the first row; offsets at/after the total select
    /// the last row. Empty indices return `None`.
    pub fn row_at(&self, offset: f32) -> Option<usize> {
        if self.count() == 0 {
            return None;
        }
        let offset = if offset.is_finite() {
            f64::from(offset.max(0.))
        } else {
            0.
        };
        Some(self.search(offset, true).min(self.count() - 1))
    }
    /// Iterate `(index, offset, height)` in O(log count + returned rows).
    /// Both range endpoints clamp to count; reversed ranges are empty.
    /// Offsets saturate to finite `f32`, like `row_offset`.
    pub fn rows(&self, range: Range<usize>) -> impl Iterator<Item = (usize, f32, f32)> + '_ {
        let start = range.start.min(self.count());
        let end = range.end.min(self.count()).max(start);
        let mut offset = HeightSum::default();
        offset.add(self.prefix(start));
        self.heights[start..end]
            .iter()
            .copied()
            .enumerate()
            .map(move |(local, height)| {
                let top = offset.value().max(0.).min(f64::from(f32::MAX)) as f32;
                offset.add(f64::from(height));
                (start + local, top, height)
            })
    }
    // Largest prefix index whose offset is <= value (or < value).
    fn search(&self, value: f64, inclusive: bool) -> usize {
        let mut index = 0;
        let mut sum = HeightSum::default();
        let mut step = 1usize << (usize::BITS - self.count().leading_zeros() - 1);
        while step > 0 {
            let next = index + step;
            if next <= self.count() {
                let mut candidate = sum;
                candidate.add(self.tree[next].sum);
                candidate.add(self.tree[next].correction);
                if candidate.value() < value || (inclusive && candidate.value() == value) {
                    index = next;
                    sum = candidate;
                }
            }
            step >>= 1;
        }
        index
    }
    /// The rows a viewport `viewport_height` tall, scrolled `offset` down,
    /// shows, widened by `overscan` each way.
    pub fn visible_range(
        &self,
        offset: f32,
        viewport_height: f32,
        overscan: usize,
    ) -> Range<usize> {
        if self.count() == 0 || !viewport_height.is_finite() || viewport_height <= 0. {
            return 0..0;
        }
        let viewport = f64::from(viewport_height);
        let total = self.prefix(self.count());
        let offset = if offset.is_finite() {
            f64::from(offset.max(0.)).min((total - viewport).max(0.))
        } else {
            0.
        };
        let first = self.search(offset, true).min(self.count() - 1);
        let end = self
            .search(offset + viewport, false)
            .saturating_add(1)
            .min(self.count());
        first.saturating_sub(overscan)..end.saturating_add(overscan).min(self.count())
    }
}

#[cfg(test)]
mod height_index_tests {
    use super::*;
    #[test]
    fn heterogeneous_ranges_match_linear_oracle_after_updates() {
        let mut heights = [10., 30., 5., 60., 15., 1., 80.];
        let mut index = HeightIndex::new(heights.len(), 10.);
        for round in 0..8 {
            for (i, height) in heights.iter().copied().enumerate() {
                index.set_height(i, height).unwrap();
            }
            let total: f32 = heights.iter().sum();
            assert_eq!(index.content_height(), total);
            for viewport in [1., 10., 40., 500.] {
                for pixel in 0..240 {
                    let offset = (pixel as f32).min((total - viewport).max(0.));
                    let mut top = 0.;
                    let mut visible = Vec::new();
                    for (i, height) in heights.iter().enumerate() {
                        if top < offset + viewport && top + height > offset {
                            visible.push(i);
                        }
                        assert_eq!(index.row_offset(i), top);
                        top += height;
                    }
                    let range = visible[0]..visible.last().unwrap() + 1;
                    assert_eq!(index.visible_range(pixel as f32, viewport, 0), range);
                    assert_eq!(
                        index.visible_range(pixel as f32, viewport, 2),
                        range.start.saturating_sub(2)..(range.end + 2).min(heights.len())
                    );
                }
            }
            heights[round % 7] = 2. + round as f32 * 17.;
        }
    }
    #[test]
    fn row_lookup_and_sequential_geometry_follow_boundaries() {
        let mut index = HeightIndex::new(4, 10.);
        index.set_height(1, 5.).unwrap();
        index.set_height(2, 20.).unwrap();
        for (offset, row) in [
            (0., 0),
            (9., 0),
            (10., 1),
            (14., 1),
            (15., 2),
            (35., 3),
            (45., 3),
            (100., 3),
            (-1., 0),
            (f32::NAN, 0),
            (f32::INFINITY, 0),
        ] {
            assert_eq!(index.row_at(offset), Some(row));
        }
        assert_eq!(
            index.rows(1..99).collect::<Vec<_>>(),
            vec![(1, 10., 5.), (2, 15., 20.), (3, 35., 10.)]
        );
        for (row, top, height) in index.rows(0..4) {
            assert_eq!(top, index.row_offset(row));
            assert_eq!(Some(height), index.row_height(row));
        }
        assert_eq!(index.rows(Range { start: 3, end: 1 }).count(), 0);
        assert_eq!(index.rows(99..usize::MAX).count(), 0);
        let empty = HeightIndex::new(0, 10.);
        assert_eq!(empty.row_at(0.), None);
        assert_eq!(empty.rows(0..usize::MAX).count(), 0);
    }
    #[test]
    fn resize_preserves_rows_and_equal_updates_and_errors_are_nonmutating() {
        let mut index = HeightIndex::new(3, 12.);
        assert!(index.set_height(1, 30.).unwrap());
        assert!(!index.set_height(1, 30.).unwrap());
        for invalid in [0., -1., f32::NAN, f32::INFINITY] {
            assert_eq!(
                index.set_height(1, invalid),
                Err(HeightError::InvalidHeight)
            );
        }
        assert_eq!(
            index.set_height(3, 10.),
            Err(HeightError::Bounds { index: 3, count: 3 })
        );
        index.resize(5).unwrap();
        assert_eq!(index.row_offset(99), 78.);
        index.resize(1).unwrap();
        index.resize(3).unwrap();
        assert_eq!(index.content_height(), 36.);
        assert_eq!(index.row_height(1), Some(12.));
        assert_eq!(index.row_height(3), None);
        assert_eq!(index.resize(usize::MAX), Err(HeightError::Capacity));
        assert_eq!(index.count(), 3);
        index.resize(0).unwrap();
        assert_eq!(index.visible_range(0., 10., 2), 0..0);
    }
    #[test]
    fn finite_extremes_saturate_and_replacing_huge_rows_restores_small_sums() {
        let mut index = HeightIndex::new(4, 1.);
        index.set_height(0, f32::MAX).unwrap();
        index.set_height(1, f32::MAX).unwrap();
        assert_eq!(index.content_height(), f32::MAX);
        assert_eq!(index.visible_range(0., 10., 0), 0..1);
        index.set_height(0, 1.).unwrap();
        index.set_height(1, 2.).unwrap();
        assert_eq!(index.content_height(), 5.);
        assert_eq!(index.visible_range(1., 2., 0), 1..2);
        assert_eq!(index.visible_range(f32::NAN, 1., usize::MAX), 0..4);
        assert_eq!(index.visible_range(0., f32::INFINITY, 0), 0..0);
        assert!(HeightIndex::try_new(usize::MAX, 1.).is_err());
    }
}

#[cfg(test)]
mod height_growth_tests {
    use super::*;
    #[test]
    fn incremental_growth_and_truncation_match_fresh_prefixes() {
        let mut index = HeightIndex::new(0, 7.);
        let mut expected = Vec::new();
        for count in (1..130).chain((0..130).rev()).chain(1..260) {
            index.resize(count).unwrap();
            expected.resize(count, 7.);
            if count > 0 {
                let changed = count / 3;
                let height = (count % 19 + 1) as f32;
                index.set_height(changed, height).unwrap();
                expected[changed] = height;
            }
            let mut prefix = 0.;
            for (i, height) in expected.iter().enumerate() {
                assert_eq!(index.row_offset(i), prefix);
                prefix += height;
            }
            assert_eq!(index.content_height(), prefix);
        }
    }
    #[test]
    fn extending_tree_preserves_compensated_small_contributions() {
        let mut index = HeightIndex::new(3, 1.);
        index.set_height(0, f32::MAX).unwrap();
        index.resize(8).unwrap();
        index.set_height(0, 1.).unwrap();
        assert_eq!(index.content_height(), 8.);
        index.resize(5).unwrap();
        index.set_height(4, 9.).unwrap();
        index.resize(16).unwrap();
        assert_eq!(index.content_height(), 24.);
    }
    #[test]
    fn replacing_all_heights_matches_point_updates() {
        let heights: Vec<f32> = (0..1000).map(|i| 1. + (i * 37 % 91) as f32).collect();
        let mut points = HeightIndex::new(heights.len(), 5.);
        for (index, height) in heights.iter().enumerate() {
            points.set_height(index, *height).unwrap();
        }
        let mut replaced = HeightIndex::new(3, 5.);
        replaced.replace(heights.iter().copied()).unwrap();
        assert_eq!(replaced.count(), points.count());
        for index in 0..=heights.len() {
            assert_eq!(replaced.row_offset(index), points.row_offset(index));
        }
        assert_eq!(replaced.row_at(12_345.), points.row_at(12_345.));
        assert!(replaced.replace([1., f32::NAN]).is_err());
        assert_eq!(replaced.count(), heights.len(), "errors leave rows intact");
        replaced.resize(1003).unwrap();
        assert_eq!(replaced.content_height(), points.content_height() + 15.);
    }
}
