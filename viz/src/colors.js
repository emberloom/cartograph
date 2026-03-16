/**
 * Shared color palettes for the visualization.
 */

// 16-color palette for directories (architecture mode) and owners (ownership mode)
// First 12 = Phase 1 architecture colors. Last 4 added for Phase 2 ownership.
export const DIR_COLORS = [
  '#58a6ff', '#3fb950', '#d2a8ff', '#ffa657',
  '#f78166', '#79c0ff', '#56d364', '#e3b341',
  '#ff7b72', '#a5d6ff', '#7ee787', '#ffa198',
  '#db6d28', '#388bfd', '#bc8cff', '#4ac26b',
];

export function dirColor(index) {
  return DIR_COLORS[index % DIR_COLORS.length];
}

/**
 * Map a riskScore (0–1) to a green→yellow→red color.
 * Returns [r, g, b] each in 0–1 range.
 */
export function riskColor(score) {
  const s = Math.max(0, Math.min(1, score));
  // green #3fb950 → yellow #e3b341 → red #ff6b6b
  if (s < 0.5) {
    const t = s * 2;
    return [
      (0x3f + (0xe3 - 0x3f) * t) / 255,
      (0xb9 + (0xb3 - 0xb9) * t) / 255,
      (0x50 + (0x41 - 0x50) * t) / 255,
    ];
  } else {
    const t = (s - 0.5) * 2;
    return [
      (0xe3 + (0xff - 0xe3) * t) / 255,
      (0xb3 + (0x6b - 0xb3) * t) / 255,
      (0x41 + (0x6b - 0x41) * t) / 255,
    ];
  }
}

// Background color
export const BG_COLOR = 0x0d1117;

// Fixed color for unowned files (muted gray)
export const UNOWNED_COLOR = '#484f58';

/**
 * DJB2 hash — maps a string to a non-negative integer.
 */
function djb2(str) {
  let hash = 5381;
  for (let i = 0; i < str.length; i++) {
    hash = (((hash << 5) + hash) + str.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}

/**
 * Map an owner name to a color from the 16-color palette.
 * "unowned" always returns UNOWNED_COLOR.
 * Two owners may share a color (hash collision) — acceptable given 16 colors + 10-owner legend cap.
 */
export function ownerColor(ownerName) {
  if (!ownerName || ownerName === 'unowned') return UNOWNED_COLOR;
  return DIR_COLORS[djb2(ownerName) % DIR_COLORS.length];
}
