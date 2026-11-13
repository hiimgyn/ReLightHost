/**
 * Every particle is one sample of sound. Scroll drives three shapes the same particles morph between:
 *   0 → a spectrogram: rows of live waveforms rolling back into the distance,
 *   1 → the signal chain: input → four plugin cards → output, with signal pulses running down the cables,
 *   2 → a rotary knob: tick scale, value arc and pointer, ringed by a circular spectrum.
 * One Points draw call; the morph (and all the motion) happens in the vertex shader.
 * Stages are reached when the elements marked data-stage="1" / "2" reach mid-screen.
 */

const VERT = /* glsl */ `
  uniform float uTime, uProgress, uPixelRatio, uSize, uScaleB, uScaleC;
  uniform vec2 uKnobOff;
  attribute vec3 aChain, aKnob, aColA, aColB, aColC;
  attribute vec2 aKind; // x: 1 = cable particle (pulses run along it), y: >= 0 = fraction along a spectrum bar
  attribute float aRand;
  varying vec3 vColor;
  varying float vFade;

  float wave(vec3 p) {
    float x = p.x, z = p.z, t = uTime;
    float env = exp(-x * x * 0.045);
    float w = sin(x * 1.6 + t * 1.4 + z * 0.9) * 0.45 + sin(x * 3.3 - t * 2.2 + z * 1.9) * 0.22 + sin(x * 0.7 + t * 0.8 - z * 0.5) * 0.4;
    return env * w * (0.55 + 0.45 * sin(z * 0.9 - t * 0.9));
  }

  void main() {
    float d = aRand * 0.35;
    float tAB = smoothstep(0.0, 1.0, clamp((uProgress - d) / 0.65, 0.0, 1.0));
    float tBC = smoothstep(0.0, 1.0, clamp((uProgress - 1.0 - d) / 0.65, 0.0, 1.0));

    vec3 a = position;
    float h = wave(a);
    a.y += h;

    vec3 b = aChain * uScaleB;
    b.y += sin(aChain.x * 0.8 + uTime * 1.1) * 0.03;

    vec3 c = aKnob;
    if (aKind.y >= 0.0) {
      // circular spectrum: each bar breathes with its own band
      float ang = atan(aKnob.y, aKnob.x);
      float len = 0.12 + 0.5 * pow(0.5 + 0.5 * sin(ang * 5.0 + uTime * 2.3) * sin(ang * 3.0 - uTime * 1.3 + 1.7), 1.5);
      c.xy += normalize(aKnob.xy) * aKind.y * len;
    }
    c = c * uScaleC + vec3(uKnobOff, 0.0) + vec3(0.0, sin(uTime * 0.8) * 0.04, 0.0);

    vec3 p = mix(a, b, tAB);
    p = mix(p, c, tBC);
    // particles arc upward while travelling, so the morph reads as motion, not a crossfade
    p.y += (sin(tAB * 3.14159) + sin(tBC * 3.14159)) * (0.4 + aRand * 1.4);

    vec3 colA = aColA * (0.75 + abs(h) * 1.1) + vec3(0.35, 0.25, 0.4) * smoothstep(0.35, 0.8, abs(h));
    float pulse = aKind.x * pow(0.5 + 0.5 * sin(aChain.x * 2.2 - uTime * 4.0), 6.0);
    vec3 colB = aColB + vec3(0.8, 0.7, 1.0) * pulse;
    vColor = mix(mix(colA, colB, tAB), aColC, tBC);

    vec4 mv = modelViewMatrix * vec4(p, 1.0);
    gl_Position = projectionMatrix * mv;
    float size = mix(mix(0.9 + aRand * 1.2, 1.1 + pulse * 0.8, tAB), 1.0, tBC);
    gl_PointSize = uSize * size * uPixelRatio * (6.0 / -mv.z);
    vFade = clamp(1.0 - (-mv.z - 6.0) / 14.0, 0.06, 1.0); // far rows of the spectrogram fade out
  }
`;

const FRAG = /* glsl */ `
  uniform float uGlow;
  varying vec3 vColor;
  varying float vFade;
  void main() {
    float d = length(gl_PointCoord - 0.5);
    float a = smoothstep(0.5, 0.0, d);
    gl_FragColor = vec4(vColor * (0.55 + a), a * a * vFade * uGlow);
  }
`;

