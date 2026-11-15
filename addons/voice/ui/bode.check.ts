import { voiceEqDb } from './eqResponse.ts';

const near = (name: string, got: number, want: number, tol: number) => {
  console.log(`${name}: ${got.toFixed(4)}`);
  if (!(Math.abs(got - want) <= tol)) throw new Error(`FAIL ${name}: want ${want} +-${tol}`);
};
near('low +12 @100Hz', voiceEqDb(12, 0, 0, 100, 48000), 11.1028, 0.01);
near('low +12 @200Hz', voiceEqDb(12, 0, 0, 200, 48000), 6.0, 0.05);
near('high +12 @4kHz', voiceEqDb(0, 0, 12, 4000, 48000), 0.69, 0.05);
console.log('bode checks ok');
