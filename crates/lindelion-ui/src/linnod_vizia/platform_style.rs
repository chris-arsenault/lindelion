const STYLE: &str = r#"
    :root {
        background-color: #101315;
        color: #d9e1dd;
        font-size: 12px;
    }

    label { color: #cbd5d0; }
    .root { background-color: #101315; }
    .topbar, .panel {
        background-color: #171b1d;
        border-width: 1px;
        border-color: #2b363a;
        border-radius: 8px;
        padding: 12px;
    }
    .source-view {
        background-color: #0e1214;
        border-width: 1px;
        border-color: #2a373c;
        border-radius: 7px;
    }
    .source-hot { border-color: #8bc7a1; }
    .title { color: #edf5ef; font-size: 18px; }
    .section-title { color: #edf5ef; font-size: 12px; }
    .muted, .meter-label { color: #81908a; font-size: 10px; }
    .value-label { color: #e6eee9; font-size: 11px; }
    .status-chip {
        background-color: #20282b;
        border-width: 1px;
        border-color: #39464b;
        border-radius: 6px;
        color: #c8d4cf;
        padding-left: 8px;
        padding-right: 8px;
    }
    .chip-ready { background-color: #24372d; border-color: #79ad89; }
    .chip-warn { background-color: #3a3022; border-color: #b5864f; }
    .chip-error { background-color: #3a2528; border-color: #b05d63; }
    .segmented {
        background-color: #101516;
        border-width: 1px;
        border-color: #303b3f;
        border-radius: 6px;
        padding: 2px;
    }
    button.seg-button {
        background-color: transparent;
        border-width: 0px;
        border-radius: 4px;
        color: #8f9d98;
        font-size: 10px;
    }
    button.seg-button:hover { background-color: #242d30; color: #e0e9e4; }
    button.seg-active { background-color: #315241; color: #f0f8f2; }
    button.toolbar-button, button.pad-button, button.slice-row, button.step-button {
        background-color: #20282b;
        border-width: 1px;
        border-color: #39464b;
        border-radius: 6px;
        color: #dce6e0;
    }
    button.toolbar-button:hover, button.pad-button:hover,
    button.slice-row:hover, button.step-button:hover {
        background-color: #273235;
        border-color: #78a891;
    }
    button.pad-selected, button.slice-selected {
        background-color: #26392f;
        border-color: #7fc49c;
    }
    button.choke-active { background-color: #344735; border-color: #90bd75; }
    .ll-playback-panel {
        background-color: #111719;
        border-width: 1px;
        border-color: #3e3327;
        border-radius: 5px;
        padding: 7px;
    }
    button.ll-check-button {
        background-color: #151b1d;
        border-width: 1px;
        border-color: #303b3f;
        border-radius: 5px;
        padding-left: 7px;
        padding-right: 7px;
    }
    button.ll-check-button:hover { border-color: #f2a84b; }
    button.ll-check-on { background-color: #3f3020; border-color: #f2a84b; }
    .ll-check-indicator {
        background-color: #0d1112;
        border-width: 1px;
        border-color: #5b6661;
        border-radius: 2px;
        width: 10px;
        height: 10px;
    }
    .ll-check-indicator-on {
        background-color: #f2a84b;
        border-color: #ffd28a;
    }
    .toolbar-icon { color: #dce6e0; width: 17px; height: 17px; }
    slider { height: 22px; }
    slider .track { background-color: #263136; border-radius: 4px; }
    slider .active { background-color: #82bc98; border-radius: 4px; }
    slider .thumb {
        background-color: #eef6f0;
        border-color: #0e1112;
        border-width: 1px;
        border-radius: 6px;
        width: 13px;
        height: 18px;
    }
    .tooltip {
        background-color: #20282b;
        border-width: 1px;
        border-color: #48555a;
        border-radius: 5px;
    }
"#;
