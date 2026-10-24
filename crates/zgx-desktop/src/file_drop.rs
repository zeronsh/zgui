//! Native event-loop batch assembly. Reject complete oversized batches; never truncate.
use std::{path::PathBuf, sync::Arc};
use zgx::input::FileDropError;
pub(crate) const MAX_PATHS: usize = 1024;
pub(crate) const MAX_BYTES: usize = 1024 * 1024;
#[derive(Default)]
pub(crate) struct FileDropBatch {
    paths: Vec<PathBuf>,
    bytes: usize,
    error: Option<FileDropError>,
}
impl FileDropBatch {
    pub fn push(&mut self, path: PathBuf) {
        if self.error.is_some() {
            return;
        }
        let bytes = path.as_os_str().as_encoded_bytes().len();
        let error = if self.paths.len() >= MAX_PATHS {
            Some(FileDropError::TooManyFiles)
        } else if bytes > MAX_BYTES.saturating_sub(self.bytes) {
            Some(FileDropError::TooLarge)
        } else {
            None
        };
        if let Some(error) = error {
            self.paths = Vec::new();
            self.bytes = 0;
            self.error = Some(error);
            return;
        }
        self.bytes += bytes;
        self.paths.push(path);
    }
    pub fn take(&mut self) -> Result<Option<Arc<[PathBuf]>>, FileDropError> {
        let batch = std::mem::take(self);
        if let Some(error) = batch.error {
            return Err(error);
        }
        Ok((!batch.paths.is_empty()).then(|| batch.paths.into()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn complete_batch_and_rejection_reset() {
        let mut batch = FileDropBatch::default();
        batch.push("/a".into());
        batch.push("/b".into());
        assert_eq!(
            &*batch.take().unwrap().unwrap(),
            &[PathBuf::from("/a"), PathBuf::from("/b")]
        );
        assert!(batch.take().unwrap().is_none());
        for _ in 0..=MAX_PATHS {
            batch.push("/a".into());
        }
        assert_eq!(batch.take(), Err(FileDropError::TooManyFiles));
        batch.push("/new".into());
        assert_eq!(batch.take().unwrap().unwrap().len(), 1);
    }
    #[test]
    fn byte_limit_rejects_without_retaining_partial_paths() {
        let mut batch = FileDropBatch::default();
        batch.push("/a".into());
        batch.push("x".repeat(MAX_BYTES).into());
        assert!(batch.paths.is_empty());
        assert_eq!(batch.take(), Err(FileDropError::TooLarge));
    }
}
