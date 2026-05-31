use std::ffi::c_void;

use vizia::{ParentWindow, WindowHandle, WindowScalePolicy, prelude::*};

use super::{
    CENEDRIL_EDITOR_HEIGHT, CENEDRIL_EDITOR_WIDTH, CenedrilEditorHost, CenedrilEditorSize,
};

const STYLE: &str = r#"
    .cenedril-root {
        background-color: #11161a;
        width: 1s;
        height: 1s;
        child-space: 16px;
        row-between: 10px;
    }
    .cenedril-title {
        color: #d8e0e4;
        font-size: 22px;
    }
    .cenedril-subtitle {
        color: #7c8a90;
        font-size: 13px;
    }
    .cenedril-meter {
        width: 220px;
        height: 16px;
        background-color: #2a7f6f;
        border-radius: 4px;
    }
"#;

/// Build the Cenedril editor application. `_host`/`_parent_view` are threaded for parity with the
/// other editors; the M1 view is a static placeholder and reads neither yet (real data is M2).
fn build_cenedril_application(
    _host: CenedrilEditorHost,
    size: CenedrilEditorSize,
    _parent_view: usize,
) -> vizia::Application<impl Fn(&mut Context) + Send + 'static> {
    let width = size.width.max(CENEDRIL_EDITOR_WIDTH) as u32;
    let height = size.height.max(CENEDRIL_EDITOR_HEIGHT) as u32;
    vizia::Application::new(move |cx| {
        cx.add_stylesheet(STYLE)
            .expect("failed to add cenedril editor style");
        build_placeholder_view(cx);
    })
    .ignore_default_theme()
    .title("Cenedril")
    .inner_size((width, height))
    .with_scale_policy(WindowScalePolicy::ScaleFactor(1.0))
}

fn build_placeholder_view(cx: &mut Context) {
    VStack::new(cx, |cx| {
        Label::new(cx, "Cenedril").class("cenedril-title");
        Label::new(cx, "Visualizer — editor embedding spike (M1)").class("cenedril-subtitle");
        // Static placeholder meter bar (no signal data until M2); proves Vizia renders in the HWND.
        HStack::new(cx, |_cx| {}).class("cenedril-meter");
    })
    .class("cenedril-root");
}

pub struct CenedrilViziaEditor {
    window: WindowHandle,
}

impl CenedrilViziaEditor {
    /// # Safety
    /// `parent` must be a valid host child-window handle (an `HWND` on Windows).
    pub unsafe fn attach(
        parent: *mut c_void,
        host: CenedrilEditorHost,
        size: CenedrilEditorSize,
    ) -> Self {
        let parent_view = parent as usize;
        let parent = ParentWindow(parent);
        let window = build_cenedril_application(host, size, parent_view).open_parented(&parent);
        Self { window }
    }
}

impl Drop for CenedrilViziaEditor {
    fn drop(&mut self) {
        if self.window.is_open() {
            self.window.close();
        }
    }
}
