//! zgx's component library: composed, opinionated pieces built on zgx's
//! public API, as Radix's and the platforms' are. A data grid, a tree view,
//! a calendar, a date picker and a time field, a combobox, a select, tabs,
//! toasts and their host, a command palette, and a segmented control, a
//! stepper, a disclosure, an accordion and a separator. Each is keyboard
//! complete, says what it is to assistive technology, mirrors right to
//! left and takes the theme's tokens.
//!
//! The primitives they are made of (text, editors, buttons, checkboxes,
//! sliders, progress, scroll views and virtual lists, menus, popovers,
//! modals and tooltips, selection and drag) are zgx's own:
//! `zgx::compose`.
#![warn(missing_docs)]
mod combobox;
mod controls;
mod date;
mod grid;
mod palette;
mod select;
mod tabs;
mod toast;
mod tree;

pub use combobox::{ComboItem, ComboboxOptions, combobox};
pub use controls::{StepperOptions, accordion, disclosure, segmented, separator, stepper};
pub use date::{
    CalendarDate, CalendarOptions, TimeOfDay, TimeOptions, calendar, date_picker, days_in_month,
    is_leap_year, time_field,
};
pub use grid::{GridCell, GridColumn, GridSelection, GridSort, GridSource, data_grid};
pub use palette::{PaletteCommand, command_palette, fuzzy_score};
pub use select::{SelectOption, select, select_live};
pub use tabs::{CloseTab, Tab, TabsOptions, tabs};
pub use toast::{Toast, ToastAction, ToastKind, Toaster, toast_host};
pub use tree::{TreeChildren, TreeItem, tree};

/// The components, for `use zgx_components::prelude::*` beside
/// `zgx::compose::prelude::*`.
pub mod prelude {
    pub use super::{
        CalendarDate, CalendarOptions, ComboItem, ComboboxOptions, GridCell, GridColumn,
        GridSelection, GridSort, GridSource, PaletteCommand, SelectOption, StepperOptions, Tab,
        TabsOptions, TimeOfDay, TimeOptions, Toast, ToastKind, Toaster, TreeChildren, TreeItem,
        accordion, calendar, combobox, command_palette, data_grid, date_picker, disclosure,
        segmented, select, separator, stepper, tabs, time_field, toast_host, tree,
    };
}
