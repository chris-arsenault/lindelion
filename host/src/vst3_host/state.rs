//! Plugin opaque-state capture/restore — hand a plugin a host [`MemoryStream`] and let it write its
//! state into it (`getState`) or read its state out of it (`setState`).

use vst3::Steinberg::IBStream;
use vst3::Steinberg::Vst::{IComponent, IComponentTrait};
use vst3::{ComPtr, ComWrapper};

use super::bstream::MemoryStream;

/// Capture a plugin's opaque state as bytes (via `IComponent::getState`).
pub fn capture_state(component: &ComPtr<IComponent>) -> Vec<u8> {
    let stream = ComWrapper::new(MemoryStream::new());
    let iface = stream
        .to_com_ptr::<IBStream>()
        .expect("memory stream exposes IBStream");
    unsafe {
        component.getState(iface.as_ptr());
    }
    stream.bytes()
}

/// Restore a plugin's opaque state from `bytes` (via `IComponent::setState`).
pub fn restore_state(component: &ComPtr<IComponent>, bytes: &[u8]) {
    let stream = ComWrapper::new(MemoryStream::from_bytes(bytes.to_vec()));
    let iface = stream
        .to_com_ptr::<IBStream>()
        .expect("memory stream exposes IBStream");
    unsafe {
        component.setState(iface.as_ptr());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vst3_host::fixture::fixture_factory;
    use crate::vst3_host::{HostContext, PluginInstance};
    use vst3::Steinberg::Vst::IHostApplication;

    fn fixture_instance() -> PluginInstance {
        let factory = fixture_factory();
        let host = HostContext::new()
            .to_com_ptr::<IHostApplication>()
            .expect("IHostApplication");
        PluginInstance::from_factory(&factory, &host).expect("instance")
    }

    #[test]
    fn state_round_trips_through_capture_and_restore() {
        let original = fixture_instance();
        restore_state(original.component(), b"galad-state-42");
        assert_eq!(capture_state(original.component()), b"galad-state-42");

        // Round-trip the captured bytes through a fresh plugin (which starts empty).
        let captured = capture_state(original.component());
        let fresh = fixture_instance();
        assert_ne!(capture_state(fresh.component()), captured);
        restore_state(fresh.component(), &captured);
        assert_eq!(capture_state(fresh.component()), captured);
    }
}
