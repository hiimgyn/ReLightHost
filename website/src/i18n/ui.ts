export const languages = { en: 'English', vi: 'Tiếng Việt' } as const;
export type Lang = keyof typeof languages;

export const REPO = 'https://github.com/hiimgyn/ReLightHost';
export const DOWNLOAD = `${REPO}/releases/latest`;

const en = {
  meta: {
    title: 'ReLightHost — Real-time VST3, VST2 & CLAP plugin host for Windows',
    description:
      'ReLightHost is a lightweight real-time audio plugin host built with Rust and Tauri. Chain VST3, VST2 and CLAP plugins, route live audio over ASIO/WASAPI, and clean up your voice with built-in AI noise suppression.',
  },
  nav: { features: 'Features', showcase: 'In action', processors: 'Built-in', tech: 'Under the hood', download: 'Download' },
  hero: {
    badge: 'Free for Windows · VST3 · VST2 · CLAP',
    title1: 'Your plugins.',
    title2: 'Live, in real time.',
    sub: 'ReLightHost is a lightweight audio plugin host built with Rust. Drop VST3, VST2 and CLAP plugins into a chain, route your mic through them, and go — for streaming, calls and recording.',
    cta: 'Download for Windows',
    github: 'View on GitHub',
    alt: 'ReLightHost main window with a signal chain and two native plugin editors open',
  },
  formats: 'Hosts every major plugin format natively',
  chain: {
    eyebrow: 'Signal chain',
    title: 'Drag, drop, done.',
    sub: 'Build a linear chain from input to output. Reorder with drag-and-drop, bypass, swap or rename any plugin — every change is saved automatically.',
    alt: 'The ReLightHost signal chain with Clear, Voice Designer, Pro-Q 4 and Pro-C 2',
  },
  features: {
    eyebrow: 'Features',
    title: 'Everything a live host needs. Nothing it doesn’t.',
    items: [
      { t: 'Plugin chain', d: 'Add, remove, reorder, swap, bypass and rename plugins in a clean linear chain.' },
      { t: 'Plugin library', d: 'Search, filter and group plugins by manufacturer before adding them.' },
      { t: 'Native plugin GUIs', d: 'Editors open in real native windows — no web or remote rendering.' },
      { t: 'Crash isolation', d: 'A VST3 that keeps crashing is reloaded in an isolated child process next time.' },
      { t: 'Audio routing', d: 'Choose input and output devices, monitor live audio, toggle hardware loopback.' },
      { t: 'Real-time meters', d: 'Live VU meter, latency, plugin count, CPU and RAM right in the footer.' },
      { t: 'Session restore', d: 'Your devices, chain and plugin state come back exactly as you left them.' },
      { t: 'System tray', d: 'Minimize to tray, start with Windows, and toggle mute from the tray menu.' },
    ],
  },
  showcase: {
    eyebrow: 'In action',
    title: 'Real plugins. Real windows.',
    sub: 'Open any plugin editor straight from its card. Native windows are embedded via Win32, so your favorite plugins look and feel exactly as they should.',
    proq: 'FabFilter Pro-Q 4 running inside ReLightHost',
    clear: 'Supertone Clear running inside ReLightHost',
  },
  processors: {
    eyebrow: 'Built-in processors',
    title: 'Clean voice out of the box.',
    sub: 'No extra plugins required. ReLightHost ships with AI and DSP processors, each with a pro-audio GUI.',
    items: [
      { t: 'DeepFilterNet 3', d: 'State-of-the-art AI speech denoiser on a dedicated MMCSS Pro Audio thread.' },
      { t: 'RNNoise', d: 'Lightweight neural noise suppression with zero external models.' },
      { t: 'Compressor', d: 'Interactive transfer curve and metal rotary dials with fine-tuning.' },
      { t: 'Voice Designer', d: '3-band EQ with Bode plots and dual oscilloscopes to shape your tone.' },
    ],
  },
  tech: {
    eyebrow: 'Under the hood',
    title: 'Built in Rust, down to the ABI.',
    sub: 'VST3, VST2 and CLAP are hosted through raw FFI against each format’s native ABI — no host-side SDK dependency. Audio runs on lock-free buffers over ASIO or WASAPI.',
    stats: [
      { v: 'Rust', l: 'Audio engine' },
      { v: '3', l: 'Formats via raw FFI' },
      { v: 'ASIO', l: 'Low-latency I/O' },
      { v: 'Tauri 2', l: 'Native shell' },
    ],
  },
  cta: {
    title: 'Ready to light up your sound?',
    sub: 'Free download for Windows 10/11. Source available on GitHub.',
    note: 'Windows only for now · GPL-3.0',
  },
  footer: { made: 'Made by HiimGyn', lang: 'Language' },
};

