# Test audio fixtures — provenance & licensing

Every committed audio fixture must have a clear, redistributable license. This file records
the source and license of each. Synthetic fixtures generated in-code are not listed here.

## Licensing rule

Only **public-domain / CC0** audio is committed. Anything requiring attribution (CC-BY) must
have its attribution recorded in this file before commit. No copyrighted material.

## Existing fixtures

| File | Used by | Source / license |
| --- | --- | --- |
| `sax_test.wav` | `lindelion-pitch-shift` sax fixture tests | pre-existing in repo |
| `crunch_pitch_shift_regression.wav` | wind-fixture / Linnod crunch regression (clean 1st half, bad-algorithm crunch 2nd half) | pre-existing in repo |
| `crunch_17c_pitch_shift_regression.wav` | wind-fixture / Linnod crunch regression | pre-existing in repo |

## Added — pending review (PD ensemble / mix material)

Trimmed mono 44.1 kHz / 16-bit clips (3.5 s, 20 ms edge fades, encoder metadata stripped) from
**public-domain recordings by The Airmen of Note (United States Air Force Band)** — works of the
U.S. federal government, public domain in the U.S. These cover real full-mix / polyphonic /
in-mix-transient material the synthetic battery cannot.

| File | What it exercises | crest / centroid / low / high | Source recording (Wikimedia Commons, Public domain) | Trim |
| --- | --- | --- | --- | --- |
| `usaf_jazz_bassmix.wav` | full mix, bass-rich + transients | 5.4 / 2776 Hz / 0.31 / 0.21 | [Eagle Eyes](https://commons.wikimedia.org/wiki/File:Eagle_Eyes_-_Airmen_of_Note_-_United_States_Air_Force_Band.mp3) | 50.0–53.5 s |
| `usaf_jazz_sustained.wav` | sustained ensemble (mid) | 4.0 / 2785 Hz / 0.06 / 0.15 | [Eagle Eyes](https://commons.wikimedia.org/wiki/File:Eagle_Eyes_-_Airmen_of_Note_-_United_States_Air_Force_Band.mp3) | 101.0–104.5 s |
| `usaf_jazz_bright.wav` | bright / cymbal-heavy mix (HF) | 4.4 / 4718 Hz / 0.10 / 0.35 | [Sheridan Square](https://commons.wikimedia.org/wiki/File:Sheridan_Square_-_Airmen_of_Note_-_United_States_Air_Force_Band.mp3) | 74.0–77.5 s |
| `usaf_jazz_transient.wav` | transient-heavy ensemble | 5.5 / 3456 Hz / 0.05 / 0.22 | [Sheridan Square](https://commons.wikimedia.org/wiki/File:Sheridan_Square_-_Airmen_of_Note_-_United_States_Air_Force_Band.mp3) | 148.0–151.5 s |

Provenance: downloaded from `upload.wikimedia.org/wikipedia/commons/...`, decoded and trimmed
with a static ffmpeg; sources not committed. Commons license tag for both recordings:
**Public domain** (U.S. Air Force Band, work of the U.S. federal government).

## Added — pending review (isolated instrument notes, University of Iowa MIS)

Single isolated notes from the **University of Iowa Electronic Music Studios Musical Instrument
Samples** (post-2012 individual pitches). Each converted to mono 44.1 kHz / 16-bit, leading
silence stripped so the clip starts at the onset, trimmed to ≤2.5 s with fades, metadata removed.
These give clean single-source material across registers and timbres (formants, bass, bowed
sustain, mallet/percussive transients) that the synthetic battery and the ensemble mixes cannot.

**License (quoted from <https://theremin.music.uiowa.edu/MIS.html>):** the recordings are
*"freely available on this website and may be downloaded and used for any projects, without
restrictions."* Source pages under `https://theremin.music.uiowa.edu/MIS-Pitches-2012/`; audio
under `https://theremin.music.uiowa.edu/sound files/MIS Pitches - 2014/<family>/<instrument>/`.

| File | Note / character | f0 · crest · centroid | Source `.aif` |
| --- | --- | --- | --- |
| `iowa_doublebass_E2.wav` | bowed double bass, sub-bass | 73 Hz · 2.7 · 720 Hz | `Bass.arco.ff.sulE.E2.stereo.aif` |
| `iowa_tuba_E2.wav` | tuba, low brass | 84 Hz · 6.5 · 1255 Hz | `Tuba.ff.E2.stereo.aif` |
| `iowa_cello_C3.wav` | bowed cello, low-mid | 135 Hz · 3.0 · 1114 Hz | `Cello.arco.ff.sulC.C3.stereo.aif` |
| `iowa_bassoon_C3.wav` | bassoon, reedy low woodwind | 130 Hz · 2.6 · 1473 Hz | `Bassoon.ff.C3.stereo.aif` |
| `iowa_horn_C3.wav` | french horn, mellow brass | 132 Hz · 5.1 · 1199 Hz | `Horn.ff.C3.stereo.aif` |
| `iowa_viola_C4.wav` | bowed viola, mid | 258 Hz · 2.5 · 1908 Hz | `Viola.arco.ff.sulC.C4.stereo.aif` |
| `iowa_marimba_C4.wav` | marimba (yarn mallet), pitched transient | 264 Hz · 5.0 · 875 Hz | `Marimba.yarn.ff.C4.stereo.aif` |
| `iowa_clarinet_G4.wav` | B♭ clarinet, mid woodwind | 394 Hz · 2.6 · 3175 Hz | `BbClarinet.ff.G4.stereo.aif` |
| `iowa_violin_A4.wav` | bowed violin, A=440 | 441 Hz · 5.5 · 2496 Hz | `Violin.arco.ff.sulG.A4.stereo.aif` |
| `iowa_oboe_A4.wav` | oboe, reedy formant-rich | 441 Hz · 2.3 · 2958 Hz | `Oboe.ff.A4.stereo.aif` |
| `iowa_trumpet_C5.wav` | trumpet (vibrato), bright brass | 525 Hz · 3.2 · 2564 Hz | `Trumpet.vib.ff.C5.stereo.aif` |
| `iowa_vibraphone_C5.wav` | vibraphone, struck + sustain | 525 Hz · 3.8 · 1479 Hz | `Vibraphone.sustain.ff.C5.stereo.aif` |
| `iowa_flute_A5.wav` | flute (vibrato), breathy high | 900 Hz · 3.0 · 3694 Hz | `Flute.vib.ff.A5.stereo.aif` |
| `iowa_cymbal_crash.wav` | 13″ crash cymbal, broadband transient | noise · 22.2 · 7561 Hz | `13crash.stick.bell.ff.stereo.aif` |
| `iowa_tambourine.wav` | tambourine, bright noise transient | noise · 16.5 · 13164 Hz | `tambourine1.normal.ff.stereo.aif` |

## Added — pending review (vocals, repo owner's own recordings)

Recorded by the repository owner and contributed as test fixtures (rights held by the owner;
free to use within this project). Relocated from the repo root verbatim — no re-encoding or
trimming, so they are the original recordings. Fill the vocal/formant gap (M1) the synthetic
battery and instrument samples cannot.

| File | Content | f0 · crest · centroid · channels |
| --- | --- | --- |
| `vocal_sung.wav` | sung voice (sustained, pitched) | 464 Hz · 6.9 · 1655 Hz · mono · 5.97 s |
| `vocal_spoken.wav` | spoken voice (dynamic, consonant transients) | 118 Hz · 7.1 · 1189 Hz · stereo · 4.30 s |

## Added — pending review (owner clarinet gesture recording)

Recorded by the repository owner / user and contributed as test fixtures (rights held by the
owner; free to use within this project). Extracted from the uploaded repo-root `clarinet.wav` as
whole musical gestures, not stable-center snippets: phrase timing, attacks, sustains, releases,
and time variance are intentionally preserved. Converted to mono 44.1 kHz / 16-bit, with 10 ms
edge fades and no peak normalization.

Harmonic statistics use per-frame pitch tracking and harmonic sampling against each frame's
current f0; concert-pitch estimates are reported below. B-flat clarinet written pitch is about a
whole step above these concert estimates.

| File | Content | source window | f0 / character | h3 · upper odd tail · centroid |
| --- | --- | --- | --- | --- |
| `owner_clarinet_written_c_major_scale.wav` | written C-major scale phrase | 2.05-15.65 s | 13.60 s phrase, concert A#3-A#4 path | -8.7 dB · -32.5 dB · 874 Hz |
| `owner_clarinet_mid_register_phrase.wav` | mid-register phrase | 15.85-19.60 s | concert A#4-C5-A4 phrase | -8.1 dB · -50.0 dB · 633 Hz |
| `owner_clarinet_low_register_phrase.wav` | low-register phrase | 19.30-22.90 s | concert A#3-C4-A3 phrase | -6.8 dB · -25.4 dB · 627 Hz |
| `owner_clarinet_low_e_sustain.wav` | low E sustain (written), full bore | 27.60-34.10 s | concert D3, 6.50 s | -4.8 dB · -24.3 dB · 281 Hz |
| `owner_clarinet_register_key_high_sustain.wav` | same fingering with register key | 34.60-41.25 s | concert A4, 6.65 s | -11.8 dB · -43.5 dB · 524 Hz |
| `owner_clarinet_low_high_articulation.wav` | repeated articulation: 8 low, 8 high, 8 low, 8 high | 41.60-48.50 s | concert D3/A4 alternation | -10.4 dB · -40.6 dB · 327 Hz |
| `owner_clarinet_quick_altissimo_phrase.wav` | quick upper-register and altissimo phrase | 48.30-55.40 s | concert D3-A#5 phrase | -6.4 dB · -40.4 dB · 582 Hz |
| `owner_clarinet_altissimo_sustain.wav` | altissimo sustain with embouchure down-bend tail | 55.55-59.20 s | concert B5, 3.65 s | +0.7 dB · -40.8 dB · 1993 Hz |

## Added — public-domain spoken word (LibriVox, for the speech-effect port)

Speech fixtures for the `speech/` effects, sourced from **LibriVox** recordings, which are
released into the **public domain** (LibriVox dedicates all its recordings to the public domain
worldwide). Each was downloaded from archive.org, decoded with a static ffmpeg, and trimmed to a
5.0 s window (20 ms edge fades, peak-normalized to −3 dBFS — gain only, so dynamics / cadence /
pitch are preserved — encoder metadata stripped), **mono 48 kHz / 16-bit** (the speech effects'
native rate, so tests need no resampling).

Features are measured on the final clip: `low`/`high` = sub-250 Hz / >4 kHz energy fraction;
`syl/s` = envelope-peak syllable rate (cadence proxy); `pause` = fraction of low-energy frames;
`pstd` = pitch standard deviation in semitones (flat ↔ animated); `crest` = peak/RMS (dynamics).

| File | Exercises | low · high · syl/s · pause · pstd · crest | Source (LibriVox — Public domain) · trim |
| --- | --- | --- | --- |
| `speech_clean_continuous_48k.wav` | clean continuous speech; **bass-rich** (bass enhancer); fast | 0.78 · 0.02 · 3.7 · 0.12 · 1.9 · 4.5 | *The Forgotten Man and Other Essays* (Sumner), [forgottenman_2208](https://archive.org/details/forgottenman_2208_librivox) `forgottenman_16_sumner` · 62.5–67.5 s |
| `speech_noisy_48k.wav` | noisy speech (denoiser; enhancement under noise) — matched pair with `speech_clean_continuous` | 0.76 · 0.04 · 3.0 · 0.00 · 1.8 · 4.6 | `speech_clean_continuous` segment + synthetic pink noise at **10 dB SNR** |
| `speech_pauses_48k.wav` | speech **with pauses** (voice gate, cadence); **high dynamics** (expander) | 0.29 · 0.02 · 3.5 · 0.26 · 2.6 · 12.6 | *Female Scripture Characters* (Jay), [femalescripturecharacters_2405](https://archive.org/details/femalescripturecharacters_2405_librivox) `femalescripturecharacters_28_jay` · 22.5–27.5 s |
| `speech_flat_48k.wav` | **flat / monotone** delivery (cadence) | 0.30 · 0.01 · 3.2 · 0.26 · 1.1 · 9.1 | *Poems* (Chesterton), [poems_1102](https://archive.org/details/poems_1102_librivox) `poems_11_chesterton` · 37.5–42.5 s |
| `speech_animated_48k.wav` | **animated / expressive** delivery (cadence) | 0.10 · 0.00 · 3.7 · 0.13 · 7.4 · 5.7 | *Grimm's Fairy Tales*, [grimmsfairytales_2104](https://archive.org/details/grimmsfairytales_2104_librivox) `fairytales_34_grimm` · 38.5–43.5 s |
| `speech_fast_48k.wav` | **fast** speech rate (cadence / WPM) | 0.45 · 0.02 · 3.8 · 0.15 · 3.6 · 5.8 | *The Divine Comedy* (dramatic reading), [divinecomedy2dramatic_1509](https://archive.org/details/divinecomedy2dramatic_1509_librivox) `divinecomedy_055_alighieri` · 81.0–86.0 s |
| `speech_slow_48k.wav` | **slow** speech rate (cadence / WPM); continuous | 0.32 · 0.02 · 2.8 · 0.14 · 2.5 · 6.4 | *The Happiness of Hazelbrook* (dramatic reading), [happinessofhazelbrook_2605](https://archive.org/details/happinessofhazelbrook_2605_librivox) `happinesshazelbrook_05_obrien` · 39.5–44.5 s |

Provenance: each source MP3 was fetched from `archive.org/download/<identifier>/<file>.mp3`,
decoded + trimmed in-tooling; source MP3s are not committed. LibriVox license tag: **Public
domain.** `speech_noisy_48k.wav`'s pink noise is synthetic (deterministic), so the speech content
remains public domain.
