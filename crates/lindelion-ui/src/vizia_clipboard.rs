use std::path::{Path, PathBuf};

use objc2::rc::{Retained, autoreleasepool};
use objc2_app_kit::{
    NSFileContentsPboardType, NSPasteboard, NSPasteboardType, NSPasteboardTypeFileURL,
    NSPasteboardTypeSound, NSPasteboardTypeString, NSPasteboardTypeURL, NSURLNSPasteboardSupport,
};
#[allow(deprecated)]
use objc2_app_kit::{NSFilenamesPboardType, NSURLPboardType};
use objc2_foundation::{NSArray, NSString, NSURL};

pub(crate) fn file_pasteboard_types() -> Retained<NSArray<NSString>> {
    #[allow(deprecated)]
    NSArray::from_slice(&[
        unsafe { NSPasteboardTypeFileURL },
        unsafe { NSPasteboardTypeURL },
        unsafe { NSPasteboardTypeString },
        unsafe { NSPasteboardTypeSound },
        unsafe { NSFileContentsPboardType },
        unsafe { NSURLPboardType },
        unsafe { NSFilenamesPboardType },
    ])
}

pub(crate) fn pasteboard_has_file_source(pasteboard: &NSPasteboard) -> bool {
    pasteboard
        .availableTypeFromArray(&file_pasteboard_types())
        .is_some()
}

pub(crate) fn file_path_from_general_pasteboard() -> Option<PathBuf> {
    autoreleasepool(|_| {
        let pasteboard = NSPasteboard::generalPasteboard();
        file_path_from_pasteboard(&pasteboard)
    })
}

pub(crate) fn read_file_contents_from_general_pasteboard(destination: &Path) -> Option<PathBuf> {
    autoreleasepool(|_| {
        let pasteboard = NSPasteboard::generalPasteboard();
        read_file_contents_from_pasteboard(&pasteboard, destination)
    })
}

pub(crate) fn file_path_from_pasteboard(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    file_path_from_url_type(pasteboard, unsafe { NSPasteboardTypeFileURL })
        .or_else(|| file_path_from_url_type(pasteboard, unsafe { NSPasteboardTypeURL }))
        .or_else(|| {
            #[allow(deprecated)]
            file_path_from_url_type(pasteboard, unsafe { NSURLPboardType })
        })
        .or_else(|| file_path_from_url_object(pasteboard))
        .or_else(|| {
            #[allow(deprecated)]
            file_path_from_filenames(pasteboard)
        })
        .or_else(|| file_path_from_plain_string(pasteboard))
}

pub(crate) fn read_file_contents_from_pasteboard(
    pasteboard: &NSPasteboard,
    destination: &Path,
) -> Option<PathBuf> {
    let destination = destination.to_string_lossy();
    let destination = NSString::from_str(destination.as_ref());
    for pasteboard_type in file_content_pasteboard_types() {
        if pasteboard
            .readFileContentsType_toFile(pasteboard_type, &destination)
            .is_some()
        {
            return Some(PathBuf::from(destination.to_string()));
        }
    }
    None
}

fn file_content_pasteboard_types() -> [Option<&'static NSPasteboardType>; 3] {
    #[allow(deprecated)]
    [
        Some(unsafe { NSPasteboardTypeSound }),
        Some(unsafe { NSFileContentsPboardType }),
        None,
    ]
}

fn file_path_from_url_type(
    pasteboard: &NSPasteboard,
    pasteboard_type: &NSString,
) -> Option<PathBuf> {
    let url_text = pasteboard.stringForType(pasteboard_type)?;
    file_path_from_url_text(&url_text)
}

fn file_path_from_url_object(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    let url = NSURL::URLFromPasteboard(pasteboard)?;
    file_path_from_url(&url)
}

#[allow(deprecated)]
fn file_path_from_filenames(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    let property = pasteboard.propertyListForType(unsafe { NSFilenamesPboardType })?;
    let filenames = property.downcast_ref::<NSArray>()?;
    for filename in filenames {
        if let Some(filename) = filename.downcast_ref::<NSString>() {
            return Some(PathBuf::from(filename.to_string()));
        }
    }
    None
}

fn file_path_from_plain_string(pasteboard: &NSPasteboard) -> Option<PathBuf> {
    let text = pasteboard.stringForType(unsafe { NSPasteboardTypeString })?;
    let path = PathBuf::from(text.to_string());
    path.is_absolute().then_some(path)
}

fn file_path_from_url_text(url_text: &NSString) -> Option<PathBuf> {
    let url = NSURL::URLWithString(url_text)?;
    file_path_from_url(&url)
}

fn file_path_from_url(url: &NSURL) -> Option<PathBuf> {
    if !url.isFileURL() {
        return None;
    }
    url.path().map(|path| PathBuf::from(path.to_string()))
}
