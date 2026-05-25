PLUGIN ?= lamath
MACOS_TARGET ?= aarch64-apple-darwin
CACHE_DIR ?= $(HOME)/.lindelion-cache
LINDELION_CARGO_TARGET_DIR ?= $(CACHE_DIR)/target
ifeq ($(PLUGIN),glirdir)
DEFAULT_BUNDLE_NAME := Glirdir.vst3
else
DEFAULT_BUNDLE_NAME := Lamath.vst3
endif
BUNDLE_NAME ?= $(DEFAULT_BUNDLE_NAME)
VST3_STAGING_DIR ?= $(CACHE_DIR)/bundles
VST3_DIR ?= /Library/Audio/Plug-Ins/VST3/Ahara
VST3_STAGED_BUNDLE ?= $(VST3_STAGING_DIR)/$(BUNDLE_NAME)
VST3_INSTALLED_BUNDLE ?= $(VST3_DIR)/$(BUNDLE_NAME)

.PHONY: ci fmt fmt-check clippy test check bench bench-smoke macos-check build bundle-macos inspect-vst3 cache-dir docs

ci: check bench-smoke

cache-dir:
	@mkdir -p "$(CACHE_DIR)" "$(LINDELION_CARGO_TARGET_DIR)" "$(VST3_STAGING_DIR)"

check:
	cargo run -p xtask -- check

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --all-targets --release -- -D warnings -W clippy::cognitive_complexity

test:
	cargo test --workspace

bench:
	cargo bench --workspace --no-fail-fast

bench-smoke:
	cargo bench --workspace --no-run

docs:
	cargo test -p lindelion-dsp-utils --test plot_data
	cargo test -p lindelion-onset-detect --test plot_data
	cargo test -p lindelion-pitch-detect --test plot_data
	cargo test -p lamath export_modal_bank_impulse_csv
	cargo test -p lamath export_waveguide_impulse_csv
	@command -v python3 >/dev/null || { echo "python3 required for plot rendering. See tools/dsp-plot/README.md." >&2; exit 1; }
	@python3 -c "import matplotlib, scipy" 2>/dev/null || { echo "matplotlib + scipy required. pip install -r tools/dsp-plot/requirements.txt" >&2; exit 1; }
	@mkdir -p docs/plots
	python3 tools/dsp-plot/plot_freqz.py docs/plots/data/onepolelowpass_mag.csv docs/plots/onepolelowpass_mag.svg --title "OnePoleLowpass magnitude response (fs=48 kHz)"
	python3 tools/dsp-plot/plot_freqz.py docs/plots/data/onepolelowpass_phase.csv docs/plots/onepolelowpass_phase.svg --title "OnePoleLowpass phase response (fs=48 kHz)" --ylabel "Phase (degrees)"
	python3 tools/dsp-plot/plot_time.py docs/plots/data/onepolelowpass_impulse.csv docs/plots/onepolelowpass_impulse.svg --title "OnePoleLowpass impulse response (fs=48 kHz)"
	python3 tools/dsp-plot/plot_freqz.py docs/plots/data/biquad_mag.csv docs/plots/biquad_mag.svg --title "Biquad magnitude response (fc=1 kHz, Q=0.707)"
	python3 tools/dsp-plot/plot_freqz.py docs/plots/data/biquad_phase.csv docs/plots/biquad_phase.svg --title "Biquad phase response (fc=1 kHz, Q=0.707)" --ylabel "Phase (degrees)"
	python3 tools/dsp-plot/plot_pz.py docs/plots/data/biquad_ba.csv docs/plots/biquad_pz.svg --title "Biquad pole-zero (fc=1 kHz, Q=0.707)"
	python3 tools/dsp-plot/plot_time.py docs/plots/data/biquad_impulse.csv docs/plots/biquad_impulse.svg --title "Biquad impulse response (fc=1 kHz, Q=0.707)"
	python3 tools/dsp-plot/plot_freqz.py docs/plots/data/svf_mag.csv docs/plots/svf_mag.svg --title "Svf magnitude response (fc=1 kHz, R=0.3)"
	python3 tools/dsp-plot/plot_freqz.py docs/plots/data/svf_phase.csv docs/plots/svf_phase.svg --title "Svf phase response (fc=1 kHz, R=0.3)" --ylabel "Phase (degrees)"
	python3 tools/dsp-plot/plot_time.py docs/plots/data/svf_impulse.csv docs/plots/svf_impulse.svg --title "Svf impulse response (fc=1 kHz, R=0.3)"
	python3 tools/dsp-plot/plot_time.py docs/plots/data/delay_impulse.csv docs/plots/delay_impulse.svg --title "DelayLine impulse response"
	python3 tools/dsp-plot/plot_freqz.py docs/plots/data/allpass_mag.csv docs/plots/allpass_mag.svg --title "FirstOrderAllpass magnitude response"
	python3 tools/dsp-plot/plot_freqz.py docs/plots/data/allpass_phase.csv docs/plots/allpass_phase.svg --title "FirstOrderAllpass phase response" --ylabel "Phase (degrees)"
	python3 tools/dsp-plot/plot_time.py docs/plots/data/allpass_impulse.csv docs/plots/allpass_impulse.svg --title "FirstOrderAllpass impulse response"
	python3 tools/dsp-plot/plot_time.py docs/plots/data/smoothing_step.csv docs/plots/smoothing_step.svg --title "LinearSmoother step response (target = 1.0, fs = 48 kHz)"
	python3 tools/dsp-plot/plot_time.py docs/plots/data/adsr_step.csv docs/plots/adsr_step.svg --title "ADSR step response (A=20 ms, D=100 ms, S=0.5, R=200 ms)"
	python3 tools/dsp-plot/plot_time.py docs/plots/data/modal_impulse.csv docs/plots/modal_impulse.svg --title "ModalBank impulse response (Marimba, 32 modes, 220 Hz)"
	python3 tools/dsp-plot/plot_time.py docs/plots/data/waveguide_impulse.csv docs/plots/waveguide_impulse.svg --title "WaveguideResonator impulse response (String, 240 Hz, gain=0.95)"
	python3 tools/dsp-plot/plot_markers.py docs/plots/data/onset_signal.csv docs/plots/data/onset_markers.csv docs/plots/onset_detection.svg --title "EnergyTransientDetector on synthetic tone bursts (sensitivity=0.7)"
	python3 tools/dsp-plot/plot_time.py docs/plots/data/pitch_tracking.csv docs/plots/pitch_tracking.svg --title "SwiftF0Detector tracking synthetic frequency sweep" --ylabel "Frequency (Hz)"

