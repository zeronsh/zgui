//! Asynchronous native confirmation and informational prompts.
use crate::FileDialogError;
use std::sync::Arc;
use winit::window::Window;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptLevel {
    #[default]
    Info,
    Warning,
    Error,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptButtons {
    #[default]
    Ok,
    OkCancel,
    YesNo,
    YesNoCancel,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptResponse {
    Ok,
    Cancel,
    Yes,
    No,
    Custom(String),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptOptions {
    pub title: String,
    pub description: String,
    pub level: PromptLevel,
    pub buttons: PromptButtons,
}
impl PromptOptions {
    pub fn new(title: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            ..Self::default()
        }
    }
    pub fn buttons(mut self, buttons: PromptButtons) -> Self {
        self.buttons = buttons;
        self
    }
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
    crate::file_dialog::background(move || {
        let dialog = rfd::MessageDialog::new()
            .set_title(options.title)
            .set_description(options.description)
            .set_level(options.level.into())
            .set_buttons(options.buttons.into());
        #[cfg(target_os = "windows")]
        let dialog = dialog.set_parent(parent.as_ref());
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
