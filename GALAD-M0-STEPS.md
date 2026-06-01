# Galad M0 — Step Plan

Expansion of **M0** from [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) into execution-ready, red→green
steps. Scope: stand up the target-gated **`host/`** binary crate, keep it out of the Linux/macOS
`make ci` path, wire a Windows verify command, and define the **serializable host session model**
with a round-trip test. **No audio** (M2), **no VST3 host protocol** (M1), **no disk
persistence/restore** (M4) — M0's model round-trips *in memory* (serialize → deserialize), not to a
file.

**Source-of-truth & precedents:**
- [`GALAD-HOST-PLAN.md`](GALAD-HOST-PLAN.md) "Context / reuse map" + "Cross-cutting constraints".
- [ADR-0022](docs/adr/0022-windows-vst3-host.md) — Windows-only, target-gated, excluded from
  `make ci`; "`cfg(windows)` / a dedicated workspace exclusion".
- [ADR-0001](docs/adr/0001-allocation-free-audio-thread.md) — applies to the audio callback (M2+),
  **not** to M0's control-thread session model.
- **Serialization precedent:** `crates/lindelion-plugin-shell/src/patch_io.rs` — the versioned
  `format_version` envelope + round-trip test pattern; and `PluginState` in
  `crates/lindelion-plugin-shell/src/state.rs` (`{ format_version: u32, payload: Vec<u8> }`) — the
  durable shape for an **opaque per-plugin state blob**. M0 mirrors these *host-locally*; it does
  **not** depend on the guest `lindelion-plugin-shell` crate (host is the opposite side of the
  protocol).
- **Build-path precedent:** [`CENEDRIL-M0-STEPS.md`](CENEDRIL-M0-STEPS.md) Steps 6–7 — the
  **cargo-xwin** / `x86_64-pc-windows-msvc` cross-build-from-Linux toolchain Galad reuses for its
  Windows verify command. Unlike Cenedril (a `cdylib` that *does* build in Linux `make ci`), Galad
  is **excluded** from `make ci` per ADR-0022.

---

## Decisions in this phase

Three `[DECISION]` stops remain; each carries a recommendation so confirmation is quick:

1. **Step 1 — name + crate identity + workspace mechanism.** Recommend: product **Galad**; directory
   stays **`host/`** (fixed by ADR-0022), Cargo **package `galad`**, **bin `galad`**; register `host`
   as a **workspace member** and exclude it from the `make ci` commands via **`--exclude galad`**
   (keeps `vst3.workspace`/lockfile/`target/` shared for M1, vs. a `[workspace] exclude` that would
   split host into its own lockfile).
2. **Step 2 — Windows verify command + CI runner.** Recommend: verify via **cargo-xwin cross-build**
   from Linux (`cargo xwin build -p galad --target x86_64-pc-windows-msvc`) plus `cargo test -p galad`
   for the portable model; **defer** a dedicated Windows CI runner.
3. **Step 3 — session file format + opaque-blob encoding.** Recommend: **host-local versioned TOML**
   (mirror `patch_io.rs`), opaque per-plugin state encoded as a **base64 string** (add the small
   `base64` crate as a host-only dep); alternative is a `Vec<u8>` integer array (no dep, verbose).

---

