# Voice — skeptical review (2026-11-15)

Recent fixes: delay line always fed — correct. Drive /pre normalisation — quiet level now unity, but see A2.
Keeping biquad state at the 0 dB snap — makes clicks worse, see A1. RT cost fine (~6 biquads + 2 tanh / frame).

## Defects
- A1 (high, simulated) Band jump to 0 dB clicks ~2× harder (84× vs 43× normal step) because `set_identity` releases
  loaded state. Fix: delete the `< 0.05 dB` shortcut (`:42,57,77,97`); formulas give pass-through at 0 dB (+12→0 then 1.3×).
- A2 (high) Drive=1 output = tanh(8x)/8 ≤ 0.125 → −18 dBFS brick wall; Drive 0.5 costs −2.9 dB on −6 dBFS peaks (`:146-150`).
- A3 (med-high) Drive aliases (no oversampling, 8× pre-gain): 9th harmonic of 5 kHz folds to 3 kHz. Fix: ADAA or 2–4× OS.
- A4 (high) Width = 12 ms Haas (L dry, R delayed): mono sum notches at 41.7 Hz + every 83.3 Hz; image pulls left.
  Discord sends mono. Fix: B1.
- A5 (high) BodePlotCanvas draws hand-made shapes, not the RBJ response (200 Hz: graph +12 vs DSP +6; 4 kHz High: +3.7 vs +0.7).
- A6 (med) EQ/drive/width/ceiling step changes → zipper; Width 0→1 swaps R instantly (click).
- A7 (med) UI param write holds processor mutex → audio try_lock fail → block unprocessed. Fix: atomics in API.
- Minor: "Tube Saturation" label (symmetric tanh = odd harmonics only); hard-coded English in VoiceGui (:148,:205,:277,:282);
  limiter sample-peak, no lookahead, instant attack.

## Upgrades
| # | Upgrade | Value | Risk | Size | Sound/params |
|---|---|---|---|---|---|
| 1 | Mono-safe width: L=M+w·S, R=M−w·S (S = delayed/all-pass M) | High | Low | S | changes / none |
| 2 | Delete 0 dB shortcut (A1) | High | Very low | XS | No |
| 3 | Parameter smoothing (dB per-sample ~20 ms, coeffs every 16–32 samples) | High | Low | S | No |
| 4 | HPF/rumble 80–120 Hz before Drive | High | Low | S | +1 |
| 5 | Re-gain Drive + ADAA/2× OS, optional asymmetry | Med-high | Med (presets) | M | changes |
| 6 | Matching EQ graph (A5) | Med | Very low | S | UI |
| 7 | De-esser 5–9 kHz before Drive | High (streamers) | Med | M | +1–2 |
| 8 | Voice presets (Broadcast, Podcast, Discord-clean, Warm) | Med | Low | S | No |
| 9 | True-peak limiter, 1–2 ms lookahead (report latency) | Med | Med | M | slight |
| 10 | Output/makeup gain | Med | Low | XS | +1 |
Order: HPF → de-esser → EQ → drive → width → limiter.
