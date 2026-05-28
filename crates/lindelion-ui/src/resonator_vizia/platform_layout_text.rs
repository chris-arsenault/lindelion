fn patch_detail_text(signals: EditorSignals) -> Memo<String> {
    Memo::new(move |_| {
        let command = signals
            .command_status
            .get()
            .map(|command| command_label(Some(command)))
            .unwrap_or("Ready");
        format!("{} / {command}", signals.patch_name.get())
    })
}

fn voices_text(active_voices: Signal<f32>) -> Memo<String> {
    Memo::new(move |_| format!("{:.0}", active_voices.get().max(0.0)))
}

fn pitch_confidence_text(confidence: Signal<f32>) -> Memo<String> {
    Memo::new(move |_| format!("{:.0}%", confidence.get().clamp(0.0, 1.0) * 100.0))
}

fn sidechain_text(label: &'static str, signal: Signal<bool>) -> Memo<String> {
    Memo::new(move |_| format!("{label} {}", if signal.get() { "on" } else { "off" }))
}

fn selected_layer_text(signals: EditorSignals) -> Memo<String> {
    Memo::new(move |_| format!("Layer {}", selected_layer_index(signals) + 1))
}

fn library_browser_title(signals: EditorSignals) -> Memo<String> {
    Memo::new(move |_| {
        let sample_count = signals.library_samples.get().len();
        if sample_count == 0 {
            return "Library browser / 0 samples".to_string();
        }
        let start = clamped_library_page_start(signals.library_page_start.get(), sample_count);
        let end = (start + LIBRARY_BROWSER_ROWS).min(sample_count);
        format!("Library browser / {}-{} of {}", start + 1, end, sample_count)
    })
}

fn selected_layer_index(signals: EditorSignals) -> usize {
    let selected = signals.selected_slot.get();
    if selected.is_finite() {
        selected.round().clamp(0.0, 3.0) as usize
    } else {
        0
    }
}

fn layer_label(index: usize) -> String {
    format!("Layer {}", index + 1)
}

fn layer_detail_text(
    summaries: Signal<[ResonatorEditorSlotSummary; 4]>,
    index: usize,
) -> Memo<String> {
    Memo::new(move |_| summaries.get()[index].detail.clone())
}

fn sample_label_text(signals: EditorSignals, row: usize) -> Memo<String> {
    Memo::new(move |_| {
        signals
            .library_samples
            .get()
            .get(signals.library_page_start.get().saturating_add(row))
            .map(|sample| sample.label.clone())
            .unwrap_or_else(|| "No library sample".to_string())
    })
}

fn sample_detail_text(signals: EditorSignals, row: usize) -> Memo<String> {
    Memo::new(move |_| {
        signals
            .library_samples
            .get()
            .get(signals.library_page_start.get().saturating_add(row))
            .map(|sample| sample.detail.clone())
            .unwrap_or_else(|| "Empty library".to_string())
    })
}
