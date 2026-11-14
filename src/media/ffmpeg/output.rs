use std::path::{Path, PathBuf};

/// Publish a completed export only after FFmpeg has closed it successfully.
/// Keeping the temporary file beside the destination makes replacement atomic.
pub(super) struct ExportOutput {
    temporary: tempfile::TempPath,
    destination: PathBuf,
}

impl ExportOutput {
    pub(super) fn new<'a>(
        destination: &Path,
        inputs: impl IntoIterator<Item = &'a Path>,
    ) -> Result<Self, String> {
        match destination.try_exists() {
            Ok(true) => {
                for input in inputs {
                    if same_file::is_same_file(input, destination)
                        .map_err(|error| format!("Cannot compare export paths: {error}"))?
                    {
                        return Err("Export output must not overwrite an input file".to_owned());
                    }
                }
            }
            Ok(false) => {}
            Err(error) => return Err(format!("Cannot access export output: {error}")),
        }

        let parent = destination
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let temporary = tempfile::Builder::new()
            .prefix(".gop-composer-")
            .suffix(".tmp")
            .tempfile_in(parent)
            .map_err(|error| format!("Cannot create export output: {error}"))?
            .into_temp_path();
        Ok(Self {
            temporary,
            destination: destination.to_owned(),
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.temporary
    }

    pub(super) fn finish(self) -> Result<(), String> {
        self.temporary
            .persist(&self.destination)
            .map_err(|error| format!("Cannot save completed export: {error}"))
    }
}
