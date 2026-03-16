import * as THREE from 'three';
import { scene, markDirty, startContinuousAnimation, stopContinuousAnimation, registerTickCallback, unregisterTickCallback } from './renderer.js';
import { dirColor, riskColor, ownerColor } from './colors.js';

let instancedMesh = null;
let nodeCount = 0;
let fileNodesRef = [];

// Temp objects for matrix/color manipulation (avoid allocations in hot paths)
const _matrix = new THREE.Matrix4();
const _position = new THREE.Vector3();
const _quaternion = new THREE.Quaternion();
const _scale = new THREE.Vector3();
const _color = new THREE.Color();
const _color2 = new THREE.Color();

// ── Ripple / Pulse state ──
let _rippleActive = false;
let _rippleNodesByDepth = {}; // { 1: [id,...], 2: [id,...], 3: [id,...] }
let _rippleSelectedId = null;
let _rippleStartTime = 0;
let _pulseTime = 0;

// ── Scrubber state ──
let _scrubberMask = null; // Set<number> | null
let _filterMask = null; // Set<number> | null — computed by filters.js
let _currentMode = 'architecture';

const BLAST_COLORS = ['#ff6b6b', '#ff9f43', '#ffd93d'];
const RIPPLE_DEPTH_DELAY = 300;  // ms between each depth wave
const RIPPLE_WAVE_DURATION = 200; // ms per wave animation

/**
 * Radius for a file node based on degree.
 */
function nodeRadius(degree) {
  return Math.max(3, Math.min(16, 2 + Math.sqrt(degree) * 2.5));
}

/**
 * Create instanced mesh for all file nodes.
 * @param {Array} fileNodes - from layout.js
 */
export function createNodes(fileNodes) {
  // INVARIANT: fileNodes must be sorted by id with ids 0..N-1 (contiguous).
  // All ripple/pulse hot paths use fileNodesRef[id] as a direct O(1) index.
  // layout.js guarantees this; do not call createNodes with reindexed data.
  fileNodesRef = fileNodes;
  nodeCount = fileNodes.length;

  const geo = new THREE.CircleGeometry(1, 16);
  const mat = new THREE.MeshBasicMaterial({
    vertexColors: false,
    transparent: true,
    opacity: 0.85,
  });

  instancedMesh = new THREE.InstancedMesh(geo, mat, nodeCount);
  instancedMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);

  for (let i = 0; i < nodeCount; i++) {
    const fn = fileNodes[i];
    const r = nodeRadius(fn.degree);
    _position.set(fn.x, -fn.y, 0);
    _scale.set(r, r, 1);
    _matrix.compose(_position, _quaternion, _scale);
    instancedMesh.setMatrixAt(i, _matrix);
    _color.set(dirColor(fn.topLevelDirIdx));
    instancedMesh.setColorAt(i, _color);
  }

  instancedMesh.instanceMatrix.needsUpdate = true;
  instancedMesh.instanceColor.needsUpdate = true;
  scene.add(instancedMesh);
  markDirty();
}

/**
 * Update all node colors for a given mode.
 * Stores mode for use by clearRipple and setScrubberMask.
 * @param {'architecture' | 'risk' | 'ownership'} mode
 */
export function updateColors(mode) {
  _currentMode = mode;
  if (!instancedMesh) return;

  for (let i = 0; i < nodeCount; i++) {
    const fn = fileNodesRef[i];
    if (mode === 'risk') {
      const [r, g, b] = riskColor(fn.riskScore);
      _color.setRGB(r, g, b);
    } else if (mode === 'ownership') {
      _color.set(ownerColor(fn.owner));
    } else {
      _color.set(dirColor(fn.topLevelDirIdx));
    }
    // Apply brightness masks (scrubber + unified filter compose: fail either → dim)
    const shouldDim =
      (_scrubberMask !== null && !_scrubberMask.has(fn.id)) ||
      (_filterMask !== null && !_filterMask.has(fn.id));
    if (shouldDim) {
      _color.multiplyScalar(0.2);
    }
    instancedMesh.setColorAt(i, _color);
  }

  instancedMesh.instanceColor.needsUpdate = true;
  markDirty();
}

// ── Ripple tick (Phase A: per-depth wave animation) ──
function _rippleTick(dt) {
  const elapsed = performance.now() - _rippleStartTime;
  let allDone = true;

  for (let d = 1; d <= 3; d++) {
    const ids = _rippleNodesByDepth[d];
    if (!ids || ids.length === 0) continue;

    const startMs = (d - 1) * RIPPLE_DEPTH_DELAY;
    const prog = Math.max(0, Math.min(1, (elapsed - startMs) / RIPPLE_WAVE_DURATION));
    if (prog < 1) allDone = false;

    _color2.set(BLAST_COLORS[d - 1]);
    _color.set('#1a1e24');

    for (const id of ids) {
      // fileNodesRef is sorted by id so index === id
      _color.set('#1a1e24').lerp(_color2, prog);
      instancedMesh.setColorAt(id, _color);
    }
  }

  // Selected node stays white throughout
  instancedMesh.setColorAt(_rippleSelectedId, new THREE.Color('#ffffff'));
  instancedMesh.instanceColor.needsUpdate = true;

  if (allDone) {
    unregisterTickCallback(_rippleTick);
    _pulseTime = 0;
    registerTickCallback(_pulseTick);
  }
}

