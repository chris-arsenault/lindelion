//! Stylesheet for the Lamath Stringed editor.

pub(super) const STYLE: &str = r#"
    .stringed-root {
        background-color: #101214;
        width: 1s;
        height: 1s;
        padding: 16px;
        vertical-gap: 12px;
    }
    .stringed-title { color: #e6e3d6; font-size: 22px; }
    .stringed-subtitle { color: #9da69c; font-size: 12px; }
    .stringed-header { horizontal-gap: 12px; }
    .stringed-header-text { width: 1s; vertical-gap: 2px; }
    .stringed-card-row { horizontal-gap: 12px; }
    .stringed-driver-card { width: 1s; }
    .stringed-player-card { width: auto; }
    .stringed-hint { color: #7e887e; font-size: 10px; }
    .stringed-strip {
        background-color: #171a1d;
        border-color: #34383c;
        border-width: 1px;
        border-radius: 6px;
        padding: 10px;
        vertical-gap: 8px;
    }
    .stringed-section { color: #bbc0b8; font-size: 12px; }
    .stringed-path { horizontal-gap: 10px; }
    .stringed-choice {
        background-color: #23272b;
        border-color: #44494e;
        border-width: 1px;
        border-radius: 5px;
        color: #cbd1c8;
        width: 76px;
        height: 30px;
    }
    .stringed-choice-on { border-color: #c68b4a; color: #f2dfc4; }
    .stringed-knobs { horizontal-gap: 10px; }
    .stringed-knob-cell { width: 82px; vertical-gap: 4px; alignment: center; }
    .stringed-knob { width: 44px; height: 44px; }
    .stringed-knob .knob-track { color: #c68b4a; background-color: #2b3033; }
    .stringed-knob .knob-head {
        background-color: #202427;
        border-color: #6a7068;
        border-width: 1px;
        color: #f3e2ca;
    }
    .stringed-knob .knob-tick {
        background-color: #f3e2ca;
        width: 2px;
        height: 9px;
        corner-radius: 1px;
    }
    .stringed-label { color: #d9ddd2; font-size: 11px; text-align: center; }
    .stringed-value { color: #919890; font-size: 10px; text-align: center; }
    .stringed-switch-row { horizontal-gap: 8px; }
    .stringed-switch {
        background-color: #22262a;
        border-color: #42484d;
        border-width: 1px;
        border-radius: 5px;
        color: #bec5bc;
        width: 116px;
        height: 30px;
    }
    .stringed-switch-on { border-color: #8eb782; color: #d8ead0; }
    .stringed-slot-row { horizontal-gap: 6px; }
    .stringed-slot {
        background-color: #202428;
        border-color: #3d4448;
        border-width: 1px;
        border-radius: 5px;
        width: 72px;
        height: 58px;
        padding: 5px;
        vertical-gap: 2px;
    }
    .stringed-slot-on { border-color: #c68b4a; }
    .stringed-slot-key { color: #9ba39b; font-size: 10px; text-align: center; }
    .stringed-slot-name { color: #e0e4d8; font-size: 10px; text-align: center; }
    .stringed-slot-source { color: #858f86; font-size: 10px; text-align: center; }
    .stringed-actions { horizontal-gap: 8px; alignment: center; }
    .stringed-current { color: #d9ddd2; width: 330px; }
    .stringed-button {
        background-color: #24282c;
        border-color: #42484d;
        border-width: 1px;
        border-radius: 5px;
        color: #dce2d8;
        width: 76px;
        padding: 6px;
    }
    .stringed-button:hover { border-color: #c68b4a; }
"#;