## Step 1 — Create the `host/` binary crate and register it as a workspace member  [DECISION]
- **File(s):** `Cargo.toml` (workspace `members`), `host/Cargo.toml` (new), `host/src/main.rs` (new),
  `host/src/session.rs` (new, empty `mod` stub filled in Step 3); update `host/README.md` (flip "not
  yet a workspace member" → registered), the **Code Map** row in `AGENTS.md` (drop "(Reserved, not
  yet a workspace member)"), and the `Galad` **Product Names** row in `AGENTS.md` if its wording
  implies "not yet a member". These doc edits are the factual consequence of the crate becoming
  real — keep wording, change only the now-false "reserved/not a member" claims.
- **Reference behavior:** A **binary** crate (the plan: "a new in-workspace `host/` binary crate"),
  not a `cdylib` like the plugins. `[package] name = "galad"` with `version/edition/license/authors`
  `.workspace = true` (mirror `plugins/cenedril/Cargo.toml`'s package stanza). The **directory is
  `host/`** (ADR-0022); package name `galad` ≠ dir name is allowed by Cargo. Default bin → `galad`.
  M0 dependencies are **portable only** — `serde` (`{ workspace = true, features = ["derive"]}` is
  already the workspace default) and `toml` (`.workspace = true`); **no** `lindelion-plugin-shell`,
  **no** WASAPI/`egui`/`vst3` yet (those enter M1/M2/M6 as `[target.'cfg(windows)'.dependencies]`).
  `main.rs` is a minimal portable stub (`fn main()` printing the host banner) — the real
  WASAPI/`egui` app entry is `#[cfg(windows)]` in later milestones; M0 has nothing Windows-specific
  to gate yet. `main.rs` declares `mod session;`.
- **Change:** add `"host"` to `Cargo.toml` `members`; create `host/Cargo.toml`, `host/src/main.rs`
  (banner + `mod session;` + a `#[test] fn crate_builds() { assert!(true) }` placeholder Step 3
  replaces), and an empty `host/src/session.rs`; make the README/Code-Map factual edits.
- **Verify:** `cargo test -p galad`. **Red:** before the member + package exist, cargo errors *"no
  such package: galad"* (greenfield — the crate doesn't resolve). **Green:** the placeholder test
  compiles and passes. Then `cargo metadata --format-version 1` lists `galad` as a workspace member.
- **[DECISION]:** confirm **Galad** as the product name and **`galad`** as the package/bin name
  (dir `host/`); confirm the **member + `--exclude galad`** mechanism (Step 2) over `[workspace]
  exclude = ["host"]`. The executor stops here for confirmation before wiring Step 2.

## Step 2 — Exclude `host/` from `make ci`; wire the Windows verify command  [depends on #1] [DECISION]
- **File(s):** `xtask/src/main.rs` (the `run_ci` clippy + test arg lists), `xtask/src/tests.rs` (new
  assertion), `Makefile` (the `macos-check` `cargo check --workspace`, and a new
  `host-windows-check` target), `host/README.md` (record the verify command).
- **Reference behavior:** `make ci` runs `cargo run -p xtask -- check` → `run_ci`
  (`xtask/src/main.rs:49`), which calls `cargo clippy --workspace …` (`main.rs:55-73`) and
  `cargo test --workspace` (`main.rs:104-106`); on macOS, `make ci` also runs `macos-check`'s
  `cargo check --workspace --target $(MACOS_TARGET)` (`Makefile`). All three are `--workspace`, so a
  new member is pulled in on Linux *and* macOS. Per ADR-0022 the host must be **excluded** from every
  `make ci` build. Add **`--exclude galad`** to: (a) the clippy arg list, (b) the test arg list, and
  (c) the Makefile `macos-check` `cargo check --workspace`. The **Windows verify command** mirrors
  [`CENEDRIL-M0-STEPS.md`](CENEDRIL-M0-STEPS.md) Step 7's cargo-xwin path: a `host-windows-check`
  Makefile target running `cargo xwin build -p galad --target x86_64-pc-windows-msvc` (the
  ship-correct MSVC ABI, cross-built from Linux), with the same cargo-xwin/`rustup target` prereq
  checks `build-windows` uses; the portable session model is additionally exercised by
  `cargo test -p galad`.
- **Change:** lift the clippy and test arg arrays in `run_ci` to named `const`s (small,
  behavior-preserving refactor so they're testable) and append `"--exclude", "galad"` to each; add
  `--exclude galad` to `macos-check`; add the `host-windows-check` target + prereq checks +
  `.PHONY`; note the command in `host/README.md`.
- **Verify:** in `xtask/src/tests.rs`, assert the lifted clippy- and test-arg consts each contain
  `"--exclude"` immediately followed by `"galad"`. **Red:** before the args are added the assertion
  fails (the slice lacks the pair). **Green:** passes. Behavioral exit: `cargo clippy --workspace
  --exclude galad` and `cargo test --workspace --exclude galad` succeed on Linux (host untouched by
  `make ci`), and `make host-windows-check` cross-compiles `galad` for `x86_64-pc-windows-msvc` via
  cargo-xwin in this environment.
- **[DECISION]:** confirm the Windows verify command above and that a **dedicated Windows CI runner
  is deferred** (cargo-xwin cross-build + portable `cargo test -p galad` give coverage here now);
  add a real Windows runner later if/when runtime (not just compile) checks are needed.

## Step 3 — Define the serializable host session model + versioned round-trip  [depends on #1] [DECISION]
- **File(s):** `host/src/session.rs`; `host/Cargo.toml` (add the opaque-blob encoding dep if the
  base64 option is chosen); `host/src/main.rs` (replace the Step 1 placeholder test / re-export
  `session` types as needed).
- **Reference behavior:** Model exactly the plan's M0 session shape — "selected input/output
  devices, the ordered plugin-chain (slots with path + per-slot bypass + opaque per-plugin state
  blob), and app settings — serializable":
  - `HostSession { input: Option<DeviceRef>, output: Option<DeviceRef>, chain: Vec<ChainSlot>,
    settings: AppSettings }`.
  - `DeviceRef { id: String, name: String }` — a **stable serializable identifier** (WASAPI endpoint
    id) plus a human-readable name; resolved to a live device only at M2, so M0 stores strings, not
    handles.
  - `ChainSlot { plugin_path: PathBuf, bypassed: bool, state: Option<PluginStateBlob> }`.
  - `PluginStateBlob { format_version: u32, payload: Vec<u8> }` — **mirror `PluginState`**
    (`crates/lindelion-plugin-shell/src/state.rs`); this is the opaque blob `IComponent::getState`
    fills at M4, so the durable shape is fixed now even though M0 never populates it from a plugin.
  - `AppSettings` — minimal and forward-compatible: `#[derive(Default)]` + `#[serde(default)]` on
    fields so older/newer files load; include only what M0 clearly needs (e.g.
    `plugin_scan_dirs: Vec<PathBuf>`, referenced by M6's scan-folder UI). Do not invent further
    fields — later milestones extend it.
  All types derive `Serialize, Deserialize, Debug, Clone, PartialEq`. Serialization follows
  `patch_io.rs`'s **versioned envelope**: a host-local `SESSION_FORMAT_VERSION: u32` and a
  `{ format_version, session }` envelope, with `to_toml_string`/`from_toml_str`-style helpers
  (mirror `TomlPatchFormat::to_toml_string`/`from_toml_str`, host-local — **no** dependency on the
  guest `lindelion-plugin-shell`). The opaque `payload: Vec<u8>` is encoded per the decision below.
- **Change:** write the structs + the versioned (de)serialize helpers in `session.rs`; if base64 is
  chosen, add `base64 = "0.22"` as a **host-only** dependency (not a workspace dep) and a
  `#[serde(with = …)]` base64 codec for `payload`.
- **Verify:** unit test in `session.rs`: construct a `HostSession` with **both** devices `Some(...)`,
  **two** chain slots (one `bypassed: true`, one carrying a non-empty `PluginStateBlob` payload), and
  non-default `AppSettings`; serialize to a string and deserialize back; assert the result `==` the
  original (the **opaque payload bytes must match exactly** — the durable-shape guarantee) and assert
  the serialized string contains `format_version`. **Red:** the `session` types/helpers don't exist
  (greenfield — fails to resolve/compile). **Green:** the round-trip equality + version assertions
  pass. (`cargo test -p galad`.)
- **[DECISION]:** confirm **versioned TOML** (mirroring `patch_io.rs`) as the session format and
  **base64-string** encoding for the opaque blob (adds the `base64` crate). Alternative: a `Vec<u8>`
  integer array (no new dep, verbose on disk). This sets the durable on-disk shape M4 persists.

---

## Decisions table
| Step | Decision | Recommendation |
| ---- | -------- | -------------- |
| 1 | Product name + crate identity + workspace-gating mechanism. | **Galad**; dir `host/`, package/bin `galad`; **member + `--exclude galad`** (shared lockfile/`vst3.workspace`/`target/`). |
| 2 | Windows verify command + dedicated CI runner now/later. | **cargo-xwin** `x86_64-pc-windows-msvc` cross-build + `cargo test -p galad`; **defer** a Windows CI runner. |
| 3 | Session file format + opaque-blob encoding. | **Versioned TOML** (mirror `patch_io.rs`) + **base64** blob (`base64` crate); alt: `Vec<u8>` array. |

## Handoff
Run the EXECUTE-PHASE companion prompt on this step list. Three `[DECISION]` stops remain (Steps
1–3), each with a recommendation — the executor pauses at each for your confirmation. Steps 1 and 3
are verified by `cargo test -p galad` (host excluded from `make ci`); Step 2's exclusion is verified
by the `xtask/src/tests.rs` arg assertion + `make ci` staying green on Linux, and its Windows half by
`make host-windows-check` (cargo-xwin) in this environment. M0 carries **no** audio, VST3-host, or
disk-persistence work — those are M2, M1, and M4. M1 (the host-side VST3 spike) is the load-bearing
risk; expand and run it next.
