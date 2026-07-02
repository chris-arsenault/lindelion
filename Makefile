DEFAULT_PLUGIN ?= lamath
PLUGINS ?= lamath lamath-cymbal lamath-tube lamath-stringed glirdir linnod
MACOS_TARGET ?= aarch64-apple-darwin
# Windows-only new VSTs (ADR-0023): cross-built from Linux with cargo-xwin (MSVC ABI).
# Kept separate from the macOS PLUGINS list above.
WINDOWS_TARGET ?= x86_64-pc-windows-msvc
WINDOWS_PLUGINS ?= cenedril caloma lumedir lamath lamath-cymbal lamath-tube lamath-stringed
# `make build [PLUGIN=x]` and `make build-windows [PLUGIN=x]` build a single plugin when PLUGIN is
# set on the command line, else the full macOS / Windows plugin lists respectively.
ifeq ($(origin PLUGIN), undefined)
BUILD_PLUGINS ?= $(PLUGINS)
WINDOWS_BUILD_PLUGINS ?= $(WINDOWS_PLUGINS)
PLUGIN ?= $(DEFAULT_PLUGIN)
else
BUILD_PLUGINS ?= $(PLUGIN)
WINDOWS_BUILD_PLUGINS ?= $(PLUGIN)
endif
XWIN_CACHE_DIR ?= $(HOME)/.cache/cargo-xwin
# Repo root = the directory containing this Makefile. Robust to the invocation cwd (unlike $(CURDIR))
# and unique per git worktree, so all build output is repo-local and worktrees never share a cache.
REPO_ROOT := $(patsubst %/,%,$(dir $(abspath $(firstword $(MAKEFILE_LIST)))))
LAMATH_REVIEW_DIR ?= $(REPO_ROOT)/review/lamath-render-catalog
LAMATH_RENDER_ARGS ?= --all
REVIEW_AUDIO_SOURCE_DIR ?= $(LAMATH_REVIEW_DIR)
REVIEW_AUDIO_PREVIEW_DIR ?= $(REPO_ROOT)/review/audio-previews/lamath-render-catalog
REVIEW_AUDIO_ENCODER ?= ffmpeg
REVIEW_AUDIO_BITRATE ?= 192k
REVIEW_AUDIO_ARGS ?=

