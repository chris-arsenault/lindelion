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
- MP3 preview root: `../../review/audio-previews/lamath-render-catalog`
- Comments: `../../review/lamath-render-catalog-comments.json`

File and category comments save on textarea blur. Empty comments remove that entry from
the comments JSON. Category comment keys are stable IDs such as `group:drivers` and
`tag:mesh`.

## Overrides

```sh
LAMATH_REVIEW_MANIFEST=/path/to/manifest.toml \
LAMATH_REVIEW_WAV_ROOT=/path/to/wavs \
LAMATH_REVIEW_PREVIEW_ROOT=/path/to/previews \
LAMATH_REVIEW_COMMENTS_FILE=/path/to/comments.json \
npm run dev
```

Use another Sulion slot if `26000` is already occupied:

```sh
PORT=26001 npm run dev
```
