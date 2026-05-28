const STYLE: &str = r#"
    :root {
        background-color: #101315;
        color: #d9e1dd;
        font-size: 12px;
    }

    label {
        color: #cbd4cf;
    }

    .muted {
        color: #7e8a86;
    }

    .title {
        font-size: 17px;
        color: #edf5ef;
    }

    .section-title {
        font-size: 12px;
        color: #edf5ef;
    }

    .root {
        background-color: #101315;
    }

    .topbar {
        background-color: #171b1d;
        border-width: 1px;
        border-color: #283036;
        border-radius: 8px;
        padding: 14px;
    }

    .panel {
        background-color: #151a1d;
        border-width: 1px;
        border-color: #283139;
        border-radius: 8px;
        padding: 14px;
    }

    .strip {
        background-color: #111619;
        border-width: 1px;
        border-color: #263239;
        border-radius: 6px;
    }

    .slot-row {
        background-color: #1b2124;
        border-width: 1px;
        border-color: #2f3a40;
        border-radius: 6px;
        padding: 9px;
    }

    .slot-active {
        border-color: #6da684;
    }

    .sample-row {
        background-color: #1b2124;
        border-width: 1px;
        border-color: #2f3a40;
        border-radius: 6px;
        padding: 6px;
    }

    .sample-selected {
        border-color: #7fc49c;
        background-color: #202a25;
    }

    .chip {
        background-color: #20282d;
        border-width: 1px;
        border-color: #37434a;
        border-radius: 6px;
        color: #b9c7c0;
        font-size: 10px;
        padding-left: 8px;
        padding-right: 8px;
    }

    .chip-on {
        background-color: #26392f;
        border-color: #6da684;
        color: #d8efe0;
    }

    .chip-warm {
        background-color: #3a3124;
        border-color: #b2844c;
        color: #efd8b7;
    }

    button.toolbar-button {
        background-color: #20272b;
        border-width: 1px;
        border-color: #39454d;
        border-radius: 6px;
        color: #dce6e0;
    }

    button.toolbar-button:hover {
        background-color: #263139;
        border-color: #6d91a6;
    }

    .segmented {
        background-color: #0f1417;
        border-width: 1px;
        border-color: #2c373e;
        border-radius: 6px;
        padding: 2px;
    }

    button.seg-button {
        background-color: transparent;
        border-width: 0px;
        border-radius: 4px;
        color: #8f9c97;
        font-size: 10px;
    }

    button.seg-button:hover {
        background-color: #20282d;
        color: #d9e1dd;
    }

    button.seg-active {
        background-color: #2b4436;
        color: #e5f5e9;
    }

    .toolbar-icon {
        color: #dce6e0;
        width: 17px;
        height: 17px;
    }

    .meter-label {
        color: #8f9c97;
        font-size: 10px;
    }

    .value-label {
        color: #e8f0ea;
        font-size: 11px;
    }

    slider {
        height: 22px;
    }

    slider .track {
        background-color: #253038;
        border-radius: 4px;
    }

    slider .active {
        background-color: #82bc98;
        border-radius: 4px;
    }

    slider .thumb {
        background-color: #e8f0ea;
        border-color: #0f1214;
        border-width: 1px;
        border-radius: 6px;
        width: 13px;
        height: 18px;
    }

    .tooltip {
        background-color: #20272b;
        border-width: 1px;
        border-color: #48545c;
        border-radius: 5px;
    }
"#;
