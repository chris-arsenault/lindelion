use lindelion_plugin_shell::{ParameterEditorBindingProjection, ParameterId};
use lindelion_ui::resonator_vizia::ResonatorEditorControlKind as EditorControlKind;
pub(crate) use lindelion_ui::resonator_vizia::ResonatorEditorSurfaceSlot as EditorSurfaceSlot;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct EditorParameterBinding {
    slot: EditorSurfaceSlot,
    label: &'static str,
    control: EditorControlKind,
}

impl EditorParameterBinding {
    pub(crate) const fn knob(slot: EditorSurfaceSlot, label: &'static str) -> Self {
        Self {
            slot,
            label,
            control: EditorControlKind::Knob,
        }
    }

    pub(crate) const fn slider(slot: EditorSurfaceSlot, label: &'static str) -> Self {
        Self {
            slot,
            label,
            control: EditorControlKind::slider(),
        }
    }

    pub(crate) const fn binary(
        slot: EditorSurfaceSlot,
        label: &'static str,
        left_label: &'static str,
        right_label: &'static str,
        width: f32,
    ) -> Self {
        Self {
            slot,
            label,
            control: EditorControlKind::Binary {
                left_label,
                right_label,
                width,
            },
        }
    }

    pub(crate) const fn segmented(
        slot: EditorSurfaceSlot,
        label: &'static str,
        labels: &'static [&'static str],
        width: f32,
    ) -> Self {
        Self {
            slot,
            label,
            control: EditorControlKind::Segmented { labels, width },
        }
    }

    pub(crate) const fn selector(
        slot: EditorSurfaceSlot,
        label: &'static str,
        labels: &'static [&'static str],
        width: f32,
    ) -> Self {
        Self {
            slot,
            label,
            control: EditorControlKind::Selector { labels, width },
        }
    }
}

impl
    ParameterEditorBindingProjection<lindelion_ui::resonator_vizia::ResonatorEditorParameterBinding>
    for EditorParameterBinding
{
    fn project_editor_binding(
        self,
        id: ParameterId,
    ) -> lindelion_ui::resonator_vizia::ResonatorEditorParameterBinding {
        lindelion_ui::resonator_vizia::ResonatorEditorParameterBinding::new(
            id.0,
            self.slot,
            self.label,
            self.control,
        )
    }
}
