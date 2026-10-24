//! Bounded reactive lists with stable row identity and independent length signals.
use crate::reactive::{Runtime, Signal};
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};

/// Why a [`List`] or [`Row`] operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListError {
    /// Appending would make the list longer than its capacity.
    Capacity {
        /// The length it would have had.
        requested: usize,
        /// The list's capacity.
        limit: usize,
    },
    /// The index is past the end of the list.
    Bounds {
        /// The index asked for.
        index: usize,
        /// The list's length.
        length: usize,
    },
    /// Row identifiers, or memory for another row, ran out.
    Exhausted,
    /// The row has been removed from its list.
    RowRemoved,
}
impl fmt::Display for ListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Capacity { requested, limit } => write!(
                f,
                "requested list length {requested} exceeds capacity {limit}"
            ),
            Self::Bounds { index, length } => {
                write!(f, "list index {index} exceeds length {length}")
            }
            Self::Exhausted => f.write_str("list row identifiers exhausted"),
            Self::RowRemoved => f.write_str("row has been removed"),
        }
    }
}
impl std::error::Error for ListError {}

/// A row keeps its identity when preceding rows are removed. Removed handles fail.
pub struct Row<T> {
    id: u64,
    value: Signal<Option<T>>,
}
impl<T> Clone for Row<T> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            value: self.value.clone(),
        }
    }
}
impl<T: 'static> Row<T> {
    /// Its identity: unique in its list, kept while rows before it are removed, and
    /// never reused.
    pub fn id(&self) -> u64 {
        self.id
    }
    /// Call `f` with its value, tracking it; `RowRemoved` once it is removed.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> Result<R, ListError> {
        self.value
            .with(|value| value.as_ref().map(f).ok_or(ListError::RowRemoved))
    }
    /// A clone of its value, tracking it; `RowRemoved` once it is removed.
    pub fn read(&self) -> Result<T, ListError>
    where
        T: Clone,
    {
        self.with(Clone::clone)
    }
    /// Equal writes are a no-op, including repeated writes through aliases.
    pub fn write(&self, value: T) -> Result<bool, ListError>
    where
        T: PartialEq,
    {
        if self.value.with_untracked(Option::is_none) {
            return Err(ListError::RowRemoved);
        }
        Ok(self.value.set(Some(value)))
    }
}

struct ListInner<T> {
    runtime: Runtime,
    rows: RefCell<Vec<Row<T>>>,
    length: Signal<usize>,
    structure: Signal<u64>,
    next_id: Cell<u64>,
    capacity: usize,
}
/// A reactive list of at most `capacity` rows, each holding its value in its
/// own signal. Clones share it.
pub struct List<T> {
    inner: Rc<ListInner<T>>,
}
impl<T> Clone for List<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
impl<T: PartialEq + 'static> List<T> {
    /// Capacity is a hard logical limit; storage is allocated only as rows arrive.
    pub fn new(runtime: &Runtime, capacity: usize) -> Self {
        Self {
            inner: Rc::new(ListInner {
                runtime: runtime.clone(),
                rows: RefCell::new(Vec::new()),
                length: runtime.signal(0),
                structure: runtime.signal(0),
                next_id: Cell::new(0),
                capacity,
            }),
        }
    }
    /// Its length (tracked; changes to rows' values do not notify).
    pub fn len(&self) -> usize {
        self.inner.length.get()
    }
    /// Whether it has no rows (tracked, as `len`).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// The most rows it can hold.
    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }
    /// Tracks positional membership, independently of row content.
    pub fn at(&self, index: usize) -> Result<Row<T>, ListError> {
        self.inner.structure.get();
        let rows = self.inner.rows.borrow();
        rows.get(index).cloned().ok_or(ListError::Bounds {
            index,
            length: rows.len(),
        })
    }
    /// Add a row holding `value` at the end and return it; `Capacity` when the list
    /// is full.
    pub fn append(&self, value: T) -> Result<Row<T>, ListError> {
        let length = self.inner.rows.borrow().len();
        let requested = length.checked_add(1).ok_or(ListError::Exhausted)?;
        if requested > self.inner.capacity {
            return Err(ListError::Capacity {
                requested,
                limit: self.inner.capacity,
            });
        }
        let id = self.inner.next_id.get();
        let next_id = id.checked_add(1).ok_or(ListError::Exhausted)?;
        self.inner
            .rows
            .borrow_mut()
            .try_reserve(1)
            .map_err(|_| ListError::Exhausted)?;
        self.inner.next_id.set(next_id);
        let row = Row {
            id,
            value: self.inner.runtime.signal(Some(value)),
        };
        self.inner.rows.borrow_mut().push(row.clone());
        self.inner.runtime.batch(|| {
            self.inner.length.set(requested);
            self.inner
                .structure
                .update(|version| *version = version.wrapping_add(1));
        });
        Ok(row)
    }
    /// Remove the row at `index`; its handles fail with `RowRemoved` from then on.
    pub fn remove(&self, index: usize) -> Result<(), ListError> {
        let row = {
            let mut rows = self.inner.rows.borrow_mut();
            if index >= rows.len() {
                return Err(ListError::Bounds {
                    index,
                    length: rows.len(),
                });
            }
            rows.remove(index)
        };
        self.inner.runtime.batch(|| {
            row.value.set(None);
            let length = self.inner.rows.borrow().len();
            self.inner.length.set(length);
            self.inner
                .structure
                .update(|version| *version = version.wrapping_add(1));
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn length_observers_ignore_content_and_appends_batch() {
        let runtime = Runtime::new();
        let list = List::new(&runtime, 4);
        let runs = Rc::new(Cell::new(0));
        let _effect = runtime.effect({
            let list = list.clone();
            let runs = runs.clone();
            move || {
                list.len();
                runs.set(runs.get() + 1);
            }
        });
        runtime.batch(|| {
            for value in [1, 2, 3] {
                list.append(value).unwrap();
            }
        });
        assert_eq!(runs.get(), 2);
        let row = list.at(0).unwrap();
        assert!(row.write(99).unwrap());
        assert!(!row.write(99).unwrap());
        assert_eq!(runs.get(), 2);
        list.append(4).unwrap();
        assert_eq!(
            list.append(5).err(),
            Some(ListError::Capacity {
                requested: 5,
                limit: 4
            })
        );
    }
    #[test]
    fn stable_handles_and_removed_row_notifications() {
        let runtime = Runtime::new();
        let list = List::new(&runtime, 3);
        let first = list.append(1).unwrap();
        let second = list.append(2).unwrap();
        let observed = Rc::new(RefCell::new(Ok(0)));
        let _effect = runtime.effect({
            let first = first.clone();
            let observed = observed.clone();
            move || {
                *observed.borrow_mut() = first.read();
            }
        });
        list.remove(0).unwrap();
        assert_eq!(*observed.borrow(), Err(ListError::RowRemoved));
        assert_eq!(first.write(8), Err(ListError::RowRemoved));
        assert_eq!(second.id(), list.at(0).unwrap().id());
        assert_eq!(second.read(), Ok(2));
        assert_eq!(
            list.at(3).err(),
            Some(ListError::Bounds {
                index: 3,
                length: 1
            })
        );
    }
}
