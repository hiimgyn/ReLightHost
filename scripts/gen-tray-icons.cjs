// One-off generator for tray menu icons — reuses the same Ant Design icon
// paths already used across the frontend, rendered to small PNGs via sharp
// (already a devDependency) so the Rust tray code can load plain image
// files instead of needing an SVG renderer at runtime.
//
// Run: node scripts/gen-tray-icons.cjs
const fs = require('fs');
const path = require('path');
const sharp = require('sharp');

const OUT_DIR = path.join(__dirname, '..', 'src-tauri', 'icons', 'tray');
const SIZE = 32;
const BRAND = '#6367FF'; // matches --rh-primary in src/index.css

const icons = {
  'menu-show': require('@ant-design/icons-svg/lib/asn/BorderOutlined').default,
  'menu-mute-on': require('@ant-design/icons-svg/lib/asn/AudioOutlined').default,
  'menu-mute-off': require('@ant-design/icons-svg/lib/asn/AudioMutedOutlined').default,
  'menu-loopback-on': require('@ant-design/icons-svg/lib/asn/SoundFilled').default,
  'menu-loopback-off': require('@ant-design/icons-svg/lib/asn/SoundOutlined').default,
  'menu-audio-settings': require('@ant-design/icons-svg/lib/asn/SlidersOutlined').default,
  'menu-app-settings': require('@ant-design/icons-svg/lib/asn/SettingOutlined').default,
  'menu-exit': require('@ant-design/icons-svg/lib/asn/PoweroffOutlined').default,
};

function pathsOf(node) {
  const out = [];
  const walk = (n) => {
    if (n.tag === 'path' && n.attrs && n.attrs.d) out.push(n.attrs.d);
    (n.children || []).forEach(walk);
  };
  walk(node);
  return out;
}

async function main() {
  fs.mkdirSync(OUT_DIR, { recursive: true });
  for (const [name, def] of Object.entries(icons)) {
    const icon = def.icon;
    const viewBox = icon.attrs.viewBox || '0 0 1024 1024';
    const ds = pathsOf(icon);
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${viewBox}">${ds
      .map((d) => `<path d="${d}" fill="${BRAND}"/>`)
      .join('')}</svg>`;
    const outPath = path.join(OUT_DIR, `${name}.png`);
    await sharp(Buffer.from(svg)).resize(SIZE, SIZE).png().toFile(outPath);
    console.log('wrote', outPath);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
