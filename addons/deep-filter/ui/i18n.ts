const en = {
  deepFilter: {
    title: 'DeepFilterNet 3 Pro',
    subtitle: 'Deep Complex-Valued Spectrogram Filtering & ERB Denoising',
    badgeText: 'DEEP LEARNING SOTA',
    sampleRateMismatch: 'Sample Rate Mismatch',
    maxAttenuation: 'Max Attenuation',
    postFilterStrength: 'Post-filter Strength',
    mix: 'Clean / Dry Mix',
    outputGain: 'Output Trim',
    attenuationDb: 'Attenuation Limit',
    oscilloscopeTitle: 'Input vs Output Level (last 2 s)',
    vadConfidence: 'SPEECH ENERGY',
    footerDesc: 'Powered by DeepFilterNet3 — State-of-the-Art Deep Complex Spectrogram Filtering. Low latency with zero external dependencies.',
    sampleRateRequired: '48 kHz required, current: {rate} Hz',
  },
};

export const vi: { [S in keyof typeof en]: { [K in keyof (typeof en)[S]]: string } } = {
  deepFilter: {
    title: 'DeepFilterNet 3 Pro',
    subtitle: 'Lọc phổ phức hợp sâu (Complex Spectrogram) & Khử nhiễu ERB',
    badgeText: 'MẠNG NƠ-RON AI SOTA',
    sampleRateMismatch: 'Lệch tần số lấy mẫu',
    maxAttenuation: 'Giới hạn suy hao tối đa',
    postFilterStrength: 'Cường độ lọc hậu kỳ',
    mix: 'Tỷ lệ hòa trộn (Mix)',
    outputGain: 'Âm lượng đầu ra',
    attenuationDb: 'Mức suy giảm cực đại',
    oscilloscopeTitle: 'Mức đầu vào vs đầu ra (2 giây gần nhất)',
    vadConfidence: 'NĂNG LƯỢNG GIỌNG',
    footerDesc: 'Sử dụng DeepFilterNet3 — Công nghệ lọc phổ phức hợp học sâu hiện đại nhất thế giới. Độ trễ thấp, tích hợp hoàn toàn không phụ thuộc file ngoài.',
    sampleRateRequired: 'Yêu cầu 48 kHz, hiện tại: {rate} Hz',
  },
};

export default { en: en as { [S in keyof typeof en]: { [K in keyof (typeof en)[S]]: string } }, vi };
