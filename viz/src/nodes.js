import * as THREE from 'three';
import { scene, markDirty } from './renderer.js';
import { dirColor, riskColor } from './colors.js';

let instancedMesh = null;
let nodeCount = 0;
let fileNodesRef = [];

// Temp objects for instance matrix manipulation
const _matrix = new THREE.Matrix4();
const _position = new THREE.Vector3();
const _quaternion = new THREE.Quaternion();
const _scale = new THREE.Vector3();
const _color = new THREE.Color();

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
  fileNodesRef = fileNodes;
  nodeCount = fileNodes.length;

  const geo = new THREE.CircleGeometry(1, 16); // unit circle, scaled per instance
  const mat = new THREE.MeshBasicMaterial({
    vertexColors: false,
    transparent: true,
    opacity: 0.85,
  });

  instancedMesh = new THREE.InstancedMesh(geo, mat, nodeCount);
  instancedMesh.instanceMatrix.setUsage(THREE.DynamicDrawUsage);

  // Set per-instance transforms and colors
  for (let i = 0; i < nodeCount; i++) {
    const fn = fileNodes[i];
    const r = nodeRadius(fn.degree);
    _position.set(fn.x, -fn.y, 0); // flip Y
    _scale.set(r, r, 1);
    _matrix.compose(_position, _quaternion, _scale);
    instancedMesh.setMatrixAt(i, _matrix);

    // Default: architecture mode color
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
 * @param {'architecture' | 'risk'} mode
 */
export function updateColors(mode) {
  if (!instancedMesh) return;

  for (let i = 0; i < nodeCount; i++) {
    const fn = fileNodesRef[i];
    if (mode === 'risk') {
      const [r, g, b] = riskColor(fn.riskScore);
      _color.setRGB(r, g, b);
    } else {
      _color.set(dirColor(fn.topLevelDirIdx));
    }
    instancedMesh.setColorAt(i, _color);
  }

  instancedMesh.instanceColor.needsUpdate = true;
  markDirty();
}

/**
 * Dim/highlight nodes for selection.
 * @param {Set<number>|null} highlightIds - set of file IDs to keep bright, null = reset all
 * @param {Object} blastDepth - {id: depth} for blast radius coloring
 * @param {number|null} selectedId - the clicked node ID
 */
export function setHighlight(highlightIds, blastDepth, selectedId) {
  if (!instancedMesh) return;

  for (let i = 0; i < nodeCount; i++) {
    const fn = fileNodesRef[i];

    if (highlightIds === null) {
      // Reset: restore full opacity via scale
      instancedMesh.getMatrixAt(i, _matrix);
      _matrix.decompose(_position, _quaternion, _scale);
      const r = nodeRadius(fn.degree);
      _scale.set(r, r, 1);
      _matrix.compose(_position, _quaternion, _scale);
      instancedMesh.setMatrixAt(i, _matrix);

      // Restore mode color (handled by caller via updateColors)
    } else if (fn.id === selectedId) {
      // Selected node: make it bigger
      instancedMesh.getMatrixAt(i, _matrix);
      _matrix.decompose(_position, _quaternion, _scale);
      const r = nodeRadius(fn.degree) * 1.4;
      _scale.set(r, r, 1);
      _matrix.compose(_position, _quaternion, _scale);
      instancedMesh.setMatrixAt(i, _matrix);
      _color.set('#ffffff');
      instancedMesh.setColorAt(i, _color);
    } else if (blastDepth && blastDepth[fn.id] !== undefined) {
      // Blast radius nodes
      const depth = blastDepth[fn.id];
      const blastColors = ['#ff6b6b', '#ff9f43', '#ffd93d'];
      _color.set(blastColors[depth - 1] || '#ffd93d');
      instancedMesh.setColorAt(i, _color);
    } else if (!highlightIds.has(fn.id)) {
      // Dimmed
      instancedMesh.getMatrixAt(i, _matrix);
      _matrix.decompose(_position, _quaternion, _scale);
      const r = nodeRadius(fn.degree) * 0.6;
      _scale.set(r, r, 1);
      _matrix.compose(_position, _quaternion, _scale);
      instancedMesh.setMatrixAt(i, _matrix);
      _color.set('#1a1e24');
      instancedMesh.setColorAt(i, _color);
    }
  }

  instancedMesh.instanceMatrix.needsUpdate = true;
  instancedMesh.instanceColor.needsUpdate = true;
  markDirty();
}

export function getInstancedMesh() {
  return instancedMesh;
}