/** Full-screen nebula behind the particles: domain-warped fbm, tinted per stage. */
const BG_VERT = /* glsl */ `void main() { gl_Position = vec4(position.xy, 0.0, 1.0); }`;
const BG_FRAG = /* glsl */ `
  uniform float uTime, uProgress;
  uniform vec2 uRes, uMouse;
  float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
  float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p), u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
  }
  float fbm(vec2 p) {
    float v = 0.0, a = 0.5;
    for (int i = 0; i < 4; i++) { v += a * noise(p); p = p * 2.03 + vec2(1.7, 9.2); a *= 0.5; }
    return v;
  }
  void main() {
    vec2 uv = (gl_FragCoord.xy - 0.5 * uRes) / uRes.y + uMouse * 0.04;
    float t = uTime * 0.035;
    vec2 q = vec2(fbm(uv * 1.5 + t), fbm(uv * 1.5 - t + 3.1));
    float n = fbm(uv * 1.3 + q * 1.9 + vec2(t * 0.6, -t * 0.4));
    float s1 = clamp(uProgress, 0.0, 1.0), s2 = clamp(uProgress - 1.0, 0.0, 1.0);
    // spectrogram: violet + magenta · chain: indigo + cyan · knob: violet + rose
    vec3 ca = mix(mix(vec3(0.52, 0.24, 0.95), vec3(0.32, 0.28, 0.95), s1), vec3(0.55, 0.26, 0.95), s2);
    vec3 cb = mix(mix(vec3(0.92, 0.32, 0.72), vec3(0.12, 0.58, 0.86), s1), vec3(0.95, 0.42, 0.62), s2);
    vec3 col = mix(ca, cb, smoothstep(0.35, 0.75, q.x));
    float vign = 1.0 - smoothstep(0.15, 1.35, length(uv * vec2(0.75, 1.0)));
    float neb = smoothstep(0.3, 0.62, n);
    float wisp = pow(smoothstep(0.42, 0.72, fbm(uv * 3.2 + q * 2.5 - t)), 2.0); // brighter filaments
    vec3 c = vec3(0.027, 0.039, 0.078) + col * (neb * 0.62 + wisp * 0.7) * vign;
    c += ca * 0.2 * exp(-abs(uv.y + 0.2) * 3.0) * vign; // soft horizon glow
    c = c / (1.0 + c * 0.6); // keep highlights from clipping
    gl_FragColor = vec4(c, 1.0);
  }
`;

/** Far star field: tiny points on a big sphere, each twinkling at its own pace. */
const STAR_VERT = /* glsl */ `
  uniform float uTime, uPixelRatio;
  attribute float aPhase;
  varying float vA;
  void main() {
    vec4 mv = modelViewMatrix * vec4(position, 1.0);
    gl_Position = projectionMatrix * mv;
    gl_PointSize = (1.0 + fract(aPhase * 7.3) * 1.8) * uPixelRatio;
    vA = 0.35 + 0.65 * (0.5 + 0.5 * sin(uTime * (0.6 + fract(aPhase * 3.1) * 1.8) + aPhase * 6.28));
  }
`;
const STAR_FRAG = /* glsl */ `
  varying float vA;
  void main() {
    float d = length(gl_PointCoord - 0.5);
    gl_FragColor = vec4(vec3(0.88, 0.86, 1.0), smoothstep(0.5, 0.0, d) * vA);
  }
`;

type RGB = [number, number, number];
const VIOLET: RGB = [0.62, 0.4, 1.0];
const LILAC: RGB = [0.5, 0.46, 0.8];
const GREEN: RGB = [0.25, 0.85, 0.55];
const SKY: RGB = [0.35, 0.72, 1.0];
const AMBER: RGB = [1.0, 0.68, 0.25];
const DIM: RGB = [0.36, 0.36, 0.6];
const lerp3 = (a: RGB, b: RGB, t: number): RGB => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];

/** A random point on the outline of a rounded rectangle centred at (cx, cy). */
function roundRect(cx: number, cy: number, w: number, h: number, r: number): [number, number] {
  const straight = 2 * (w - 2 * r) + 2 * (h - 2 * r), arc = 2 * Math.PI * r;
  let e = Math.random() * (straight + arc);
  const sw = w - 2 * r, sh = h - 2 * r;
  if (e < sw) return [cx - sw / 2 + e, cy + h / 2];
  if ((e -= sw) < sw) return [cx - sw / 2 + e, cy - h / 2];
  if ((e -= sw) < sh) return [cx - w / 2, cy - sh / 2 + e];
  if ((e -= sh) < sh) return [cx + w / 2, cy - sh / 2 + e];
  e -= sh;
  const a = (e / arc) * Math.PI * 2, q = Math.floor(a / (Math.PI / 2));
  const ox = q === 0 || q === 3 ? sw / 2 : -sw / 2, oy = q < 2 ? sh / 2 : -sh / 2;
  return [cx + ox + Math.cos(a) * r, cy + oy + Math.sin(a) * r];
}

