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
