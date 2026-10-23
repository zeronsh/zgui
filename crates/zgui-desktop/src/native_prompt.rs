//! Asynchronous native confirmation and informational prompts.
use crate::FileDialogError;
use std::sync::Arc;
use winit::window::Window;

/// How serious a prompt is: it chooses the prompt's icon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptLevel {
    /// Information.
    #[default]
    Info,
    /// A warning.
    Warning,
    /// An error.
    Error,
}
/// Which buttons a prompt offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptButtons {
    /// OK alone.
    #[default]
    Ok,
    /// OK and Cancel.
    OkCancel,
    /// Yes and No.
    YesNo,
    /// Yes, No and Cancel.
    YesNoCancel,
}
/// The button a prompt was answered with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptResponse {
    /// OK.
    Ok,
    /// Cancel.
    Cancel,
    /// Yes.
    Yes,
    /// No.
    No,
    /// A button the platform labelled otherwise, by its label.
    Custom(String),
}
/// A native prompt's text, level and buttons.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptOptions {
    /// Its heading.
    pub title: String,
    /// Its message, under the heading.
    pub description: String,
    /// How serious it is.
    pub level: PromptLevel,
    /// The buttons it offers.
    pub buttons: PromptButtons,
}
impl PromptOptions {
    /// An informational prompt with OK alone.
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            ..Self::default()
        }
    }
    /// Offer `buttons`.
    pub fn buttons(mut self, buttons: PromptButtons) -> Self {
        self.buttons = buttons;
        self
    }
    /// At `level`.
    pub fn level(mut self, level: PromptLevel) -> Self {
        self.level = level;
        self
    }
    pub(crate) fn validate(&self) -> Result<(), FileDialogError> {
        if self.title.contains('\0') || self.description.contains('\0') {
            Err(FileDialogError::InvalidOptions)
        } else {
            Ok(())
        }
    }
}
impl From<PromptLevel> for rfd::MessageLevel {
    fn from(value: PromptLevel) -> Self {
        match value {
            PromptLevel::Info => Self::Info,
            PromptLevel::Warning => Self::Warning,
            PromptLevel::Error => Self::Error,
        }
    }
}
impl From<PromptButtons> for rfd::MessageButtons {
    fn from(value: PromptButtons) -> Self {
        match value {
            PromptButtons::Ok => Self::Ok,
            PromptButtons::OkCancel => Self::OkCancel,
            PromptButtons::YesNo => Self::YesNo,
            PromptButtons::YesNoCancel => Self::YesNoCancel,
        }
    }
}
impl From<rfd::MessageDialogResult> for PromptResponse {
    fn from(value: rfd::MessageDialogResult) -> Self {
        match value {
            rfd::MessageDialogResult::Ok => Self::Ok,
            rfd::MessageDialogResult::Cancel => Self::Cancel,
            rfd::MessageDialogResult::Yes => Self::Yes,
            rfd::MessageDialogResult::No => Self::No,
            rfd::MessageDialogResult::Custom(s) => Self::Custom(s),
        }
    }
}
#[cfg(not(target_os = "macos"))]
pub(crate) async fn show(
    parent: Arc<Window>,
    options: PromptOptions,
) -> Result<PromptResponse, FileDialogError> {
    #[cfg(target_os = "linux")]
    drop(parent);
    #[cfg(target_os = "windows")]
    let owner = crate::file_dialog::Owner::new(parent);
    crate::file_dialog::background(move || {
        let dialog = rfd::MessageDialog::new()
            .set_title(options.title)
            .set_description(options.description)
            .set_level(options.level.into())
            .set_buttons(options.buttons.into());
        #[cfg(target_os = "windows")]
        let dialog = match &owner {
            Some(owner) => dialog.set_parent(owner),
            None => dialog,
        };
        dialog.show().into()
    })?
    .await
}
#[cfg(target_os = "macos")]
pub(crate) async fn show(
    parent: Arc<Window>,
    options: PromptOptions,
) -> Result<PromptResponse, FileDialogError> {
    Ok(rfd::AsyncMessageDialog::new()
        .set_parent(parent.as_ref())
        .set_title(options.title)
        .set_description(options.description)
        .set_level(options.level.into())
        .set_buttons(options.buttons.into())
        .show()
        .await
        .into())
}
