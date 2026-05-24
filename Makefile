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

.PHONY: ci fmt fmt-check clippy test check bench bench-smoke macos-check build bundle-macos inspect-vst3 cache-dir

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
