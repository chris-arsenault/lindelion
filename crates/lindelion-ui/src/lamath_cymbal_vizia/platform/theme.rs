//! Inline editor art (SVG cymbal) and the stylesheet.

/// A top-down cymbal: a bronze disc with tonal grooves, a raised bell, and a soft highlight. Drawn
/// as inline SVG so the editor needs no image assets.
pub(super) const CYMBAL_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 240 240">
  <defs>
    <radialGradient id="disc" cx="42%" cy="36%" r="72%">
      <stop offset="0%" stop-color="#f7e4b4"/>
      <stop offset="34%" stop-color="#dcb863"/>
      <stop offset="68%" stop-color="#b27f33"/>
      <stop offset="100%" stop-color="#6c4a1c"/>
    </radialGradient>
    <radialGradient id="bell" cx="40%" cy="34%" r="74%">
      <stop offset="0%" stop-color="#fff0c6"/>
      <stop offset="58%" stop-color="#e3bb62"/>
      <stop offset="100%" stop-color="#94661f"/>
    </radialGradient>
  </defs>
  <circle cx="120" cy="120" r="116" fill="url(#disc)" stroke="#523813" stroke-width="2"/>
  <g fill="none" stroke="#7d5921" stroke-width="1.1" opacity="0.5">
    <circle cx="120" cy="120" r="106"/>
    <circle cx="120" cy="120" r="96"/>
    <circle cx="120" cy="120" r="86"/>
    <circle cx="120" cy="120" r="76"/>
    <circle cx="120" cy="120" r="66"/>
    <circle cx="120" cy="120" r="56"/>
    <circle cx="120" cy="120" r="46"/>
  </g>
  <circle cx="120" cy="120" r="34" fill="url(#bell)" stroke="#75531c" stroke-width="1.5"/>
  <ellipse cx="92" cy="84" rx="48" ry="30" fill="#fff7df" opacity="0.16"/>
</svg>"##;