function build(n: number) {
  const pos = new Float32Array(n * 3), chain = new Float32Array(n * 3), knob = new Float32Array(n * 3);
  const colA = new Float32Array(n * 3), colB = new Float32Array(n * 3), colC = new Float32Array(n * 3);
  const kind = new Float32Array(n * 2), rand = new Float32Array(n);
  const R = Math.random;
  const gauss = () => (R() + R() + R() - 1.5) / 1.5;

  // Spectrogram: rows of waveform lines, from z = 1 (near) back to z = -10.
  const ROWS = 34;

  // Chain: input node, four plugin cards, output node, left → right, standing upright.
  const CW = 1.42, CH = 1.12, GAP = 0.62, NODE = 0.36, Y = -1.05, Z = 1.2;
  const cards = [-1.5, -0.5, 0.5, 1.5].map((k) => k * (CW + GAP));
  const inX = cards[0] - CW / 2 - GAP - NODE, outX = -inX;
  const leds = [AMBER, GREEN, AMBER, GREEN];
  const ends = [inX + NODE, ...cards.flatMap((x) => [x - CW / 2, x + CW / 2]), outX - NODE]; // cable endpoints, pairwise

  // Knob: 270° sweep from -135° (7 o'clock) clockwise, value at +60°.
  const A0 = (-135 * Math.PI) / 180, A1 = (135 * Math.PI) / 180, VAL = (60 * Math.PI) / 180;
  const polar = (r: number, a: number): [number, number] => [Math.sin(a) * r, Math.cos(a) * r];

  for (let i = 0; i < n; i++) {
    const k = i * 3;
    rand[i] = R();
    kind[i * 2 + 1] = -1;

    // Spectrogram.
    const row = i % ROWS, x = (R() - 0.5) * 16;
    pos[k] = x; pos[k + 1] = gauss() * 0.012; pos[k + 2] = 1.0 - row * 0.34;
    colA.set(lerp3(lerp3([0.4, 0.62, 1.0], VIOLET, Math.min(1, (x + 8) / 8)), [1.0, 0.45, 0.78], Math.max(0, x / 8)), k);

    // Chain.
    const u = R();
    let cx = 0, cy = 0, c: RGB = VIOLET;
    if (u < 0.5) {
      const j = Math.floor(R() * 4), x0 = cards[j], v = R();
      if (v < 0.58) [cx, cy] = roundRect(x0, Y, CW, CH, 0.14); // card outline
      else if (v < 0.76) { cx = x0 - CW / 2 + 0.16 + R() * (0.45 + j * 0.08); cy = Y + CH / 2 - 0.2; c = [0.85, 0.85, 1.0]; } // name
      else if (v < 0.83) { const a = R() * 6.283, r = Math.sqrt(R()) * 0.045; cx = x0 + CW / 2 - 0.22 + Math.cos(a) * r; cy = Y + CH / 2 - 0.2 + Math.sin(a) * r; c = leds[j]; } // status LED
      else { const a = R() * 6.283; cx = x0 - 0.3 + Math.cos(a) * 0.2; cy = Y - 0.18 + Math.sin(a) * 0.2; c = LILAC; if (R() < 0.3) { cx = x0 + 0.15 + R() * 0.38; cy = Y - 0.18 + gauss() * 0.02; } } // knob + button
    } else if (u < 0.66) {
      const out = R() < 0.5, x0 = out ? outX : inX, a = R() * 6.283, r = R() < 0.75 ? NODE : Math.sqrt(R()) * 0.1;
      cx = x0 + Math.cos(a) * r; cy = Y + Math.sin(a) * r; c = out ? SKY : GREEN;
    } else {
      const seg = Math.floor(R() * 5), xa = ends[seg * 2], xb = ends[seg * 2 + 1], v = R();
      if (v < 0.82) { cx = xa + R() * (xb - xa); cy = Y + gauss() * 0.012; c = DIM; kind[i * 2] = 1; }
      else { const s = R() * 0.13, mid = (xa + xb) / 2 + 0.06; cx = mid - s; cy = Y + (R() < 0.5 ? s : -s); c = LILAC; } // arrowhead
    }
    chain[k] = cx; chain[k + 1] = cy; chain[k + 2] = Z + gauss() * 0.02;
    colB.set(c, k);

    // Knob.
    const w = R();
    let kx = 0, ky = 0, kc: RGB = VIOLET;
    if (w < 0.14) {
      const t = Math.floor(R() * 28), a = A0 + (t / 27) * (A1 - A0), major = t % 9 === 0;
      [kx, ky] = polar(1.38 + R() * (major ? 0.2 : 0.11), a); kc = a <= VAL ? [0.85, 0.82, 1.0] : DIM;
    } else if (w < 0.38) {
      const a = A0 + R() * (VAL - A0); [kx, ky] = polar(1.18 + gauss() * 0.02, a);
      kc = lerp3(VIOLET, [1.0, 0.5, 0.8], (a - A0) / (VAL - A0)); // value arc
    } else if (w < 0.44) {
      const a = VAL + R() * (A1 - VAL); [kx, ky] = polar(1.18, a); kc = DIM; // remaining track
    } else if (w < 0.56) {
      [kx, ky] = polar(0.92, R() * 6.283); kc = LILAC; // knob body
    } else if (w < 0.62) {
      [kx, ky] = polar(0.25 + R() * 0.55, VAL); kc = [1, 1, 1]; // pointer
    } else {
      const bars = 96, b = Math.floor(R() * bars), a = (b / bars) * 6.283, f = R();
      [kx, ky] = polar(1.74, a); kind[i * 2 + 1] = f;
      kc = lerp3(lerp3(VIOLET, SKY, 0.5 + 0.5 * Math.sin(a)), [1, 1, 1], f * 0.25);
    }
    knob[k] = kx; knob[k + 1] = ky; knob[k + 2] = 1.2;
    colC.set(kc, k);
  }
  return { pos, chain, knob, colA, colB, colC, kind, rand };
}

