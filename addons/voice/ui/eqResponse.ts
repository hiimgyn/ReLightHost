// Mirrors the RBJ biquads in addons/voice/src/lib.rs 1:1 (shelf slope S=1). No imports.
export type Biquad = { b0: number; b1: number; b2: number; a1: number; a2: number };

const shelf = (low: boolean, gainDb: number, f0: number, sr: number): Biquad => {
  const a = Math.pow(10, gainDb / 40);
  const w0 = (2 * Math.PI * f0) / sr;
  const cos = Math.cos(w0);
  const alph = (Math.sin(w0) / 2) * Math.sqrt(2);
  const sq = 2 * Math.sqrt(a) * alph;
  const s = low ? 1 : -1;
  const b0 = a * ((a + 1) - s * (a - 1) * cos + sq);
  const b1 = s * 2 * a * ((a - 1) - s * (a + 1) * cos);
  const b2 = a * ((a + 1) - s * (a - 1) * cos - sq);
  const a0 = (a + 1) + s * (a - 1) * cos + sq;
  const a1 = -s * 2 * ((a - 1) + s * (a + 1) * cos);
  const a2 = (a + 1) + s * (a - 1) * cos - sq;
  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 };
};

export const lowShelf = (gainDb: number, f0: number, sr: number) => shelf(true, gainDb, f0, sr);
export const highShelf = (gainDb: number, f0: number, sr: number) => shelf(false, gainDb, f0, sr);

export const peak = (gainDb: number, f0: number, q: number, sr: number): Biquad => {
  const a = Math.pow(10, gainDb / 40);
  const w0 = (2 * Math.PI * f0) / sr;
  const cos = Math.cos(w0);
  const alph = Math.sin(w0) / (2 * q);
  const b0 = 1 + alph * a;
  const b1 = -2 * cos;
  const b2 = 1 - alph * a;
  const a0 = 1 + alph / a;
  const a2 = 1 - alph / a;
  return { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: b1 / a0, a2: a2 / a0 };
};

/** |H(e^jw)| in dB. */
export const magnitudeDb = (h: Biquad, f: number, sr: number): number => {
  const w = (2 * Math.PI * f) / sr;
  const m2 = (c0: number, c1: number, c2: number) => {
    const re = c0 + c1 * Math.cos(w) + c2 * Math.cos(2 * w);
    const im = c1 * Math.sin(w) + c2 * Math.sin(2 * w);
    return re * re + im * im;
  };
  return 10 * Math.log10(m2(h.b0, h.b1, h.b2) / m2(1, h.a1, h.a2));
};

export const voiceEqDb = (low: number, mid: number, high: number, f: number, sr: number): number =>
  magnitudeDb(lowShelf(low, 200, sr), f, sr) +
  magnitudeDb(peak(mid, 2000, 0.7, sr), f, sr) +
  magnitudeDb(highShelf(high, 8000, sr), f, sr);