pub(super) const STYLE: &str = r#"
    .cymbal-root {
        background-color: #101418;
        width: 1s;
        height: 1s;
        padding: 18px;
        vertical-gap: 14px;
    }
    .cymbal-header { width: 1s; height: 48px; alignment: left; }
    .cymbal-title-block { width: 1s; vertical-gap: 2px; }
    .cymbal-title { color: #f1e8d4; font-size: 23px; }
    .cymbal-subtitle { color: #8e9a98; font-size: 12px; }

    .cymbal-perf { horizontal-gap: 8px; alignment: center; }
    .cymbal-perf-readout { width: 156px; vertical-gap: 3px; alignment: left; }
    .cymbal-perf-cap { color: #76827d; font-size: 9px; }
    .cymbal-perf-track {
        width: 156px;
        height: 7px;
        background-color: #0f1416;
        border-width: 1px;
        border-color: #303b3f;
        border-radius: 4px;
        overflow: hidden;
    }
    .cymbal-perf-fill {
        height: 1s;
        background-color: #6fcf72;
        border-radius: 3px;
    }
    .cymbal-perf-fill.cymbal-perf-warn { background-color: #e0a83f; }
    .cymbal-perf-fill.cymbal-perf-bad { background-color: #e05f5f; }
    .cymbal-perf-text { color: #9bc9a2; font-size: 10px; }
    .cymbal-perf-text.cymbal-perf-warn { color: #e7c074; }
    .cymbal-perf-text.cymbal-perf-bad { color: #f0a0a0; }
    .cymbal-perf-reset {
        background-color: #20282b;
        border-width: 1px;
        border-color: #3a4446;
        border-radius: 5px;
        color: #cbd3d0;
        font-size: 10px;
        padding-left: 8px;
        padding-right: 8px;
        height: 24px;
        alignment: center;
    }
    .cymbal-perf-reset:hover { border-color: #c99c45; }

    .cymbal-tabs {
        background-color: #0f1416;
        border-width: 1px;
        border-color: #303b3f;
        border-radius: 6px;
        padding: 3px;
        horizontal-gap: 3px;
        height: 32px;
    }
    button.cymbal-tab {
        background-color: transparent;
        border-width: 0px;
        border-radius: 4px;
        color: #9ba7a2;
        font-size: 12px;
        padding-left: 16px;
        padding-right: 16px;
        height: 1s;
        alignment: center;
    }
    button.cymbal-tab:hover { background-color: #1d2528; color: #e6eee9; }
    button.cymbal-tab-active { background-color: #3a2f1c; color: #f1e4c4; }

    .cymbal-pane { width: 1s; height: 1s; vertical-gap: 14px; }

    .cymbal-basic-top { width: 1s; height: 1s; horizontal-gap: 18px; }
    .cymbal-image-frame {
        background-color: #14191d;
        border-width: 1px;
        border-color: #3a4446;
        border-radius: 8px;
        width: 300px;
        height: 1s;
        padding: 14px;
        alignment: center;
    }
    .cymbal-image { width: 248px; height: 248px; }
    .cymbal-voice-col { width: 1s; height: 1s; vertical-gap: 12px; }
    .cymbal-voice-hint { color: #7f8b88; font-size: 12px; }
    .cymbal-voice-desc {
        color: #cdd6d1;
        font-size: 13px;
        background-color: #14191d;
        border-width: 1px;
        border-color: #2c3437;
        border-radius: 6px;
        padding: 12px;
        width: 1s;
    }

    .cymbal-panel {
        background-color: #171d20;
        border-width: 1px;
        border-color: #30383a;
        border-radius: 6px;
        padding: 12px;
        vertical-gap: 10px;
        width: 1s;
    }
    .cymbal-section { color: #c0a868; font-size: 13px; }
    .cymbal-knob-grid { horizontal-gap: 10px; vertical-gap: 10px; alignment: left; }
    .cymbal-knob-cell { width: 76px; vertical-gap: 4px; alignment: center; }
    .cymbal-knob { width: 46px; height: 46px; }
    .cymbal-knob .knob-track { color: #c99c45; background-color: #2b3032; }
    .cymbal-knob .knob-head {
        background-color: #1e2426;
        border-width: 1px;
        border-color: #65706d;
        color: #f1e8d4;
    }
    .cymbal-knob:hover .knob-head { border-color: #e2d6be; }
    .cymbal-knob .knob-tick {
        background-color: #f1e8d4;
        width: 2px;
        height: 9px;
        corner-radius: 1px;
    }
    .cymbal-knob-label { color: #cbd3d0; font-size: 11px; text-align: center; }
    .cymbal-knob-value { color: #8e9a98; font-size: 10px; text-align: center; }

    .cymbal-slot-row { horizontal-gap: 8px; }
    .cymbal-slot {
        background-color: #202729;
        border-width: 1px;
        border-color: #3a4446;
        border-radius: 5px;
        width: 110px;
        height: 60px;
        padding: 5px;
        vertical-gap: 2px;
    }
    .cymbal-slot-selected { border-color: #c99c45; }
    .cymbal-slot-key { color: #92a09c; font-size: 10px; text-align: center; }
    .cymbal-slot-label { color: #dde5e0; font-size: 11px; text-align: center; }
    .cymbal-slot-source { color: #7f908a; font-size: 10px; text-align: center; }
    .cymbal-action-row { horizontal-gap: 8px; alignment: left; }
    .cymbal-selected-label { color: #d3ded8; width: 1s; }
    .cymbal-button {
        background-color: #232a2d;
        border-radius: 5px;
        border-width: 1px;
        border-color: #3a4446;
        color: #dce2e0;
        padding: 6px;
        width: 76px;
        alignment: center;
    }
    .cymbal-button:hover { border-color: #c99c45; }

    /* Preset dropdown (Select). The default theme is ignored, so the layout primitives the popup
       relies on are restated here alongside the bronze appearance. */
    select.cymbal-select { width: 1s; height: 34px; min-width: 64px; }
    select.cymbal-select > button {
        background-color: #14191d;
        border-width: 1px;
        border-color: #45525a;
        border-radius: 6px;
        color: #f1e8d4;
        height: 1s;
        width: 1s;
        padding-left: 10px;
        padding-right: 10px;
        alignment: left;
    }
    select.cymbal-select > button:hover { border-color: #c99c45; }
    select.cymbal-select .icon { color: #c0a868; width: 16px; height: 16px; }
    select.cymbal-select popup { max-height: 340px; }

    popup {
        size: auto;
        min-width: 100%;
        left: 1s;
        right: 1s;
        z-index: 100;
        background-color: #1a2024;
        border-width: 1px;
        border-color: #45525a;
        border-radius: 6px;
        padding: 4px;
        shadow: 0px 4px 14px #00000055;
    }
    popup > list { width: 1s; min-width: auto; height: auto; max-height: 320px; }
    popup > list > scrollview { height: auto; max-height: 320px; width: 1s; min-width: auto; overflow: hidden; }
    popup > list > scrollview > scroll-content { size: auto; min-width: 100%; }
    popup scrollview scrollbar.vertical { top: 4px; bottom: 4px; right: 2px; width: 8px; }
    popup scrollbar .thumb { background-color: #4a575c; corner-radius: 4px; width: 1s; }
    popup list.selectable list-item {
        width: 1s;
        min-width: auto;
        height: 30px;
        padding-left: 8px;
        padding-right: 8px;
        alignment: left;
        horizontal-gap: 6px;
        layout-type: row;
        border-radius: 4px;
        color: #cdd6d1;
        background-color: transparent;
    }
    popup list.selectable list-item:hover { background-color: #273035; }
    popup list.selectable list-item:checked { background-color: #3a2f1c; color: #f1e4c4; }
    select.cymbal-select list-item .checkmark { fill: #c99c45; color: #c99c45; visibility: hidden; }
    select.cymbal-select list-item:checked .checkmark { visibility: visible; }
"#;
