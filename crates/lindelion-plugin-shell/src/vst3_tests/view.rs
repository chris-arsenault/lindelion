use super::*;

#[test]
fn fixed_size_plug_view_reports_and_enforces_declared_size() {
    let view = FixedSizePlugView::new(TestPlugViewDelegate, FixedSizePlugViewSize::new(320, 180));

    let mut size = unsafe { std::mem::zeroed::<ViewRect>() };
    assert_eq!(unsafe { view.getSize(&mut size) }, kResultOk);
    assert_rect(size, 0, 0, 320, 180);

    let mut requested = rect(12, 24, 640, 480);
    assert_eq!(unsafe { view.onSize(&mut requested) }, kResultOk);
    assert_eq!(unsafe { view.getSize(&mut size) }, kResultOk);
    assert_rect(size, 12, 24, 332, 204);

    assert_eq!(unsafe { view.canResize() }, kResultFalse);

    let mut constrained = rect(8, 16, 100, 100);
    assert_eq!(
        unsafe { view.checkSizeConstraint(&mut constrained) },
        kResultOk
    );
    assert_rect(constrained, 0, 0, 320, 180);
}

#[test]
fn fixed_size_plug_view_delegates_handled_paste_shortcuts() {
    let view = FixedSizePlugView::new(
        TestPastePlugViewDelegate,
        FixedSizePlugViewSize::new(320, 180),
    );

    assert_eq!(
        unsafe { view.onKeyDown(b'v' as u16, 0, KeyModifier_::kCommandKey as int16) },
        kResultTrue
    );
    assert_eq!(
        unsafe {
            view.onKeyDown(
                b'v' as u16,
                0,
                (KeyModifier_::kCommandKey | KeyModifier_::kShiftKey) as int16,
            )
        },
        kResultFalse
    );
    assert_eq!(
        unsafe { view.onKeyDown(b'x' as u16, 0, KeyModifier_::kCommandKey as int16) },
        kResultFalse
    );
}

struct TestPlugViewDelegate;

impl FixedSizePlugViewDelegate for TestPlugViewDelegate {
    unsafe fn attached(&self, _parent: *mut c_void, _size: ViewRect) -> tresult {
        kResultOk
    }
}

struct TestPastePlugViewDelegate;

impl FixedSizePlugViewDelegate for TestPastePlugViewDelegate {
    unsafe fn attached(&self, _parent: *mut c_void, _size: ViewRect) -> tresult {
        kResultOk
    }

    unsafe fn key_down(&self, event: PlugViewKeyEvent) -> tresult {
        if event.is_plain_paste_shortcut() {
            kResultTrue
        } else {
            kResultFalse
        }
    }
}
