import { updateColors } from './nodes.js';
import { markDirty } from './renderer.js';
import { setMode as setInteractionMode } from './interaction.js';

let currentMode = 'architecture';

export function switchMode(mode) {
  if (mode === currentMode) return;
  currentMode = mode;
  updateColors(mode);
  setInteractionMode(mode);

  // Update button states
  document.querySelectorAll('.mode-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.mode === mode);
  });

  // Update legend
  document.getElementById('legend-arch').style.display =
    mode === 'architecture' ? 'block' : 'none';
  document.getElementById('legend-risk').style.display =
    mode === 'risk' ? 'block' : 'none';

  markDirty();
}

export function getCurrentMode() {
  return currentMode;
}
