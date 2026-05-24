#!/usr/bin/env bash
# new-dsp-doc.sh — scaffold a new DSP-module doc from the skeleton.
#
# Usage: scripts/new-dsp-doc.sh <module-slug> [output-dir]
#   <module-slug>  kebab-case identifier, e.g. "onepolelowpass" or "modal-bank"
#   [output-dir]   destination directory; defaults to docs/dsp/
#
# Emits docs/dsp/<module-slug>.md with the nine-section skeleton stubbed,
# the current date, and a TODO marker in each section.

set -euo pipefail

if [[ $# -lt 1 ]]; then
    echo "usage: $0 <module-slug> [output-dir]" >&2
    exit 64
fi

slug="$1"
outdir="${2:-docs/dsp}"
mkdir -p "$outdir"

target="$outdir/$slug.md"
if [[ -e "$target" ]]; then
    echo "refusing to overwrite existing $target" >&2
    exit 73
fi

date=$(date +%Y-%m-%d)

cat > "$target" <<EOF
# $slug

<!-- TODO: one-sentence purpose -->

## 1. Purpose

<!-- TODO: name technique + topology + originator -->

## 2. Theory

<!-- TODO: difference equation, H(z), discretization method, stability bound, valid parameter range. -->

## 3. Algorithm

\`\`\`rust
// TODO: minimal pseudocode or runnable snippet matching the implementation
\`\`\`

## 4. Parameters

| Name | Type | Units | Range | Default | Notes |
| ---- | ---- | ---- | ---- | ---- | ---- |
| <!-- TODO --> |  |  |  |  |  |

## 5. Response plots

<!-- TODO: link plots committed under docs/plots/<slug>_*.svg with CSVs under docs/plots/data/ -->

## 6. Realtime contract

- **Allocation.** <!-- TODO -->
- **Denormals.** <!-- TODO -->
- **Reset.** <!-- TODO -->
- **Thread safety.** <!-- TODO -->
- **Bounded work.** <!-- TODO -->
- **Finite output.** <!-- TODO -->
- **SIMD.** <!-- TODO -->

## 7. Test coverage

<!-- TODO: full module paths for the tests that pin behavior. -->

## 8. Usage example

\`\`\`rust
// TODO: minimal compileable snippet using the real public API
\`\`\`

## 9. References

<!-- TODO: external (RBJ / Smith / Cytomic / DAFx) + internal (source file path) -->

---

Generated $date by lindelion-dsp-docs/scripts/new-dsp-doc.sh
EOF

echo "wrote $target"
echo "next: open references/skeleton.md and references/worked-example.md alongside while filling in sections."
