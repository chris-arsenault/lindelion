# Diagram recipes for DSP docs

DSP block diagrams use a small standard symbol set: summing junction (circle with `+`, minus inputs labelled), gain triangle (label inside, point downstream), unit-delay box (`z⁻¹`), multiplier (`⊗`), branch point. Pick the tool by audience.

## Decision rule

| Audience and detail | Tool | Output |
| ---- | ---- | ---- |
| High-level signal flow, module-to-module pipelines | Mermaid (inline in `.md`) | Rendered inline on GitHub; pure text |
| Textbook-style biquad block diagram, modal-bank topology, waveguide loop | Hand-authored SVG under `docs/diagrams/` | Committed SVG; cite from doc |
| Inline in a Rust doc comment where SVG can't render | ASCII art | Inline only |

Avoid `draw.io` / `excalidraw` round-trip files — their embedded JSON produces unreadable diffs. Reserve TikZ for the rare case where Mermaid and hand-SVG are both insufficient; do not add `texlive-*` to `make ci`.

## Mermaid recipe (high-level signal flow)

````markdown
```mermaid
flowchart LR
    X((x[n])) --> S(("+"))
    S --> Y((y[n]))
    Y --> D["z⁻¹"] --> G["× a₁"] --> S
```
````

Renders on GitHub. Standard symbols are approximated with Unicode: `⊕`, `z⁻¹`, `× k`.

## Hand-authored SVG recipe

1. Author in Inkscape, Figma, or by hand.
2. Use the standard symbol set: circle-`+` summing junction, triangle gain (label inside), rectangle `z⁻¹` delay.
3. Save as `docs/diagrams/<module>-<topic>.svg`.
4. Reference from the doc with relative path: `![](../diagrams/biquad-df2t.svg)`.

Reusable practice: build a small SVG `<symbol>` library (`<symbol id="adder">`, `<symbol id="delay">`, `<symbol id="gain">`) and `<use>` them. The SVG stays text-diffable.

## ASCII recipe (inline in Rust `///` comments)

```text
x[n] ──►(+)──┬──► y[n]
        ▲    │
        │    ▼
        └──[z⁻¹]──[× a₁]
```

Keep ASCII diagrams under ~12 lines and ~60 columns. Anything larger goes to an SVG.

## Symbol legend

| Symbol | Meaning |
| ---- | ---- |
| `(+)` or circle with `+` | Summing junction. Label minus inputs. |
| `[k]` or triangle pointing downstream | Gain (scalar multiply). Label inside. |
| `[z⁻¹]` or rectangle labelled `z⁻¹` | Unit-sample delay. |
| `⊗` | Multiplier (signal × signal). |
| `─►` | Signal flow, generally left-to-right. |
| `│ / ┬` | Branch point. |