// ── Pulse tick (Phase B: continuous oscillation) ──
const PULSE_FREQS = [1.5, 1.2, 0.9, 0.6]; // index = depth: 0=selected(1.5Hz), 1=depth-1(1.2Hz), 2=depth-2(0.9Hz), 3=depth-3(0.6Hz)

function _pulseTick(dt) {
  _pulseTime += dt;

  // Selected node: white base, 1.5Hz
  {
    const id = _rippleSelectedId;
    const bright = 1.0 + 0.15 * Math.sin(_pulseTime * 2 * Math.PI * PULSE_FREQS[0] + id * 1.3);
    _color.set('#ffffff').multiplyScalar(Math.max(0, bright));
    instancedMesh.setColorAt(id, _color);
  }

  for (let d = 1; d <= 3; d++) {
    const ids = _rippleNodesByDepth[d];
    if (!ids || ids.length === 0) continue;
    const freq = PULSE_FREQS[d];
    _color2.set(BLAST_COLORS[d - 1]);

    for (const id of ids) {
      const bright = 1.0 + 0.15 * Math.sin(_pulseTime * 2 * Math.PI * freq + id * 1.3);
      _color.copy(_color2).multiplyScalar(Math.max(0, bright));
      instancedMesh.setColorAt(id, _color);
    }
  }

  instancedMesh.instanceColor.needsUpdate = true;
}

/**
 * Animate blast radius selection: depth-by-depth ripple, then continuous pulse.
 * Replaces Phase 1's setHighlight.
 * @param {{ 1: number[], 2: number[], 3: number[] }} nodesByDepth
 * @param {number} selectedId
 */
export function startRipple(nodesByDepth, selectedId) {
  if (!instancedMesh) return;
  _rippleActive = true;
  _rippleNodesByDepth = nodesByDepth;
  _rippleSelectedId = selectedId;
  _rippleStartTime = performance.now();

  // Collect all blast ids for dim-everything-else pass
  const blastIds = new Set();
  for (const ids of Object.values(nodesByDepth)) {
    for (const id of ids) blastIds.add(id);
  }

  // Dim non-blast/non-selected nodes; enlarge selected node
  for (let i = 0; i < nodeCount; i++) {
    const fn = fileNodesRef[i];

    if (fn.id === selectedId) {
      instancedMesh.getMatrixAt(i, _matrix);
      _matrix.decompose(_position, _quaternion, _scale);
      const r = nodeRadius(fn.degree) * 1.4;
      _scale.set(r, r, 1);
      _matrix.compose(_position, _quaternion, _scale);
      instancedMesh.setMatrixAt(i, _matrix);
      instancedMesh.setColorAt(i, new THREE.Color('#ffffff'));
    } else if (!blastIds.has(fn.id)) {
      // Dim: shrink + dark color
      instancedMesh.getMatrixAt(i, _matrix);
      _matrix.decompose(_position, _quaternion, _scale);
      const r = nodeRadius(fn.degree) * 0.6;
      _scale.set(r, r, 1);
      _matrix.compose(_position, _quaternion, _scale);
      instancedMesh.setMatrixAt(i, _matrix);
      instancedMesh.setColorAt(i, new THREE.Color('#1a1e24'));
    } else {
      // Blast nodes start at dim color; rippleTick will animate them
      instancedMesh.setColorAt(i, new THREE.Color('#1a1e24'));
    }
  }

  instancedMesh.instanceMatrix.needsUpdate = true;
  instancedMesh.instanceColor.needsUpdate = true;

  startContinuousAnimation();
  registerTickCallback(_rippleTick);
}

/**
 * Clear selection: restore all nodes to mode colors, re-apply scrubber mask if active.
 * @param {string} mode - current mode ('architecture' | 'risk' | 'ownership')
 */
export function clearRipple(mode) {
  _rippleActive = false;
  unregisterTickCallback(_rippleTick);
  unregisterTickCallback(_pulseTick);
  stopContinuousAnimation();

  // Restore all node scales to normal
  for (let i = 0; i < nodeCount; i++) {
    const fn = fileNodesRef[i];
    const r = nodeRadius(fn.degree);
    instancedMesh.getMatrixAt(i, _matrix);
    _matrix.decompose(_position, _quaternion, _scale);
    _scale.set(r, r, 1);
    _matrix.compose(_position, _quaternion, _scale);
    instancedMesh.setMatrixAt(i, _matrix);
  }
  instancedMesh.instanceMatrix.needsUpdate = true;

  // Restore mode colors (+ scrubber mask applied inside updateColors)
  updateColors(mode);
}

/**
 * Apply/clear the time scrubber brightness mask.
 * Files not in activeSet are dimmed to 20% brightness.
 * No-op while ripple is active (mask applied on clearRipple).
 * @param {Set<number>|null} activeSet
 */
export function setScrubberMask(activeSet) {
  _scrubberMask = activeSet;
  if (!_rippleActive) {
    updateColors(_currentMode);
  }
}

/**
 * Set computed filter mask from filters.js.
 * null = no filter active (all nodes fully visible).
 * Composes with scrubber mask: a node must pass both to be bright.
 * Note: _rippleActive and _currentMode are existing variables in this file,
 * also used by setScrubberMask above.
 * @param {Set<number>|null} mask
 */
export function setFilterMask(mask) {
  _filterMask = mask;
  if (!_rippleActive) {
    updateColors(_currentMode);
  }
}

export function getInstancedMesh() {
  return instancedMesh;
}
