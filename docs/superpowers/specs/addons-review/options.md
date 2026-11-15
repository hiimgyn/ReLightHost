# Optional upgrades: your menu

The plan fixes bugs and adds real meters, smoothing and tests. It does NOT do anything below, because each
item adds a control or changes how existing presets sound. Pick by id, e.g. "do C-HPF, V-DEESS".
Nothing here is implemented. Sources: the four reviews in this folder.

"Params" = adds a control or changes a range. "Sound" = existing presets would sound different.

## Best next 5

1. **C-HPF** (compressor sidechain high-pass): stops breath, desk thumps and rumble from squashing your voice. High value, low risk, small.
2. **V-HPF** (voice rumble filter): cleans low rumble before Drive and the EQ, so everything after it works on cleaner audio. High value, low risk, small.
3. **N-MONO** (noise suppressor mono-sum): a mic with identical left and right needs one model, not two, so CPU halves with no change in sound. High value, low risk, small.
4. **V-DEESS** (de-esser): tames harsh "s" sounds, the most common streamer complaint. High value, medium risk, medium size.
5. **D-MIXFOLD** (deep filter, fold Mix into attenuation): Mix then behaves smoothly instead of combing at mid settings; the control id stays, no new controls. High value, low risk, small.

Close sixth: **D-RESAMP**, high value, but medium risk and medium size.

## Compressor

| Id | What it does | Value | Risk | Size | Params / Sound |
|---|---|---|---|---|---|
| C-HPF | Detector ignores bass (about 80-150 Hz), so rumble and plosives do not trigger compression. | High (voice) | Low | S | +1 param (or fixed) / Yes |
| C-AUTOMAKE | Toggle that adds makeup gain for you as threshold and ratio change. | Medium | Low | S | +1 toggle / only if on |
| C-RMS | Peak or RMS detection: RMS reacts to loudness (smoother), peak to spikes. | Medium | Low | S | +1 param / Yes |
| C-LIMIT | Lookahead limiter mode that catches peaks before they pass. Reports latency. | Medium | Medium | M | +1-2 params / Yes |
| C-LOGDET | Log-domain detector to fix the detector under-reading (8.1 dB reduction vs 9.0 promised). | Medium | Medium | S | No / slightly |

Note on C-LOGDET: a plan reviewer measured it and the 50 Hz ripple got worse, not better
(gain swings 6.5-9.0 dB with it vs 7.0-9.0 dB now). Recommendation: skip unless a different detector design is tried.
C-AUTOMAKE, C-RMS and C-LIMIT would default to off, so existing presets keep their sound.

## Noise suppressor

| Id | What it does | Value | Risk | Size | Params / Sound |
|---|---|---|---|---|---|
| N-FLOOR | Replace the gate strength with a "Floor" in dB (0 to -60): how quiet the removed noise ends up. Easier to understand. | High | Low | S | Remaps a param, presets need migration / same sound |
| N-MONO | Detect identical left and right and process once. | High | Low | S | No / same, CPU halves |

Not on the menu: true linked stereo (needs a fork of the noise library; Medium value, High risk, Large).
Default attenuation of -30 dB is tiny (XS) but would be done together with N-FLOOR.

## Voice

| Id | What it does | Value | Risk | Size | Params / Sound |
|---|---|---|---|---|---|
| V-HPF | Rumble filter (80-120 Hz) before Drive. | High | Low | S | +1 / Yes |
| V-DEESS | Softens harsh "s" sounds (5-9 kHz) before Drive. | High (streamers) | Medium | M | +1-2 / Yes |
| V-PRESETS | One-click voices: Broadcast, Podcast, Discord-clean, Warm. | Medium | Low | S | No / only when chosen |
| V-TPLIM | True-peak limiter with 1-2 ms lookahead; catches peaks between samples. Reports latency. | Medium | Medium | M | No / slight |
| V-GAIN | Output gain after the limiter, to set final level. | Medium | Low | XS | +1 / No (default 0 dB) |
| V-ADAA | Reduce drive aliasing (harsh false tones). Plain ADAA dulls highs (-1.25 dB at 8 kHz, -6 dB at 16 kHz), so it needs 2x oversampling. | Medium-high | Medium (presets) | M | No / Yes |
| V-DRIVE | Restore level at heavy Drive: today Drive 1.0 caps output near -18 dBFS and Drive 0.5 costs about 3 dB. | Medium-high | Medium (presets) | M | No / Yes, level changes |

V-ADAA and V-DRIVE are best done together (same drive stage), with optional asymmetry for warmth.

## Deep filter

| Id | What it does | Value | Risk | Size | Params / Sound |
|---|---|---|---|---|---|
| D-THRESH | Use the upstream thresholds -15/35/35 instead of -10/30/20, so soft speech is not zeroed. Audible effect not measured. | Medium | Low | XS | No / Yes |
| D-MIXFOLD | Fold Mix into the attenuation limit, so Mix at 50% is smooth instead of combing. | High | Low | S | No (id kept) / Mix curve changes |
| D-RESAMP | Native 44.1 and 96 kHz by resampling on the worker thread (library already present). | High | Medium | M | No / +2-5 ms latency, more CPU |
| D-MONO | Mono or linked mode: identical L/R runs one channel, -33% CPU. | Medium | Low | S-M | No / same |
| D-WARM | Pass audio through and fade the filter in while the model loads (about 0.5 s). | Medium | Low | S | No / No |
| D-LAYOUT | Controls: Strength (0-60 dB or off), Post-filter, Gain; thresholds under Advanced. | Medium | Low | S | +params / No |
| D-LITE | Lighter model (256 hidden, +20 ms lookahead): less CPU, lower quality. | Low-medium | Medium | M | No / Yes |

Already decided: the post-filter default of 0.02 (range 0-0.1, migration v/10) was approved and is done in
Task 9, so it is not on this menu. D-THRESH is the separate, not implemented, item #8.

## All add-ons

| Id | What it does | Value | Risk | Size | Params / Sound |
|---|---|---|---|---|---|
| X-UNBYPASS | Crossfade when un-bypassing. The plan resets the add-on on un-bypass, which plays its latency as silence (20 ms for the noise suppressor); a crossfade would smooth that. | Medium | Low | M | No / No |