macos-check:
	cargo check -p lamath --target aarch64-apple-darwin

build: cache-dir
	@if [ "$$(uname -s)" != "Darwin" ]; then \
		echo "make build creates a macOS VST3 bundle and must be run on macOS."; \
		exit 2; \
	fi
	@rustup target list --installed | grep -qx "$(MACOS_TARGET)" || rustup target add "$(MACOS_TARGET)"
	CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" \
	CARGO_INCREMENTAL=1 \
	LINDELION_BUNDLE_DIR="$(VST3_STAGING_DIR)" \
	cargo run -p xtask -- bundle "$(PLUGIN)" --target "$(MACOS_TARGET)"
	@echo "Installing VST3 bundle to: $(VST3_INSTALLED_BUNDLE)"
	@sudo mkdir -p "$(VST3_DIR)"
	@sudo rm -rf "$(VST3_INSTALLED_BUNDLE)"
	@sudo ditto "$(VST3_STAGED_BUNDLE)" "$(VST3_INSTALLED_BUNDLE)"
	@sudo xattr -dr com.apple.quarantine "$(VST3_INSTALLED_BUNDLE)" 2>/dev/null || true
	@echo "Published VST3 bundle in: $(VST3_INSTALLED_BUNDLE)"
	@echo "Use Ableton's VST3 system folders; no custom folder is required."

bundle-macos: build

inspect-vst3:
	@if [ "$$(uname -s)" != "Darwin" ]; then \
		echo "inspect-vst3 must be run on macOS."; \
		exit 2; \
	fi
	@echo "Bundle: $(VST3_INSTALLED_BUNDLE)"
	@echo "Executable:"
	@/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "$(VST3_INSTALLED_BUNDLE)/Contents/Info.plist"
	@echo "Mach-O:"
	@file "$(VST3_INSTALLED_BUNDLE)/Contents/MacOS/$$(/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "$(VST3_INSTALLED_BUNDLE)/Contents/Info.plist")"
	@echo "Exports:"
	@nm -gU "$(VST3_INSTALLED_BUNDLE)/Contents/MacOS/$$(/usr/libexec/PlistBuddy -c "Print :CFBundleExecutable" "$(VST3_INSTALLED_BUNDLE)/Contents/Info.plist")" | egrep 'GetPluginFactory|bundleEntry|bundleExit|BundleEntry|BundleExit'
	@echo "Code signature:"
	@codesign --verify --deep --strict --verbose=4 "$(VST3_INSTALLED_BUNDLE)"
