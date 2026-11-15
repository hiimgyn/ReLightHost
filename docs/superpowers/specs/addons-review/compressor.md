# Compressor — skeptical review (2026-11-15)

Verified correct: max(|L|,|R|) stereo link; attack/release `exp(-1/(τ·sr))` = time-to-63%; dB-domain gain computer;
parallel mix; transfer curve == DSP (max diff 1.4e-14 dB); NaN guard fixes a real latch; FTZ by host; ~2 transcendentals/frame.

## Defects
- A1 (high) GR meter and live dot are fake (`CompressorGui.tsx:184-185`): dot = threshold-6, GR = f(params). Remove → real meters (B1).
- A2 (high) Every gain change is an unsmoothed step (`compressor.rs:77-84,107-110`): Reset with +30 dB makeup clicks; knob drags zipper. Fix: one-pole 5–20 ms smoother on final factor.
- A3 (high) Detector under-reads (release branch tracks |x|): −6 dBFS sine → 8.1 dB GR vs 9.0 dB promised; 50 Hz with 0.1/10 ms → GR swings 7–9 dB per half-cycle. Fix: log-domain branching/decoupled peak detector on GR. Doc comment order is also wrong.
- A4 (medium) UI param write holds processor mutex; audio `try_lock` fails → block passes uncompressed. Fix in add-on API: params + meters via atomics.
- A5 (cosmetic) Curve clamps output to [-60,0] dB; DSP can exceed 0 dBFS.
- No DSP unit tests (static curve, mix=0 identity, ratio=1, NaN reset).

## Upgrades
| # | Upgrade | Value | Risk | Size | Sound/params |
|---|---|---|---|---|---|
| B1 | Real meters (in/GR/out dB) via atomics + `meters()` | High | Low | M | No/No |
| B2 | Gain smoothing (A2) | High | Low | S | clicks only/No |
| B3 | Sidechain HPF ~80–150 Hz on detector | High (voice) | Low | S | Yes/+1 or fixed |
| B4 | Log-domain detector (A3) | Med | Med | S | slightly/No |
| B5 | Auto-makeup | Med | Low | S | opt-in/+1 toggle |
| B6 | RMS/peak mode | Med | Low | S | Yes/+1 |
| B7 | Lookahead limiter mode (reports latency) | Med | Med | M | Yes/+1–2 |
| B8 | Negative makeup range, hold | Low | Low | XS | No/range+1 |

Suggested order: B2, B1, B3, B4 + tests; B5–B7 opt-in, default off so presets sound the same.
