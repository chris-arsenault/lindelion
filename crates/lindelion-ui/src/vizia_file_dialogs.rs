use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    task::Poll,
};

use dispatch2::DispatchQueue;
use rfd::FileDialog;

#[derive(Clone, Copy)]
pub(crate) struct DialogParent;

impl DialogParent {
    pub(crate) fn from_ns_view(ns_view: usize) -> Option<Self> {
        (ns_view != 0).then_some(Self)
    }
}

pub(crate) struct PendingFileDialog {
    selection: Arc<Mutex<Option<Option<PathBuf>>>>,
}

impl PendingFileDialog {
    pub(crate) fn pick_file(dialog: FileDialog) -> Self {
        Self::spawn(dialog, FileDialogAction::PickFile)
    }

    pub(crate) fn pick_folder(dialog: FileDialog) -> Self {
        Self::spawn(dialog, FileDialogAction::PickFolder)
    }

    pub(crate) fn save_file(dialog: FileDialog) -> Self {
        Self::spawn(dialog, FileDialogAction::SaveFile)
    }

    pub(crate) fn poll_path(&mut self) -> Poll<Option<PathBuf>> {
        let Ok(mut selection) = self.selection.lock() else {
            return Poll::Ready(None);
        };
        match selection.take() {
            Some(path) => Poll::Ready(path),
            None => Poll::Pending,
        }
    }

    fn spawn(dialog: FileDialog, action: FileDialogAction) -> Self {
        let selection = Arc::new(Mutex::new(None));
        let selection_target = Arc::clone(&selection);
        DispatchQueue::main().exec_async(move || {
            let path = match action {
                FileDialogAction::PickFile => dialog.pick_file(),
                FileDialogAction::PickFolder => dialog.pick_folder(),
                FileDialogAction::SaveFile => dialog.save_file(),
            };
            if let Ok(mut selection) = selection_target.lock() {
                *selection = Some(path);
            }
        });
        Self { selection }
    }
}

#[derive(Clone, Copy)]
enum FileDialogAction {
    PickFile,
    PickFolder,
    SaveFile,
}

pub(crate) fn wav_audio_dialog(directory: &Path, parent: Option<DialogParent>) -> FileDialog {
    with_parent(
        FileDialog::new()
            .add_filter("WAV audio", &["wav", "wave"])
            .set_directory(directory),
        parent,
    )
}

pub(crate) fn patch_save_file_dialog(
    product_name: &'static str,
    directory: &Path,
    file_name: impl Into<String>,
    parent: Option<DialogParent>,
) -> FileDialog {
    with_parent(
        FileDialog::new()
            .add_filter(format!("{product_name} Patch"), &["toml"])
            .set_directory(directory)
            .set_file_name(file_name.into()),
        parent,
    )
}

pub(crate) fn patch_load_file_dialog(
    product_name: &'static str,
    directory: &Path,
    parent: Option<DialogParent>,
) -> FileDialog {
    with_parent(
        FileDialog::new()
            .add_filter(format!("{product_name} Patch"), &["toml"])
            .set_directory(directory),
        parent,
    )
}

pub(crate) fn patch_export_directory_dialog(
    directory: &Path,
    parent: Option<DialogParent>,
) -> FileDialog {
    with_parent(FileDialog::new().set_directory(directory), parent)
}

pub(crate) fn midi_save_file_dialog(
    file_name: impl Into<String>,
    parent: Option<DialogParent>,
) -> FileDialog {
    with_parent(
        FileDialog::new()
            .add_filter("MIDI", &["mid", "midi"])
            .set_file_name(file_name.into()),
        parent,
    )
}

fn with_parent(dialog: FileDialog, parent: Option<DialogParent>) -> FileDialog {
    let _ = parent;
    // In a VST editor, parenting RFD to the baseview NSView opens NSOpenPanel as a sheet.
    // Ableton then triggers a baseview key-window notification panic across AppKit.
    dialog
}