const vi: typeof en = {
  meta: {
    title: 'ReLightHost — Trình host plugin VST3, VST2 & CLAP thời gian thực cho Windows',
    description:
      'ReLightHost là trình host plugin âm thanh thời gian thực, nhẹ, viết bằng Rust và Tauri. Xâu chuỗi plugin VST3, VST2, CLAP, định tuyến âm thanh trực tiếp qua ASIO/WASAPI và lọc ồn giọng nói bằng AI tích hợp sẵn.',
  },
  nav: { features: 'Tính năng', showcase: 'Thực tế', processors: 'Tích hợp sẵn', tech: 'Công nghệ', download: 'Tải về' },
  hero: {
    badge: 'Miễn phí cho Windows · VST3 · VST2 · CLAP',
    title1: 'Plugin của bạn.',
    title2: 'Chạy trực tiếp, thời gian thực.',
    sub: 'ReLightHost là trình host plugin âm thanh gọn nhẹ viết bằng Rust. Kéo thả plugin VST3, VST2, CLAP vào chuỗi xử lý, cho mic đi qua và dùng ngay — cho livestream, gọi điện và thu âm.',
    cta: 'Tải cho Windows',
    github: 'Xem trên GitHub',
    alt: 'Cửa sổ chính ReLightHost với chuỗi xử lý tín hiệu và hai giao diện plugin gốc đang mở',
  },
  formats: 'Host trực tiếp mọi định dạng plugin phổ biến',
  chain: {
    eyebrow: 'Chuỗi tín hiệu',
    title: 'Kéo, thả, xong.',
    sub: 'Xây dựng chuỗi xử lý tuyến tính từ đầu vào đến đầu ra. Sắp xếp bằng kéo thả, bypass, thay thế hay đổi tên plugin — mọi thay đổi đều được lưu tự động.',
    alt: 'Chuỗi tín hiệu ReLightHost với Clear, Voice Designer, Pro-Q 4 và Pro-C 2',
  },
  features: {
    eyebrow: 'Tính năng',
    title: 'Đủ mọi thứ một trình host live cần. Không thừa gì.',
    items: [
      { t: 'Chuỗi plugin', d: 'Thêm, xoá, sắp xếp, thay thế, bypass và đổi tên plugin trong chuỗi tuyến tính.' },
      { t: 'Thư viện plugin', d: 'Tìm kiếm, lọc và nhóm plugin theo nhà sản xuất trước khi thêm vào chuỗi.' },
      { t: 'Giao diện plugin gốc', d: 'Mở editor trong cửa sổ native thật — không render qua web hay từ xa.' },
      { t: 'Cô lập crash', d: 'Plugin VST3 hay crash sẽ tự chạy trong tiến trình con riêng ở lần mở sau.' },
      { t: 'Định tuyến âm thanh', d: 'Chọn thiết bị vào/ra, monitor âm thanh trực tiếp, bật tắt loopback phần cứng.' },
      { t: 'Đồng hồ thời gian thực', d: 'VU meter, độ trễ, số plugin, CPU và RAM hiển thị ngay ở thanh dưới.' },
      { t: 'Khôi phục phiên', d: 'Thiết bị, chuỗi plugin và trạng thái plugin được khôi phục y như lúc bạn rời đi.' },
      { t: 'Khay hệ thống', d: 'Thu nhỏ xuống khay, khởi động cùng Windows, tắt tiếng ngay từ menu khay.' },
    ],
  },
  showcase: {
    eyebrow: 'Thực tế',
    title: 'Plugin thật. Cửa sổ thật.',
    sub: 'Mở editor của bất kỳ plugin nào ngay từ thẻ của nó. Cửa sổ native được nhúng qua Win32, nên plugin quen thuộc của bạn hiển thị và hoạt động đúng như vốn có.',
    proq: 'FabFilter Pro-Q 4 chạy trong ReLightHost',
    clear: 'Supertone Clear chạy trong ReLightHost',
  },
  processors: {
    eyebrow: 'Bộ xử lý tích hợp',
    title: 'Giọng nói sạch ngay từ đầu.',
    sub: 'Không cần thêm plugin nào. ReLightHost đi kèm các bộ xử lý AI và DSP, mỗi bộ có giao diện chuẩn pro-audio.',
    items: [
      { t: 'DeepFilterNet 3', d: 'Bộ lọc ồn giọng nói AI hàng đầu, chạy trên luồng MMCSS Pro Audio riêng.' },
      { t: 'RNNoise', d: 'Lọc ồn bằng mạng nơ-ron gọn nhẹ, không cần model bên ngoài.' },
      { t: 'Compressor', d: 'Đường cong nén tương tác và núm xoay kim loại tinh chỉnh chính xác.' },
      { t: 'Voice Designer', d: 'EQ 3 dải với biểu đồ Bode và dao động ký kép để tạo chất giọng.' },
    ],
  },
  tech: {
    eyebrow: 'Công nghệ',
    title: 'Viết bằng Rust, tới tận tầng ABI.',
    sub: 'VST3, VST2 và CLAP được host qua FFI thuần trên ABI gốc của từng định dạng — không phụ thuộc SDK phía host. Âm thanh chạy trên bộ đệm lock-free qua ASIO hoặc WASAPI.',
    stats: [
      { v: 'Rust', l: 'Engine âm thanh' },
      { v: '3', l: 'Định dạng qua FFI thuần' },
      { v: 'ASIO', l: 'I/O độ trễ thấp' },
      { v: 'Tauri 2', l: 'Vỏ ứng dụng native' },
    ],
  },
  cta: {
    title: 'Sẵn sàng thắp sáng âm thanh của bạn?',
    sub: 'Tải miễn phí cho Windows 10/11. Mã nguồn công khai trên GitHub.',
    note: 'Hiện chỉ hỗ trợ Windows · GPL-3.0',
  },
  footer: { made: 'Phát triển bởi HiimGyn', lang: 'Ngôn ngữ' },
};

export const ui = { en, vi };
