use std::ffi::c_void;

use vizia::{WindowScalePolicy, prelude::*};

use super::{LUMEDIR_EDITOR_HEIGHT, LUMEDIR_EDITOR_WIDTH, LumedirEditorHost, LumedirEditorSize};
use crate::vizia_window::ViziaWindowEditor;

const STYLE: &str = r#"
    .lumedir-root {
        background-color: #11161a;
        width: 1s;
        height: 1s;
        child-space: 16px;
        row-between: 10px;
    }
    .lumedir-title {
        color: #d8e0e4;
        font-size: 22px;
    }
    .lumedir-subtitle {
        color: #7c8a90;
        font-size: 13px;
    }
    .lumedir-meter {
        width: 220px;
        height: 16px;
        background-color: #2a7f6f;
        border-radius: 4px;
    }
"#;

/// Build the Lúmedir editor application. `_host` is threaded for parity with the other editors; the
/// M0 view is a static placeholder and does not read it yet (delivery snapshots arrive in M4).
fn build_lumedir_application(
    _host: LumedirEditorHost,
    size: LumedirEditorSize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(LUMEDIR_EDITOR_WIDTH) as u32;
    let height = size.height.max(LUMEDIR_EDITOR_HEIGHT) as u32;
    vizia::Application::new(move |cx| {
        cx.add_stylesheet(STYLE)
            .expect("failed to add lumedir editor style");
        build_placeholder_view(cx);
    })
    .ignore_default_theme()
    .title("Lumedir")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

fn build_placeholder_view(cx: &mut Context) {
    VStack::new(cx, |cx| {
        Label::new(cx, "Lumedir").class("lumedir-title");
        Label::new(cx, "Speech-Coach — editor embedding (M0)").class("lumedir-subtitle");
        // Static placeholder meter bar (no delivery data until M4); proves Vizia renders in the HWND.
        HStack::new(cx, |_cx| {}).class("lumedir-meter");
    })
    .class("lumedir-root");
}

/// The Lúmedir editor: a thin newtype over the shared [`ViziaWindowEditor`], which owns the
/// `IPlugView`→`HWND` attach and the close-on-drop teardown. The inner editor is an RAII guard —
/// held only so its `Drop` closes the host window — hence never read directly.
pub struct LumedirViziaEditor(#[allow(dead_code)] ViziaWindowEditor);

impl LumedirViziaEditor {
    /// # Safety
    /// `parent` must be a valid host child-window handle (an `HWND` on Windows).
    pub unsafe fn attach(
        parent: *mut c_void,
        host: LumedirEditorHost,
        size: LumedirEditorSize,
    ) -> Self {
        let application = build_lumedir_application(host, size);
        Self(unsafe { ViziaWindowEditor::attach(parent, application) })
    }
}
