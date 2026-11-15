# Noise Suppressor (RNNoise) — skeptical review (2026-11-15)

Verified correct: ×32768 scaling in/out; latency 960 (1 frame library overlap-add + 1 primed); one shared gate_gain;
gate smoothing ~70 ms; nnnoiseless allocates nothing per frame; queue cap covers blocks ≤ 8192.

## Defects
1. (high) VAD averaged across channels (`:124`): silent R input caps VAD ≈ 0.5 → gate never opens above 50%. Fix: `vad_l.max(vad_r)`.
2. (high) Gate one 70 ms time for open+close, no hold/hysteresis: onsets clipped (~−14 dB at start), noise floor pumps. Fix: open ~3 ms, close ~150 ms, hold ~200 ms, hysteresis ~0.1.
3. (high) Large blocks: gate target set once per block from newest frame (8192 block = 17 frames on one reading). Fix: per-frame gate.
4. (high) NaN input poisons nnnoiseless state forever. Fix: sanitize input, rebuild state on non-finite output.
5. (high, rare) UI VAD/param reads share the processor mutex; audio try_lock fail → block skips plugin (2 clicks, 20 ms jump). Fix: atomics in add-on API.
6. (high) Un-bypass replays stale queued audio. Fix: `reset()` in the API, called when bypass turns off.
7. (high) Wet-alignment test is modulo the 320-sample tone period → can't catch 640/1280 lags. Fix: gliding-pitch signal, assert 960 ± 4.
8. (medium) Mix/output gain unsmoothed → zipper.
9. (low) easyfft per-thread planner allocates on first frame after each device restart. Document.
Cleanup: stale doc comments at `:166`, `:77`; delete redundant `unsafe impl Send` (`:177`).

## Upgrades
| # | Upgrade | Value | Risk | Size | Sound/params |
|---|---|---|---|---|---|
| 1 | Max-VAD + open/close/hold (D1,D2) | High | Low | S | onsets kept / optional |
| 2 | Gate attenuation in dB ("Floor" 0..−60) | High | Low | S | same / remap + preset migration |
| 3 | Mono-sum / auto-detect identical L/R → one state | High | Low | S | CPU halves |
| 4 | GR meter + threshold marker on VAD orb (smoothed, peak hold) | High | Low | S–M | UI only |
| 5 | Default attenuation −30 dB | Med | Low | XS | presets unaffected |
| 6 | NaN sanitize + reset() hook (D4,D6) | Med | Low | S | No |
| 7 | Resampling 44.1/96 kHz (rubato 0.14.1 already compiled via libDF) | Med | Med | M | +latency/CPU |
| 8 | True linked stereo (needs nnnoiseless fork) | Med | High | L | — |
UX: gate still acts at Mix 0 — label it.
