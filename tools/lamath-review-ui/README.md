# Lamath Review UI

Local dev-only reviewer for the Lamath render catalog.

## Run

```sh
cd tools/lamath-review-ui
npm install
npm run dev
```

The server defaults to Sulion's first published dev-server slot:
`0.0.0.0:26000`, visible on the LAN as `http://192.168.66.3:26000/`.
Only ports `26000-26010` are published by the Sulion Docker Compose stack.

The server prints the LAN URL when it starts. By default it reads:

- Manifest: `../../review/lamath-render-catalog/manifest.toml`
- WAV root: `../../review/lamath-render-catalog`
- Comments: `../../review/lamath-render-catalog-comments.json`

The audition players serve the manifest WAVs directly. MP3 previews are intentionally ignored so
fresh renders cannot be shadowed by stale compressed files.

File and category comments save on textarea blur. Empty comments remove that entry from
the comments JSON. Category comment keys are stable IDs such as `group:drivers` and
`tag:mesh`.

## Variant rows

Cases that are the same audition differing only along axes — driver/family, velocity, bell,
register, etc. — collapse into a single row with one selector per varying axis (a constant axis
shows as fixed context). Switching a selector swaps the player, metrics, created-at, and comment
to that rendered case; comments stay keyed per case, so each variant keeps its own note. A row's
created-at is the selected WAV's last-render time, so re-renders are visible per variant.

The axes come from `manifest.toml` schema **v2** (each `[[cases]]` record carries a `family` key
and structured `axes`). The audio metrics are unchanged from v1, so the manifest can be upgraded
**without re-rendering** (which would reset every WAV's timestamp):

```sh
cargo run -p lamath --bin lamath-render-catalog -- --regen-manifest
```

This re-emits `manifest.toml`/`index.md` from the catalog, reusing the metrics already stored for
each WAV and writing no audio. A pre-v2 manifest still loads — each case just renders as its own
single-variant row until regenerated.

## Overrides

```sh
LAMATH_REVIEW_MANIFEST=/path/to/manifest.toml \
LAMATH_REVIEW_WAV_ROOT=/path/to/wavs \
LAMATH_REVIEW_COMMENTS_FILE=/path/to/comments.json \
npm run dev
```

Use another Sulion slot if `26000` is already occupied:

```sh
PORT=26001 npm run dev
```
