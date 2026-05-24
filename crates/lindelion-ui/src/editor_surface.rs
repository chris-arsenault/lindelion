use std::marker::PhantomData;

pub trait EditorSurfaceSlot: Copy + Eq + 'static {
    const ALL: &'static [Self];

    fn index(self) -> usize;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditorControlKind {
    Knob,
    Slider {
        width: f32,
    },
    Binary {
        left_label: &'static str,
        right_label: &'static str,
        width: f32,
    },
    Segmented {
        labels: &'static [&'static str],
        width: f32,
    },
    Selector {
        labels: &'static [&'static str],
        width: f32,
    },
}

impl EditorControlKind {
    pub const fn slider() -> Self {
        Self::Slider { width: 0.0 }
    }

    pub const fn slider_with_width(width: f32) -> Self {
        Self::Slider { width }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorParameterBinding<Slot> {
    id: u32,
    slot: Slot,
    label: &'static str,
    control: EditorControlKind,
}

impl<Slot: Copy> EditorParameterBinding<Slot> {
    pub const fn new(id: u32, slot: Slot, label: &'static str, control: EditorControlKind) -> Self {
        Self {
            id,
            slot,
            label,
            control,
        }
    }

    pub const fn id(self) -> u32 {
        self.id
    }

    pub const fn slot(self) -> Slot {
        self.slot
    }

    pub const fn label(self) -> &'static str {
        self.label
    }

    pub const fn control(self) -> EditorControlKind {
        self.control
    }
}

pub trait EditorSurfaceBinding<Slot: EditorSurfaceSlot>: Copy {
    fn slot(self) -> Slot;
}

impl<Slot: EditorSurfaceSlot> EditorSurfaceBinding<Slot> for EditorParameterBinding<Slot> {
    fn slot(self) -> Slot {
        self.slot
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSurfaceHostError<Slot> {
    DuplicateSlot(Slot),
    MissingSlot(Slot),
    InvalidSlotIndex(Slot),
}

#[derive(Debug, Clone, Copy)]
pub struct CompleteSurfaceHost<Slot, Binding, const N: usize> {
    parameter_bindings: [Option<Binding>; N],
    _slot: PhantomData<Slot>,
}

impl<Slot, Binding, const N: usize> CompleteSurfaceHost<Slot, Binding, N>
where
    Slot: EditorSurfaceSlot,
    Binding: EditorSurfaceBinding<Slot>,
{
    pub fn new(
        bindings: impl IntoIterator<Item = Binding>,
    ) -> Result<Self, EditorSurfaceHostError<Slot>> {
        let mut parameter_bindings = [None; N];
        for binding in bindings {
            let slot = binding.slot();
            let Some(entry) = parameter_bindings.get_mut(slot.index()) else {
                return Err(EditorSurfaceHostError::InvalidSlotIndex(slot));
            };
            if entry.is_some() {
                return Err(EditorSurfaceHostError::DuplicateSlot(slot));
            }
            *entry = Some(binding);
        }

        for slot in Slot::ALL {
            let Some(entry) = parameter_bindings.get(slot.index()) else {
                return Err(EditorSurfaceHostError::InvalidSlotIndex(*slot));
            };
            if entry.is_none() {
                return Err(EditorSurfaceHostError::MissingSlot(*slot));
            }
        }

        Ok(Self {
            parameter_bindings,
            _slot: PhantomData,
        })
    }

    pub const fn parameter_bindings(self) -> [Option<Binding>; N] {
        self.parameter_bindings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestSlot {
        First,
        Second,
    }

    impl EditorSurfaceSlot for TestSlot {
        const ALL: &'static [Self] = &[Self::First, Self::Second];

        fn index(self) -> usize {
            match self {
                Self::First => 0,
                Self::Second => 1,
            }
        }
    }

    type TestBinding = EditorParameterBinding<TestSlot>;
    type TestHost = CompleteSurfaceHost<TestSlot, TestBinding, 2>;

    #[test]
    fn complete_surface_host_indexes_bindings_by_slot() {
        let host = TestHost::new([
            TestBinding::new(11, TestSlot::Second, "Second", EditorControlKind::slider()),
            TestBinding::new(10, TestSlot::First, "First", EditorControlKind::Knob),
        ])
        .unwrap();

        let bindings = host.parameter_bindings();
        assert_eq!(bindings[0].unwrap().id(), 10);
        assert_eq!(bindings[1].unwrap().id(), 11);
    }

    #[test]
    fn complete_surface_host_rejects_duplicate_slots() {
        let error = TestHost::new([
            TestBinding::new(10, TestSlot::First, "First", EditorControlKind::Knob),
            TestBinding::new(11, TestSlot::First, "Again", EditorControlKind::slider()),
        ])
        .unwrap_err();

        assert_eq!(
            error,
            EditorSurfaceHostError::DuplicateSlot(TestSlot::First)
        );
    }

    #[test]
    fn complete_surface_host_rejects_missing_slots() {
        let error = TestHost::new([TestBinding::new(
            10,
            TestSlot::First,
            "First",
            EditorControlKind::Knob,
        )])
        .unwrap_err();

        assert_eq!(error, EditorSurfaceHostError::MissingSlot(TestSlot::Second));
    }
}