export function mountScene(el: HTMLElement) {
  let dispose = () => {};
  let cancelled = false;

  import('three').then((THREE) => {
    if (cancelled) return;
    let renderer: InstanceType<typeof THREE.WebGLRenderer>;
    try {
      renderer = new THREE.WebGLRenderer({ antialias: false, alpha: true, powerPreference: 'high-performance' });
    } catch {
      return; // no WebGL: the page still works, just without the scene
    }
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    const mobile = window.innerWidth < 768;
    renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    el.appendChild(renderer.domElement);

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(50, 1, 0.1, 100);

    const d = build(mobile ? 7000 : 16000);
    const geo = new THREE.BufferGeometry();
    geo.setAttribute('position', new THREE.BufferAttribute(d.pos, 3));
    geo.setAttribute('aChain', new THREE.BufferAttribute(d.chain, 3));
    geo.setAttribute('aKnob', new THREE.BufferAttribute(d.knob, 3));
    geo.setAttribute('aColA', new THREE.BufferAttribute(d.colA, 3));
    geo.setAttribute('aColB', new THREE.BufferAttribute(d.colB, 3));
    geo.setAttribute('aColC', new THREE.BufferAttribute(d.colC, 3));
    geo.setAttribute('aKind', new THREE.BufferAttribute(d.kind, 2));
    geo.setAttribute('aRand', new THREE.BufferAttribute(d.rand, 1));
    const uniforms = {
      uTime: { value: 0 },
      uProgress: { value: 0 },
      uPixelRatio: { value: renderer.getPixelRatio() },
      uSize: { value: mobile ? 5.5 : 6.5 },
      uGlow: { value: 1 },
      uKnobOff: { value: new THREE.Vector2() }, // knob sits right of the copy, or above it on mobile (set in resize)
      uScaleB: { value: 1 }, // mobile: chain and knob shrink to fit a narrow screen
      uScaleC: { value: 1 },
    };
    const mat = new THREE.ShaderMaterial({
      vertexShader: VERT,
      fragmentShader: FRAG,
      uniforms,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
    });
    const points = new THREE.Points(geo, mat);
    points.frustumCulled = false;
    scene.add(points);

    const bgUniforms = { uTime: uniforms.uTime, uProgress: uniforms.uProgress, uRes: { value: new THREE.Vector2() }, uMouse: { value: new THREE.Vector2() } };
    const bgMat = new THREE.ShaderMaterial({ vertexShader: BG_VERT, fragmentShader: BG_FRAG, uniforms: bgUniforms, depthTest: false, depthWrite: false });
    const bgGeo = new THREE.PlaneGeometry(2, 2);
    const bg = new THREE.Mesh(bgGeo, bgMat);
    bg.frustumCulled = false;
    bg.renderOrder = -1;
    scene.add(bg);

    const starCount = mobile ? 900 : 2200;
    const starPos = new Float32Array(starCount * 3), starPhase = new Float32Array(starCount);
    for (let i = 0; i < starCount; i++) {
      const u = Math.random() * 2 - 1, th = Math.random() * Math.PI * 2, r = 35 + Math.random() * 15;
      const c = Math.sqrt(1 - u * u);
      starPos.set([Math.cos(th) * c * r, u * r, Math.sin(th) * c * r], i * 3);
      starPhase[i] = Math.random();
    }
    const starGeo = new THREE.BufferGeometry();
    starGeo.setAttribute('position', new THREE.BufferAttribute(starPos, 3));
    starGeo.setAttribute('aPhase', new THREE.BufferAttribute(starPhase, 1));
    const starMat = new THREE.ShaderMaterial({
      vertexShader: STAR_VERT,
      fragmentShader: STAR_FRAG,
      uniforms: { uTime: uniforms.uTime, uPixelRatio: uniforms.uPixelRatio },
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
    });
    const stars = new THREE.Points(starGeo, starMat);
    scene.add(stars);

    const resize = () => {
      const w = el.clientWidth, h = el.clientHeight, narrow = w < 768;
      uniforms.uKnobOff.value.set(narrow ? 0 : 2.3, narrow ? 2.5 : 0);
      uniforms.uScaleB.value = narrow ? 0.46 : 0.86;
      uniforms.uScaleC.value = narrow ? 0.7 : 1;
      renderer.setSize(w, h, false);
      renderer.getDrawingBufferSize(bgUniforms.uRes.value);
      camera.aspect = w / h;
      camera.updateProjectionMatrix();
    };
    resize();
    window.addEventListener('resize', resize);

    const mouse = { x: 0, y: 0 };
    const onMove = (e: PointerEvent) => {
      mouse.x = e.clientX / window.innerWidth - 0.5;
      mouse.y = e.clientY / window.innerHeight - 0.5;
    };
    window.addEventListener('pointermove', onMove);

    const anchor = (n: number) => {
      const a = document.querySelector<HTMLElement>(`[data-stage="${n}"]`);
      return a ? a.getBoundingClientRect().top + window.scrollY : document.documentElement.scrollHeight * (n / 2);
    };
    const scrollTarget = () => {
      const y = window.scrollY + window.innerHeight * 0.5;
      const a1 = anchor(1), a2 = anchor(2);
      return y < a1 ? Math.max(0, y - window.innerHeight * 0.5) / Math.max(1, a1 - window.innerHeight * 0.5) : 1 + Math.min(1, (y - a1) / Math.max(1, a2 - a1));
    };

    let last = performance.now();
    let raf = 0;
    const look = new THREE.Vector3();
    const tick = () => {
      const now = performance.now();
      const dt = Math.min((now - last) / 1000, 0.05);
      last = now;
      if (!reduce) uniforms.uTime.value += dt;
      bgUniforms.uMouse.value.set(mouse.x, -mouse.y);
      stars.rotation.y = uniforms.uTime.value * 0.006;
      const target = scrollTarget();
      uniforms.uProgress.value = reduce ? target : uniforms.uProgress.value + (target - uniforms.uProgress.value) * Math.min(1, dt * 3.5);

      const p = uniforms.uProgress.value;
      // Full brightness only near a stage; in between (feature sections) particles fade so text stays readable.
      const near = Math.max(...[0, 1, 2].map((k) => 1 - Math.min(1, Math.max(0, (Math.abs(p - k) - 0.03) / 0.12))));
      uniforms.uGlow.value = 0.16 + 0.84 * near;
      const far = window.innerWidth < 768 ? 1.45 : 1;
      // spectrogram from above its near edge, chain and knob straight on
      const cy = p < 1 ? 2.6 - p * 1.6 : 1.0 - (p - 1) * 0.6;
      const cz = (p < 1 ? 7.5 - p * 0.3 : 7.2 + (p - 1) * 1.8) * far;
      camera.position.x += (mouse.x * 1.2 - camera.position.x) * 0.05;
      camera.position.y += (cy - mouse.y * 0.6 - camera.position.y) * 0.08;
      camera.position.z += (cz - camera.position.z) * 0.08;
      look.set(0, p < 1 ? -0.8 + p * 0.5 : -0.3 + (p - 1) * 0.3, 0); // wave above the hero copy, chain below mid-screen
      camera.lookAt(look);

      renderer.render(scene, camera);
      raf = requestAnimationFrame(tick);
    };
    camera.position.set(0, 2.6, 7.5);
    tick();

    dispose = () => {
      cancelAnimationFrame(raf);
      window.removeEventListener('resize', resize);
      window.removeEventListener('pointermove', onMove);
      geo.dispose();
      bgGeo.dispose();
      bgMat.dispose();
      starGeo.dispose();
      starMat.dispose();
      mat.dispose();
      renderer.dispose();
      renderer.domElement.remove();
    };
  });

  return () => {
    cancelled = true;
    dispose();
  };
}
