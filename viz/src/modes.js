import { markDirty, updateRegionColors } from './renderer.js';
import { setMode as setInteractionMode } from './interaction.js';
import { regions, fileNodes } from './layout.js';
import { ownerColor } from './colors.js';
import * as THREE from 'three';

let currentMode = 'architecture';
let _ownershipRegionColors = null; // cached Map<name, THREE.Color>

/**
 * Compute plurality owner color per region (cached after first call).
 * Returns Map<regionName, THREE.Color>.
 */
function _computeOwnershipRegionColors() {
  if (_ownershipRegionColors) return _ownershipRegionColors;

  // Count owner occurrences per region name
  const ownerCounts = new Map(); // regionName -> {ownerName -> count}
  for (const fn of fileNodes) {
    // Find which region this file falls in (use topLevelDir as region identifier)
    const regionName = fn.topLevelDir || '';
    if (!regionName) continue;
    if (!ownerCounts.has(regionName)) ownerCounts.set(regionName, new Map());
    const counts = ownerCounts.get(regionName);
    counts.set(fn.owner, (counts.get(fn.owner) || 0) + 1);
  }

  _ownershipRegionColors = new Map();
  for (const r of regions) {
    const counts = ownerCounts.get(r.name);
    if (!counts || counts.size === 0) continue;
    // Plurality owner: highest count
    let maxCount = 0;
    let pluralityOwner = 'unowned';
    for (const [owner, count] of counts) {
      if (count > maxCount) { maxCount = count; pluralityOwner = owner; }
    }
    _ownershipRegionColors.set(r.name, new THREE.Color(ownerColor(pluralityOwner)));
  }
  return _ownershipRegionColors;
}

export function switchMode(mode) {
  if (mode === currentMode) return;
  currentMode = mode;
  setInteractionMode(mode);

  // Update button states
  document.querySelectorAll('.mode-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.mode === mode);
    btn.style.color = btn.dataset.mode === mode ? '#58a6ff' : '#8b949e';
  });

  // Update region colors
  if (mode === 'ownership') {
    updateRegionColors(_computeOwnershipRegionColors());
  } else {
    updateRegionColors(null); // reset to original directory colors
  }

  // Update legend
  document.getElementById('legend-arch').style.display =
    mode === 'architecture' ? 'block' : 'none';
  document.getElementById('legend-risk').style.display =
    mode === 'risk' ? 'block' : 'none';
  const legendOwnership = document.getElementById('legend-ownership');
  if (legendOwnership) {
    legendOwnership.style.display = mode === 'ownership' ? 'block' : 'none';
  }

  // Notify ui.js to refresh ownership legend content
  window.dispatchEvent(new CustomEvent('mode-changed', { detail: { mode } }));

  markDirty();
}

export function getCurrentMode() {
  return currentMode;
}
