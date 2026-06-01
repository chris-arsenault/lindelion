DEFAULT_PLUGIN ?= lamath
PLUGINS ?= lamath glirdir linnod
ifeq ($(origin PLUGIN), undefined)
BUILD_PLUGINS ?= $(PLUGINS)
PLUGIN ?= $(DEFAULT_PLUGIN)
else
BUILD_PLUGINS ?= $(PLUGIN)
endif
MACOS_TARGET ?= aarch64-apple-darwin
# Windows-only new VSTs (ADR-0023): cross-built from Linux with cargo-xwin (MSVC ABI).
# Kept separate from the macOS PLUGINS list above.
WINDOWS_TARGET ?= x86_64-pc-windows-msvc
WINDOWS_PLUGINS ?= cenedril
XWIN_CACHE_DIR ?= $(HOME)/.cache/cargo-xwin
CACHE_DIR ?= $(HOME)/.lindelion-cache
LINDELION_CARGO_TARGET_DIR ?= $(CACHE_DIR)/target
# Release Windows builds use their OWN target dir so optimized artifacts never invalidate or
# compete with the day-to-day debug/dev caches (./target for make ci/test, and the dir above for
# the debug Windows build / macOS bundles).
LINDELION_RELEASE_TARGET_DIR ?= $(CACHE_DIR)/target-release
BUNDLE_NAME ?= $(shell CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" cargo run -q -p xtask -- plugin-info "$(PLUGIN)" --field bundle-file)
VST3_VALIDATOR ?= validator
VST3_STAGING_DIR ?= $(CACHE_DIR)/bundles
VST3_DIR ?= /Library/Audio/Plug-Ins/VST3/Ahara
VST3_STAGED_BUNDLE ?= $(VST3_STAGING_DIR)/$(BUNDLE_NAME)
VST3_INSTALLED_BUNDLE ?= $(VST3_DIR)/$(BUNDLE_NAME)

.PHONY: ci fmt fmt-check clippy test test-models test-integration check bench bench-smoke host-macos-check macos-check build build-windows windows-sdk-prep host-windows-check host-windows-release bundle-macos inspect-vst3 validate-vst3 cache-dir docs plugin-info

ci: check host-macos-check

cache-dir:
	@mkdir -p "$(CACHE_DIR)" "$(LINDELION_CARGO_TARGET_DIR)" "$(VST3_STAGING_DIR)"

check:
	cargo run -p xtask -- check

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --workspace --lib --bins --tests -- -D warnings -W clippy::cognitive_complexity

test:
	cargo test --workspace

# Heavy model-integration tests (ONNX Runtime inference). Excluded from `make ci` because they
# saturate the CPU; run them on their own, less frequently.
test-models:
	cargo test -p lindelion-speech-denoiser -p lindelion-speech-voice-gate --test integration -- --include-ignored
	cargo test -p lindelion-speech-bass-enhancer -p lindelion-speech-consonant-transient -p lindelion-speech-dynamic-eq -p lindelion-speech-upward-expander --test integration --features test-sync-analysis -- --include-ignored
	cargo test -p lindelion-speech-air-exciter -p lindelion-speech-dereverberation -p lindelion-speech-room-tone -p lindelion-speech-spectral-contrast --test integration -- --include-ignored

# Integration tests: heavy DSP regression (fidelity/stability/tuning sweeps) plus filesystem- and
# thread-touching tests. Gated behind the per-crate `integration-tests` feature, excluded from the
# default `make ci` unit run; run them here on their own.
test-integration:
	cargo test -p lamath --features integration-tests
	cargo test -p linnod --features integration-tests
	cargo test -p glirdir --features integration-tests
	cargo test -p lindelion-pitch-shift --features integration-tests
	cargo test -p lindelion-dsp-utils --features integration-tests
	cargo test -p lindelion-plugin-shell --features integration-tests
	cargo test -p lindelion-sample-library --features integration-tests
	cargo test -p galad --features integration-tests

bench:
	cargo bench --workspace --no-fail-fast

bench-smoke:
	cargo bench --workspace --no-run

docs:
	cargo test -p lindelion-dsp-utils --test plot_data -- --include-ignored
	cargo test -p lindelion-onset-detect --test plot_data -- --include-ignored
	cargo test -p lindelion-pitch-detect --test plot_data -- --include-ignored
	cargo test -p lamath export_modal_bank_impulse_csv -- --include-ignored
	cargo test -p lamath export_waveguide_impulse_csv -- --include-ignored
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

host-macos-check:
	@if [ "$$(uname -s)" = "Darwin" ]; then \
		$(MAKE) macos-check; \
	else \
		echo "Skipping macOS-target check on non-macOS host; Apple C toolchain is required."; \
	fi

macos-check:
	@if [ "$$(uname -s)" != "Darwin" ]; then \
		echo "macos-check must be run on macOS; Apple C toolchain is required."; \
		exit 2; \
	fi
	@rustup target list --installed | grep -qx "$(MACOS_TARGET)" || rustup target add "$(MACOS_TARGET)"
	@# `galad` (the Windows-only host) is excluded: it is verified by `make host-windows-check`
	@# (cargo-xwin), not the macOS workspace check (ADR-0022).
	RUSTFLAGS="$(RUSTFLAGS) -D warnings" cargo check --workspace --exclude galad --target "$(MACOS_TARGET)"

plugin-info:
	CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" cargo run -p xtask -- plugin-info "$(PLUGIN)"

build: cache-dir
	@if [ "$$(uname -s)" != "Darwin" ]; then \
		echo "make build creates a macOS VST3 bundle and must be run on macOS."; \
		exit 2; \
	fi
	@rustup target list --installed | grep -qx "$(MACOS_TARGET)" || rustup target add "$(MACOS_TARGET)"
	@for plugin in $(BUILD_PLUGINS); do \
		bundle_name="$$(CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" cargo run -q -p xtask -- plugin-info "$$plugin" --field bundle-file)"; \
		staged_bundle="$(VST3_STAGING_DIR)/$$bundle_name"; \
		installed_bundle="$(VST3_DIR)/$$bundle_name"; \
		echo "Building VST3 bundle for $$plugin..."; \
		CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" \
		CARGO_INCREMENTAL=1 \
		LINDELION_BUNDLE_DIR="$(VST3_STAGING_DIR)" \
		cargo run -p xtask -- bundle "$$plugin" --target "$(MACOS_TARGET)" || exit 1; \
		echo "Installing VST3 bundle to: $$installed_bundle"; \
		sudo mkdir -p "$(VST3_DIR)"; \
		sudo rm -rf "$$installed_bundle"; \
		sudo ditto "$$staged_bundle" "$$installed_bundle"; \
		sudo xattr -dr com.apple.quarantine "$$installed_bundle" 2>/dev/null || true; \
		echo "Published VST3 bundle in: $$installed_bundle"; \
	done
	@echo "Use Ableton's VST3 system folders; no custom folder is required."

