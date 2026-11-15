# DeepFilterNet — skeptical review (2026-11-15)

Measured (release bench on vendored libDF): DFN3-LL, conv/DF lookahead 0, FFT 960 / hop 480; delay 480 at 6 dB atten
(prev_noisy alignment correct); CPU per hop mono 0.45 ms (p99 2.4), stereo 0.67 ms (p99 3.2–3.6); model load 0.42–0.55 s;
stereo = batch of 2 (ERB mask averaged, DF coefs per channel); set_atten_lim fix correct; NaN poisons 3 hops then recovers.

## Defects
1. (high) Hop-drop cap 2400 (`:20,235`) < sustainable delay ≈ 960+n−gcd(n,480): n=2048 → ~2976 → a hop dropped every block.
   Fix: cap = 960 + n + 3·HOP.
2. (high) Max Attenuation 0 dB: libDF bypasses with 0 delay (`tract.rs:543`) → 10 ms skip/repeat, comb at mid Mix, click,
   VAD 100%. Fix: worker passes `atten.max(0.1)`.
3. (high) Underrun resume repeats g dry samples and grows delay; start delay below sustainable unless n | 480
   (n=256: 992 vs 1184) → startup always underruns. Fix: seed delay from first block size; on resume skip `dry_ahead`
   from both queues with 2 ms crossfade.
4. (high) Bypass freezes queues/model → 20–60 ms stale audio on un-bypass. Fix: reset() on bypass change.
5. (med-high) latency fixed 960 (`:321`); real ≥1184 @256, 1440 @512, 1952 @1024, grows after underruns. Fix: report dry_l.len() via atomic.
6. (medium) Model failure → pass-through but no Error status; worker panic leaves `running` true → permanent dry.
   Fix: catch_unwind in worker; status/loading in the API.
7. (med-high) Post-filter default 0.2 on 0–1 (upstream 0.02): bin gain 0.1 → −18 dB extra at 0.2 vs −5 dB at 0.02.
   Fix: default 0.02, range 0–0.1 (keep id → version bump + migrate), rename "Post-filter strength".
8. (medium) Thresholds −10/30/20 vs upstream −15/35/35: soft speech zeroed, processed/unprocessed switching above 30 dB.
   Fix: with_thresholds(-15, 35, 35). Audible effect not measured.
9. (low) Inference-error path pushes current hop, not prev_noisy (480-sample jump).
Comment at `:120` "no heap allocation" is false (libDF allocates per hop on the worker; harmless).

## Upgrades
| # | Upgrade | Value | Risk | Size | Sound/params |
|---|---|---|---|---|---|
| 1 | Fold Mix into attenuation (eff. limit = 1 − m(1 − l)); ring → [f32;2] | High | Low | S | none (id kept) |
| 2 | Resampling 44.1/96 kHz (rubato 0.14.1 already in lock), on worker | High | Med | M | +2–5 ms |
| 3 | Mono/linked mode (L==R → one channel, −33% CPU) | Med | Low | S–M | — |
| 4 | Real meter: per-hop out/in energy dB (GR) + smoothed SNR | High | Low | S | UI |
| 5 | Warm-up pass-through + crossfade in (0.5 s model load) | Med | Low | S | — |
| 6 | Crossfades for trimmer skips, hop drops, bypass | Med | Low | M | — |
| 7 | Controls: Strength (0–60/∞), Post-filter, Gain; thresholds under Advanced | Med | Low | S | +params |
| 8 | Lighter DFN3 model (256 hidden, +20 ms lookahead) | Low-med | Med | M | changes |
| 9 | Worker park/semaphore wake instead of 1 ms poll | Low | Low | S | — |
