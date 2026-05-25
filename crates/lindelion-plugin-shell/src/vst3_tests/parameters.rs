use super::*;
use crate::{ParameterInfo as ShellParameterInfo, ParameterRange};

#[test]
fn parameter_info_helper_projects_shared_metadata() {
    let shell_info =
        ShellParameterInfo::continuous(42, "Gain", "dB", ParameterRange::linear(-12.0, 12.0, 0.0));
    let mut vst_info = unsafe { std::mem::zeroed::<ParameterInfo>() };

    assert_eq!(
        unsafe {
            fill_vst3_parameter_info(
                Vst3ParameterInfo::from_parameter(shell_info).hidden(),
                &mut vst_info,
            )
        },
        kResultOk
    );

    assert_eq!(vst_info.id, 42);
    assert_eq!(wide_string(&vst_info.title), "Gain");
    assert_eq!(wide_string(&vst_info.units), "dB");
    assert_eq!(vst_info.defaultNormalizedValue, 0.5);
    assert_ne!(
        vst_info.flags & ParameterInfo_::ParameterFlags_::kCanAutomate,
        0
    );
    assert_ne!(
        vst_info.flags & ParameterInfo_::ParameterFlags_::kIsHidden,
        0
    );
}

#[test]
fn parameter_string_helpers_parse_and_write_plain_values() {
    let mut input = [0 as TChar; 8];
    copy_wstring(" 6.5 ", &mut input);
    let mut output = [0 as TChar; 128];

    assert_eq!(
        unsafe { parse_vst3_plain_value_string(input.as_mut_ptr()) },
        Some(6.5)
    );
    assert_eq!(
        unsafe { write_vst3_parameter_string("6.50", &mut output) },
        kResultOk
    );
    assert_eq!(wide_string(&output), "6.50");
}