# Cross-build the Windows-only new VSTs (Cenedril, ...) from Linux via cargo-xwin (ADR-0023).
# Produces the MSVC-ABI .vst3 bundle in the staging dir; load-verify it on Windows / in Galad.
build-windows: cache-dir
	@if ! cargo xwin --version >/dev/null 2>&1; then \
		echo "build-windows needs cargo-xwin. Install it with: cargo install cargo-xwin"; \
		exit 2; \
	fi
	@rustup target list --installed | grep -qx "$(WINDOWS_TARGET)" || rustup target add "$(WINDOWS_TARGET)"
	@# Case-fix: lld-link is case-sensitive on Linux but some build scripts (skia) link e.g.
	@# `Advapi32.lib` while the xwin SDK ships lowercase `advapi32.lib`. Symlink capitalized
	@# variants once the SDK is extracted. (On a fresh cache the SDK is downloaded during the
	@# first build, so that first run may fail at link; re-run `make build-windows` to succeed.)
	@for d in um ucrt; do \
		dir="$(XWIN_CACHE_DIR)/xwin/sdk/lib/$$d/x86_64"; \
		[ -d "$$dir" ] || continue; \
		for lib in "$$dir"/*.lib; do \
			base="$$(basename "$$lib")"; \
			cap="$$(printf '%s' "$$base" | sed -E 's/^(.)/\U\1/')"; \
			if [ "$$cap" != "$$base" ] && [ ! -e "$$dir/$$cap" ]; then ln -s "$$base" "$$dir/$$cap"; fi; \
		done; \
	done
	@for plugin in $(WINDOWS_PLUGINS); do \
		bundle_name="$$(CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" cargo run -q -p xtask -- plugin-info "$$plugin" --field bundle-file)"; \
		staged_bundle="$(VST3_STAGING_DIR)/$$bundle_name"; \
		echo "Building Windows VST3 bundle for $$plugin..."; \
		CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" \
		CARGO_INCREMENTAL=1 \
		XWIN_ACCEPT_LICENSE=1 \
		LINDELION_BUNDLE_DIR="$(VST3_STAGING_DIR)" \
		cargo run -p xtask -- bundle "$$plugin" --target "$(WINDOWS_TARGET)" || exit 1; \
		echo "Staged Windows VST3 bundle: $$staged_bundle"; \
	done
	@echo "Copy the staged .vst3 to a Windows host, or load it in the Galad host, to verify."

# Galad (the `galad/` Windows VST3 host) is excluded from `make ci` (ADR-0022); this is its
# verify command — cross-compile the `galad` binary for the MSVC ABI from Linux via cargo-xwin.
# Runtime verification (live audio, plugin hosting) happens on Windows in later milestones.
# Shared cargo-xwin + Windows SDK prep for the galad host builds. Idempotent; operates on the shared
# xwin SDK cache (not a target dir). The case-fix symlinks work around lld-link's case-sensitivity on
# Linux: the xwin SDK ships lowercase import libs (e.g. `kernel32.lib`) while some link directives use
# capitalized names. On a fresh cache the SDK downloads during the first build, so that first run may
# fail at link — just re-run.
windows-sdk-prep:
	@if ! cargo xwin --version >/dev/null 2>&1; then \
		echo "Windows builds need cargo-xwin. Install it with: cargo install cargo-xwin"; \
		exit 2; \
	fi
	@rustup target list --installed | grep -qx "$(WINDOWS_TARGET)" || rustup target add "$(WINDOWS_TARGET)"
	@for d in um ucrt; do \
		dir="$(XWIN_CACHE_DIR)/xwin/sdk/lib/$$d/x86_64"; \
		[ -d "$$dir" ] || continue; \
		for lib in "$$dir"/*.lib; do \
			base="$$(basename "$$lib")"; \
			cap="$$(printf '%s' "$$base" | sed -E 's/^(.)/\U\1/')"; \
			if [ "$$cap" != "$$base" ] && [ ! -e "$$dir/$$cap" ]; then ln -s "$$base" "$$dir/$$cap"; fi; \
		done; \
	done

# /FORCE:MULTIPLE resolves the skia<->windows ICU duplicate-symbol clash. Galad is the only target that
# links BOTH skia (Vizia's renderer, which statically bundles ICU and exports ubrk_*/ures_*/...) AND the
# `windows`/winit stack, whose monolithic `windows.0.52.0.lib` umbrella import lib drags in icu.dll import
# stubs for the same symbols. The clean fix (granular raw-dylib imports) is unavailable: `--cfg
# windows_raw_dylib` flips windows-sys 0.52's fn ABI to `extern "C"` and breaks glutin's DefWindowProcW,
# and the winit/glutin stack is pinned to windows-sys 0.52 inside the Vizia rev. /FORCE:MULTIPLE keeps the
# first-seen definition; only skia CALLS these ICU functions, and which static-vs-stub wins is confirmed
# on Windows hardware (galad's live behavior is always a Windows field-check, never a make-ci gate).
host-windows-check: cache-dir windows-sdk-prep
	CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" \
	CARGO_INCREMENTAL=1 \
	XWIN_ACCEPT_LICENSE=1 \
	RUSTFLAGS="$(RUSTFLAGS) -C link-arg=/FORCE:MULTIPLE" \
	cargo xwin build -p galad --target "$(WINDOWS_TARGET)"

