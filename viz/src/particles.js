/**
 * Co-change particle flow.
 *
 * When a node is selected and the co-change layer is on, shows animated
 * dots flowing along arcs to co-change partners. Speed ∝ confidence.
 *
 * Opacity is encoded as RGB brightness (THREE.PointsMaterial does not support
 * per-point alpha). Particles fade to background (#0d1117) at arc endpoints.
 */
import * as THREE from 'three';
import { getScene, markDirty, startContinuousAnimation, stopContinuousAnimation, registerTickCallback, unregisterTickCallback } from './renderer.js';
import { filePositions, regions } from './layout.js';
import { computeArcPoints } from './edges.js';
import { dirColor } from './colors.js';

const MAX_PARTNERS = 20;
const PARTICLES_PER_ARC = 12;
const BG = new THREE.Color(0x0d1117);

let _pointsMesh = null;

// Per-particle Float32Arrays (flat, indexed by particle index)
let _t = null;       // progress 0→1 along arc
let _speed = null;   // advance per second
let _arcIdx = null;  // which arc this particle belongs to

// Pre-sampled arc point arrays: _arcPoints[arcIdx] = THREE.Vector3[]
let _arcPoints = [];

// Base colors per arc (RGB of co-change partner node color)
let _arcBaseColors = [];

function _tick(dt) {
  if (!_pointsMesh) return;

  const total = _t.length;
  const posArr = _pointsMesh.geometry.attributes.position;
  const colArr = _pointsMesh.geometry.attributes.color;

  for (let i = 0; i < total; i++) {
    _t[i] += _speed[i] * dt;
    if (_t[i] > 1) _t[i] -= 1;

    const arc = _arcPoints[_arcIdx[i]];
    const pts = arc.length - 1;
    const tClamped = Math.max(0, Math.min(1, _t[i]));
    const fi = tClamped * pts;
    const lo = Math.floor(fi);
    const hi = Math.min(lo + 1, pts);
    const frac = fi - lo;

    // Interpolate position between sampled points
    const ax = arc[lo].x + (arc[hi].x - arc[lo].x) * frac;
    const ay = arc[lo].y + (arc[hi].y - arc[lo].y) * frac;
    posArr.setXYZ(i, ax, ay, 0.5);

    // Brightness = sin(t*π): peaks at midpoint, 0 at endpoints
    const brightness = Math.sin(tClamped * Math.PI);
    const base = _arcBaseColors[_arcIdx[i]];
    // Lerp from BG color to base color by brightness
    const r = BG.r + (base.r - BG.r) * brightness;
    const g = BG.g + (base.g - BG.g) * brightness;
    const b = BG.b + (base.b - BG.b) * brightness;
    colArr.setXYZ(i, r, g, b);
  }

  posArr.needsUpdate = true;
  colArr.needsUpdate = true;
}

/**
 * Pre-compute region centroids (depth-1 regions only) as {name → {x,y}}.
 * Y is already flipped for Three.js space.
 */
function _buildCentroids() {
  const centroids = {};
  for (const r of regions) {
    if (r.depth === 1) {
      centroids[r.name] = {
        x: (r.x0 + r.x1) / 2,
        y: -((r.y0 + r.y1) / 2),
      };
    }
  }
  return centroids;
}

/**
 * Initialize particles for a selected node's co-change partners.
 * @param {number} nodeId - selected file node id
 * @param {Array<{t: number, c: number}>} cochangeData - top co-change partners [{t:targetId, c:confidence}]
 * @param {Array} fileNodes - fileNodes array from layout.js (for topLevelDir lookup)
 */
export function initParticles(nodeId, cochangeData, fileNodes) {
  clearParticles();

  const partners = cochangeData.slice(0, MAX_PARTNERS);
  if (partners.length === 0) return;

  const centroids = _buildCentroids();
  const srcPos = filePositions.get(nodeId);
  if (!srcPos) return;

  // Build file id → topLevelDir map
  const fileTopDir = {};
  for (const fn of fileNodes) fileTopDir[fn.id] = fn.topLevelDir;

  const srcDir = fileTopDir[nodeId];
  const srcWorldPos = { x: srcPos.x, y: -srcPos.y };

  _arcPoints = [];
  _arcBaseColors = [];

  for (let a = 0; a < partners.length; a++) {
    const { t: tgtId, c: conf } = partners[a];
    const tgtPos = filePositions.get(tgtId);
    if (!tgtPos) continue;

    const tgtDir = fileTopDir[tgtId];
    const tgtWorldPos = { x: tgtPos.x, y: -tgtPos.y };

    let centroidA, centroidB;
    if (srcDir && tgtDir && srcDir !== tgtDir && centroids[srcDir] && centroids[tgtDir]) {
      centroidA = centroids[srcDir];
      centroidB = centroids[tgtDir];
    } else {
      // Same module: degenerate Bezier (straight line)
      centroidA = centroidB = {
        x: (srcWorldPos.x + tgtWorldPos.x) / 2,
        y: (srcWorldPos.y + tgtWorldPos.y) / 2,
      };
    }

    _arcPoints.push(computeArcPoints(srcWorldPos, tgtWorldPos, centroidA, centroidB, 16));

    // Base color: architecture palette color of the co-change partner
    const fn = fileNodes.find(f => f.id === tgtId);
    const hex = fn ? dirColor(fn.topLevelDirIdx) : '#58a6ff';
    _arcBaseColors.push(new THREE.Color(hex));
  }

  const arcCount = _arcPoints.length;
  if (arcCount === 0) return;

  const total = arcCount * PARTICLES_PER_ARC;

  _t = new Float32Array(total);
  _speed = new Float32Array(total);
  _arcIdx = new Int32Array(total);

  for (let a = 0; a < arcCount; a++) {
    const conf = partners[a]?.c ?? 0.5;
    const speed = 0.15 + conf * 0.35;
    for (let p = 0; p < PARTICLES_PER_ARC; p++) {
      const i = a * PARTICLES_PER_ARC + p;
      _t[i] = p / PARTICLES_PER_ARC; // staggered start so flow is continuous
      _speed[i] = speed;
      _arcIdx[i] = a;
    }
  }

  // Build BufferGeometry
  const geo = new THREE.BufferGeometry();
  geo.setAttribute('position', new THREE.Float32BufferAttribute(new Float32Array(total * 3), 3));
  geo.setAttribute('color', new THREE.Float32BufferAttribute(new Float32Array(total * 3), 3));

  const mat = new THREE.PointsMaterial({
    size: 4,
    vertexColors: true,
    transparent: false,
    depthWrite: false,
    sizeAttenuation: false,
  });

  _pointsMesh = new THREE.Points(geo, mat);
  getScene().add(_pointsMesh);

  registerTickCallback(_tick);
  startContinuousAnimation();
}

/**
 * Remove particles from scene and stop animation.
 */
export function clearParticles() {
  if (_pointsMesh) {
    getScene().remove(_pointsMesh);
    _pointsMesh.geometry.dispose();
    _pointsMesh.material.dispose();
    _pointsMesh = null;
  }
  unregisterTickCallback(_tick);
  stopContinuousAnimation();
  _arcPoints = [];
  _arcBaseColors = [];
  _t = null;
  _speed = null;
  _arcIdx = null;
}
