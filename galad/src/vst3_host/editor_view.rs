//! The platform-neutral half of attaching a plugin editor: confirm the view supports the host's
//! window platform (`HWND`), read its initial size, and wire the host frame. The real `HWND` creation
//! and `IPlugView::attached` are Windows-only (Step 5).

use vst3::ComPtr;
use vst3::Steinberg::*;

use super::instance::HostError;

/// Confirm `view` supports `HWND`, set its `frame`, and return its initial size.
pub fn prepare(
    view: &ComPtr<IPlugView>,
    frame: &ComPtr<IPlugFrame>,
) -> Result<ViewRect, HostError> {
    unsafe {
        crate::diagnostics::log("editor-view: isPlatformTypeSupported(HWND) begin");
        if view.isPlatformTypeSupported(kPlatformTypeHWND) != kResultTrue {
            crate::diagnostics::log("editor-view: isPlatformTypeSupported(HWND) failed");
            return Err(HostError::EditorUnsupported);
        }
        crate::diagnostics::log("editor-view: isPlatformTypeSupported(HWND) done");
        crate::diagnostics::log("editor-view: setFrame begin");
        view.setFrame(frame.as_ptr());
        crate::diagnostics::log("editor-view: setFrame done");
        let mut rect = ViewRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        crate::diagnostics::log("editor-view: getSize begin");
        view.getSize(&mut rect);
        crate::diagnostics::log(format!(
            "editor-view: getSize done rect=({}, {}, {}, {})",
            rect.left, rect.top, rect.right, rect.bottom
        ));
        Ok(rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vst3_host::fixture::{FixtureView, fixture_factory};
    use crate::vst3_host::{EditorController, HostContext, HostPlugFrame, PluginInstance};
    use vst3::ComWrapper;
    use vst3::Steinberg::Vst::IHostApplication;

    #[test]
    fn prepare_reports_view_size_and_sets_frame() {
        let factory = fixture_factory();
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");
        let instance = PluginInstance::from_factory(&factory, &host).expect("instance");
        let editor = EditorController::new(&factory, &instance, &host).expect("controller");
        let view = editor.create_view().expect("view");
        let frame = HostPlugFrame::new();
        let frame_ptr = frame.to_com_ptr::<IPlugFrame>().expect("IPlugFrame");

        let rect = prepare(&view, &frame_ptr).expect("prepare");

        assert_eq!((rect.right - rect.left, rect.bottom - rect.top), (320, 240));
    }

    #[test]
    fn prepare_errors_when_hwnd_unsupported() {
        let view = ComWrapper::new(FixtureView::without_hwnd())
            .to_com_ptr::<IPlugView>()
            .expect("IPlugView");
        let frame = HostPlugFrame::new();
        let frame_ptr = frame.to_com_ptr::<IPlugFrame>().expect("IPlugFrame");

        let result = prepare(&view, &frame_ptr);

        assert!(matches!(result, Err(HostError::EditorUnsupported)));
    }
}