# Release build of the galad host. Uses a SEPARATE target dir ($(LINDELION_RELEASE_TARGET_DIR)) so the
# optimized build never invalidates or competes with the debug/dev caches. Same skia/ICU /FORCE:MULTIPLE
# workaround. The release exe still links the MSVC CRT dynamically (needs the VC++ 2015-2022
# redistributable on the target); add `-C target-feature=+crt-static` to make it standalone if skia's
# prebuilt CRT linkage allows.
host-windows-release: cache-dir windows-sdk-prep
	@mkdir -p "$(LINDELION_RELEASE_TARGET_DIR)"
	CARGO_TARGET_DIR="$(LINDELION_RELEASE_TARGET_DIR)" \
	CARGO_INCREMENTAL=0 \
	XWIN_ACCEPT_LICENSE=1 \
	RUSTFLAGS="$(RUSTFLAGS) -C link-arg=/FORCE:MULTIPLE" \
	cargo xwin build -p galad --release --target "$(WINDOWS_TARGET)"
	@echo "Release galad.exe -> $(LINDELION_RELEASE_TARGET_DIR)/$(WINDOWS_TARGET)/release/galad.exe"
	@echo "Built galad for $(WINDOWS_TARGET). Run it on Windows to verify runtime behavior."

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

validate-vst3: inspect-vst3
	CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" \
	cargo run -p xtask -- validator "$(PLUGIN)" \
		--bundle "$(VST3_INSTALLED_BUNDLE)" \
		--validator "$(VST3_VALIDATOR)"