# All build output lives in the repo (gitignored), in dirs kept separate from ./target (the cargo
# default used by `make ci`/tests) so release/iteration cache invalidation never crosses into it:
#   ./target          dev / CI (cargo default)
#   ./target-build    iteration builds (build / build-windows / host-windows-check) + staged dev bundles
#   ./target-release  the release chain (make release)
LINDELION_CARGO_TARGET_DIR ?= $(REPO_ROOT)/target-build
LINDELION_RELEASE_TARGET_DIR ?= $(REPO_ROOT)/target-release
VST3_STAGING_DIR ?= $(LINDELION_CARGO_TARGET_DIR)/bundles
BUNDLE_NAME ?= $(shell CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" cargo run -q -p xtask -- plugin-info "$(PLUGIN)" --field bundle-file)
VST3_VALIDATOR ?= validator
VST3_DIR ?= /Library/Audio/Plug-Ins/VST3/Ahara
VST3_STAGED_BUNDLE ?= $(VST3_STAGING_DIR)/$(BUNDLE_NAME)
VST3_INSTALLED_BUNDLE ?= $(VST3_DIR)/$(BUNDLE_NAME)

.PHONY: ci fmt fmt-check clippy test test-models tune-defaults render-lamath-audio compress-review-audio test-integration check bench bench-smoke host-macos-check macos-check build build-windows windows-sdk-prep host-windows-check host-windows-release release release-windows release-macos bundle-macos inspect-vst3 validate-vst3 cache-dir docs plugin-info

ci: check host-macos-check

cache-dir:
	@mkdir -p "$(LINDELION_CARGO_TARGET_DIR)" "$(LINDELION_RELEASE_TARGET_DIR)" "$(VST3_STAGING_DIR)"

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
	cargo test -p lindelion-speech-bass-enhancer -p lindelion-speech-consonant-transient -p lindelion-speech-dynamic-eq -p lindelion-speech-upward-expander --test integration -- --include-ignored
	cargo test -p lindelion-speech-air-exciter -p lindelion-speech-dereverberation -p lindelion-speech-room-tone -p lindelion-speech-spectral-contrast --test integration -- --include-ignored
	cargo test -p caloma --test chain --test chain_e2e --test full_chain_fidelity -- --include-ignored
	cargo test -p lumedir --test dynamism_fixtures -- --include-ignored
	cargo test -p lumedir --test delivery_fixtures -- --include-ignored

# Offline default-tuning (M5): run the seeded full-chain search per signal order and write the
# winning patches into plugins/caloma/src/defaults/<order>.toml. Heavy (loads the NN models, runs the
# chain many times); deterministic, so the written defaults are reviewable in the diff. Run when an
# effect change or a retune is wanted; the result is committed. Built in **release** — the ~16-slot
# chain's DSP is far too slow unoptimized for a many-candidate search (this is an offline generator,
# not the debug-only `make ci` path).
tune-defaults:
	cargo test --release -p caloma --test tune_defaults -- --include-ignored --nocapture

# Audition render. Like tune-defaults this is an offline generator whose DSP (the dense mesh
# especially) is far too slow unoptimized, so it builds in **release**, routed to the separate
# LINDELION_RELEASE_TARGET_DIR so it never pollutes the debug dev/CI cache (./target) or clashes
# with concurrent debug builds. Select cases with LAMATH_RENDER_ARGS, e.g.
# `--case <id>`, `--tag mesh` (all Mesh cases), `--group <id>`, or `--all` (default).
render-lamath-audio:
	CARGO_TARGET_DIR="$(LINDELION_RELEASE_TARGET_DIR)" \
		cargo run -q --release -p lamath --bin lamath-render-catalog -- \
		--out "$(LAMATH_REVIEW_DIR)" $(LAMATH_RENDER_ARGS)

# Symphony render: the multi-movement piece for the four Lamath instrument
# families. Each movement renders per-track stems plus its own mix and MIDI
# under review/lamath-song/<movement>/; the combined master lands at
# review/lamath-song/lamath-symphony.wav. Pass MOVEMENT=<slug> to render one
# movement while iterating (skips the combined master).
# Release build routed to LINDELION_RELEASE_TARGET_DIR like render-lamath-audio.
render-lamath-song:
	CARGO_TARGET_DIR="$(LINDELION_RELEASE_TARGET_DIR)" \
		cargo run -q --release -p lamath --bin lamath-song -- \
		--out "$(REPO_ROOT)/review/lamath-song" $(if $(MOVEMENT),--movement $(MOVEMENT),)

compress-review-audio:
	cargo run -p xtask -- compress-review-audio --source "$(REVIEW_AUDIO_SOURCE_DIR)" --out "$(REVIEW_AUDIO_PREVIEW_DIR)" --encoder "$(REVIEW_AUDIO_ENCODER)" --bitrate "$(REVIEW_AUDIO_BITRATE)" $(REVIEW_AUDIO_ARGS)

# Integration tests: heavy DSP regression (fidelity/stability/tuning sweeps) plus filesystem- and
# thread-touching tests. Gated behind the per-crate `integration-tests` feature, excluded from the
# default `make ci` unit run; run them here on their own.
test-integration:
	cargo test -p lamath --features integration-tests
	cargo test -p lindelion-idiophone --features integration-tests
	cargo test -p lindelion-wind --features integration-tests
	cargo test -p lindelion-string --features integration-tests
	cargo test -p lamath-cymbal --features integration-tests
	cargo test -p lamath-tube --features integration-tests
	cargo test -p lamath-stringed --features integration-tests
	cargo test -p linnod --features integration-tests
	cargo test -p glirdir --features integration-tests
	cargo test -p cenedril --features integration-tests
	cargo test -p lindelion-pitch-shift --features integration-tests
	cargo test -p lindelion-dsp-utils --features integration-tests
	cargo test -p lindelion-plugin-shell --features integration-tests
	cargo test -p lindelion-sample-library --features integration-tests
	cargo test -p galad --features integration-tests
	cargo test -p lumedir --test integration --features test-sync-analysis -- --include-ignored
	cargo test -p lumedir --features integration-tests

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
	@# The mixed-case `DirectML.lib`/`PathCch.lib` (Caloma's ONNX Runtime DirectML EP + skia) cannot
	@# be derived by capitalizing the first letter, so they are symlinked explicitly.
	@for d in um ucrt; do \
		dir="$(XWIN_CACHE_DIR)/xwin/sdk/lib/$$d/x86_64"; \
		[ -d "$$dir" ] || continue; \
		for lib in "$$dir"/*.lib; do \
			base="$$(basename "$$lib")"; \
			cap="$$(printf '%s' "$$base" | sed -E 's/^(.)/\U\1/')"; \
			if [ "$$cap" != "$$base" ] && [ ! -e "$$dir/$$cap" ]; then ln -s "$$base" "$$dir/$$cap"; fi; \
		done; \
		for mixed in DirectML:directml PathCch:pathcch; do \
			want="$${mixed%%:*}.lib"; have="$${mixed##*:}.lib"; \
			if [ -e "$$dir/$$have" ] && [ ! -e "$$dir/$$want" ]; then ln -s "$$have" "$$dir/$$want"; fi; \
		done; \
	done
	@for plugin in $(WINDOWS_BUILD_PLUGINS); do \
		bundle_name="$$(CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" cargo run -q -p xtask -- plugin-info "$$plugin" --field bundle-file)"; \
		staged_bundle="$(VST3_STAGING_DIR)/$$bundle_name"; \
		echo "Building Windows VST3 bundle for $$plugin..."; \
		CARGO_TARGET_DIR="$(LINDELION_CARGO_TARGET_DIR)" \
		CARGO_INCREMENTAL=1 \
		XWIN_ACCEPT_LICENSE=1 \
		CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="$(CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS) -C link-arg=/FORCE:MULTIPLE" \
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

# Full release chain: every distributable artifact, built --release into the in-repo, gitignored
# $(LINDELION_RELEASE_TARGET_DIR) (= ./target-release), so release cache invalidation never touches
# the dev/CI cache (./target) or the iteration cache ($(LINDELION_CARGO_TARGET_DIR)). `make release`
# dispatches by host OS.
release:
	@if [ "$$(uname -s)" = "Darwin" ]; then $(MAKE) release-macos; else $(MAKE) release-windows; fi

# Windows release: galad.exe (via host-windows-release) + the Windows VST3 plugin bundles
# ($(WINDOWS_PLUGINS)), cross-compiled into target-release.
release-windows: host-windows-release
	@for plugin in $(WINDOWS_PLUGINS); do \
		bundle_name="$$(CARGO_TARGET_DIR="$(LINDELION_RELEASE_TARGET_DIR)" cargo run -q -p xtask -- plugin-info "$$plugin" --field bundle-file)"; \
		echo "Building Windows release VST3 bundle for $$plugin..."; \
		CARGO_TARGET_DIR="$(LINDELION_RELEASE_TARGET_DIR)" \
		CARGO_INCREMENTAL=0 \
		XWIN_ACCEPT_LICENSE=1 \
		CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS="$(CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS) -C link-arg=/FORCE:MULTIPLE" \
		LINDELION_BUNDLE_DIR="$(LINDELION_RELEASE_TARGET_DIR)/bundles" \
		cargo run -p xtask -- bundle "$$plugin" --target "$(WINDOWS_TARGET)" || exit 1; \
		echo "Staged Windows release VST3 bundle: $(LINDELION_RELEASE_TARGET_DIR)/bundles/$$bundle_name"; \
	done
	@echo "Windows release artifacts:"
	@echo "  galad.exe -> $(LINDELION_RELEASE_TARGET_DIR)/$(WINDOWS_TARGET)/release/galad.exe"
	@echo "  .vst3     -> $(LINDELION_RELEASE_TARGET_DIR)/bundles/"

# macOS release: the instrument VST3 bundles (lamath/glirdir/linnod) built --release into
# target-release and staged (not installed). Must run on macOS.
release-macos:
	@if [ "$$(uname -s)" != "Darwin" ]; then \
		echo "release-macos builds macOS VST3 bundles and must be run on macOS."; \
		exit 2; \
	fi
	@rustup target list --installed | grep -qx "$(MACOS_TARGET)" || rustup target add "$(MACOS_TARGET)"
	@for plugin in $(PLUGINS); do \
		bundle_name="$$(CARGO_TARGET_DIR="$(LINDELION_RELEASE_TARGET_DIR)" cargo run -q -p xtask -- plugin-info "$$plugin" --field bundle-file)"; \
		echo "Building macOS release VST3 bundle for $$plugin..."; \
		CARGO_TARGET_DIR="$(LINDELION_RELEASE_TARGET_DIR)" \
		CARGO_INCREMENTAL=0 \
		LINDELION_BUNDLE_DIR="$(LINDELION_RELEASE_TARGET_DIR)/bundles" \
		cargo run -p xtask -- bundle "$$plugin" --target "$(MACOS_TARGET)" || exit 1; \
		echo "Staged macOS release VST3 bundle: $(LINDELION_RELEASE_TARGET_DIR)/bundles/$$bundle_name"; \
	done
	@echo "macOS release bundles staged in $(LINDELION_RELEASE_TARGET_DIR)/bundles (install separately if desired)."

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
