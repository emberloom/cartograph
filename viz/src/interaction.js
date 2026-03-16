import * as THREE from 'three';
import { camera, renderer, markDirty, setResizeCallback } from './renderer.js';
import { fileNodes } from './layout.js';
import { setHighlight, updateColors } from './nodes.js';
import { setEdgeOpacity, getEdgeMesh } from './edges.js';

// ── State ──
let selectedNode = null;
let currentMode = 'architecture';
let structAdj = {};
let cochangeByNode = {};
let showCochange = false;

// ── Picking ──
let pickingRT = null;
let pickingScene = null;
let pickingMesh = null;
const _pickColor = new Uint8Array(4);

// ── Zoom state ──
let zoomLevel = 1;
const MIN_ZOOM = 0.3;
const MAX_ZOOM = 20;

// Camera bounds snapshot (initial)
let camInitial = { left: 0, right: 0, top: 0, bottom: 0 };
let panOffset = { x: 0, y: 0 };
let isPanning = false;
let panStart = { x: 0, y: 0 };
let didPan = false; // true if mouse moved during drag — suppresses click

/**
 * Initialize interaction: zoom, pan, picking, selection.
 * @param {Object} data - full data.json
 */
export function initInteraction(data) {
  cochangeByNode = data.cochange_by_node || {};

  // Build structural adjacency for blast radius BFS
  structAdj = {};
  for (const [s, t] of data.struct_edges) {
    if (!structAdj[s]) structAdj[s] = [];
    if (!structAdj[t]) structAdj[t] = [];
    structAdj[s].push(t);
    structAdj[t].push(s);
  }

  // Store initial camera bounds
  camInitial = {
    left: camera.left,
    right: camera.right,
    top: camera.top,
    bottom: camera.bottom,
  };

  // Setup picking render target
  pickingRT = new THREE.WebGLRenderTarget(1, 1, {
    format: THREE.RGBAFormat,
    type: THREE.UnsignedByteType,
  });

  // Build picking scene with unique color per node
  pickingScene = new THREE.Scene();
  pickingScene.background = new THREE.Color(0x000000);
  const geo = new THREE.CircleGeometry(1, 16);
  const mat = new THREE.MeshBasicMaterial({ vertexColors: false });
  pickingMesh = new THREE.InstancedMesh(geo, mat, fileNodes.length);

  const _m = new THREE.Matrix4();
  const _p = new THREE.Vector3();
  const _q = new THREE.Quaternion();
  const _s = new THREE.Vector3();
  const _c = new THREE.Color();

  for (let i = 0; i < fileNodes.length; i++) {
    const fn = fileNodes[i];
    const r = Math.max(3, Math.min(16, 2 + Math.sqrt(fn.degree) * 2.5));
    // Make picking radius slightly larger for easier clicks
    const pr = r * 1.3;
    _p.set(fn.x, -fn.y, 0);
    _s.set(pr, pr, 1);
    _m.compose(_p, _q, _s);
    pickingMesh.setMatrixAt(i, _m);

    // Encode id+1 as color (0 = background = no node)
    const encoded = fn.id + 1;
    _c.setRGB(
      ((encoded >> 16) & 0xff) / 255,
      ((encoded >> 8) & 0xff) / 255,
      (encoded & 0xff) / 255,
    );
    pickingMesh.setColorAt(i, _c);
  }

  pickingMesh.instanceMatrix.needsUpdate = true;
  pickingMesh.instanceColor.needsUpdate = true;
  pickingScene.add(pickingMesh);

  // Event listeners
  const canvas = renderer.domElement;
  canvas.addEventListener('wheel', onWheel, { passive: false });
  canvas.addEventListener('mousedown', onMouseDown);
  canvas.addEventListener('mousemove', onMouseMove);
  canvas.addEventListener('mouseup', onMouseUp);
  canvas.addEventListener('click', onClick);

  // Layer toggle handler
  let showImports = true;
  window.addEventListener('layer-toggle', (e) => {
    const { layer, visible } = e.detail;
    if (layer === 'imports') {
      showImports = visible;
      const mesh = getEdgeMesh();
      if (mesh) {
        mesh.visible = visible;
        markDirty();
      }
    }
    // Co-change layer: controls whether co-change arcs draw on selection
    // (no pre-rendered geometry to toggle — it's created on selectNode)
    if (layer === 'cochange') {
      showCochange = visible;
    }
  });

  // Escape key clears selection
  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') clearSelection();
  });

  setResizeCallback(() => {
    camInitial = {
      left: camera.left,
      right: camera.right,
      top: camera.top,
      bottom: camera.bottom,
    };
    // Reset pan and zoom when window resizes
    panOffset = { x: 0, y: 0 };
    zoomLevel = 1;
  });

  // Animated camera transition
  let animating = false;
  let animTarget = { zoom: 1, panX: 0, panY: 0 };

  function animateToTarget(targetZoom, targetPanX, targetPanY, duration, onComplete) {
    const startZoom = zoomLevel;
    const startPanX = panOffset.x;
    const startPanY = panOffset.y;
    const startTime = performance.now();
    animating = true;

    function step(now) {
      const t = Math.min(1, (now - startTime) / duration);
      const ease = t * (2 - t); // ease-out quad
      zoomLevel = startZoom + (targetZoom - startZoom) * ease;
      panOffset.x = startPanX + (targetPanX - startPanX) * ease;
      panOffset.y = startPanY + (targetPanY - startPanY) * ease;
      applyZoom();
      if (t < 1) {
        requestAnimationFrame(step);
      } else {
        animating = false;
        if (onComplete) onComplete();
      }
    }
    requestAnimationFrame(step);
  }

  // Listen for navigate-to-node from hotspots/search
  window.addEventListener('navigate-to-node', (e) => {
    const id = e.detail.id;
    const node = fileNodes.find(fn => fn.id === id);
    if (!node) return;

    const targetPanX = node.x - (camInitial.right + camInitial.left) / 2;
    const targetPanY = -node.y - (camInitial.top + camInitial.bottom) / 2;
    const targetZoom = 4;

    animateToTarget(targetZoom, targetPanX, targetPanY, 400, () => {
      selectNode(node);
    });
  });
}

export function updateCamInitial() {
  camInitial = {
    left: camera.left,
    right: camera.right,
    top: camera.top,
    bottom: camera.bottom,
  };
}

function onWheel(e) {
  e.preventDefault();
  const factor = e.deltaY > 0 ? 0.9 : 1.1;
  zoomLevel = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, zoomLevel * factor));
  applyZoom();
}

function applyZoom() {
  const cx = (camInitial.right + camInitial.left) / 2 + panOffset.x;
  const cy = (camInitial.top + camInitial.bottom) / 2 + panOffset.y;
  const hw = ((camInitial.right - camInitial.left) / 2) / zoomLevel;
  const hh = ((camInitial.top - camInitial.bottom) / 2) / zoomLevel;
  camera.left = cx - hw;
  camera.right = cx + hw;
  camera.top = cy + hh;
  camera.bottom = cy - hh;
  camera.updateProjectionMatrix();
  markDirty();
}

function onMouseDown(e) {
  if (e.button === 0) {
    isPanning = true;
    didPan = false;
    panStart = { x: e.clientX, y: e.clientY };
  }
}

function onMouseMove(e) {
  if (!isPanning) return;
  const dx = e.clientX - panStart.x;
  const dy = e.clientY - panStart.y;
  if (Math.abs(dx) > 2 || Math.abs(dy) > 2) didPan = true;
  panStart = { x: e.clientX, y: e.clientY };

  // Convert pixel movement to world units
  const viewW = camera.right - camera.left;
  const viewH = camera.top - camera.bottom;
  const canvasW = renderer.domElement.clientWidth;
  const canvasH = renderer.domElement.clientHeight;
  panOffset.x -= (dx / canvasW) * viewW;
  panOffset.y += (dy / canvasH) * viewH;
  applyZoom();
}

function onMouseUp() {
  isPanning = false;
}

function onClick(e) {
  // Suppress click if this was a pan gesture
  if (didPan) return;

  // Pick node via GPU color buffer
  const rect = renderer.domElement.getBoundingClientRect();
  const x = e.clientX - rect.left;
  const y = e.clientY - rect.top;

  // Render picking scene to full-size RT and read the clicked pixel
  const dpr = renderer.getPixelRatio();
  const rtW = Math.ceil(rect.width * dpr);
  const rtH = Math.ceil(rect.height * dpr);
  pickingRT.setSize(rtW, rtH);

  renderer.setRenderTarget(pickingRT);
  renderer.render(pickingScene, camera);

  const pixelX = Math.floor(x * dpr);
  const pixelY = Math.floor((rect.height - y) * dpr); // flip Y for GL
  renderer.readRenderTargetPixels(pickingRT, pixelX, pixelY, 1, 1, _pickColor);

  renderer.setRenderTarget(null);

  const encoded = (_pickColor[0] << 16) | (_pickColor[1] << 8) | _pickColor[2];
  if (encoded === 0) {
    clearSelection();
    return;
  }

  const nodeId = encoded - 1;
  const node = fileNodes.find(fn => fn.id === nodeId);
  if (node) {
    selectNode(node);
  }
}

/**
 * BFS blast radius on structural adjacency.
 */
function blastRadiusBFS(nodeId, maxDepth) {
  const blastDepth = {};
  let frontier = [nodeId];
  const visited = new Set([nodeId]);
  for (let depth = 1; depth <= maxDepth; depth++) {
    const next = [];
    for (const n of frontier) {
      for (const nb of structAdj[n] || []) {
        if (!visited.has(nb)) {
          visited.add(nb);
          blastDepth[nb] = depth;
          next.push(nb);
        }
      }
    }
    frontier = next;
  }
  return blastDepth;
}

function selectNode(node) {
  selectedNode = node;

  const blastDepth = blastRadiusBFS(node.id, 3);
  const highlightIds = new Set(Object.keys(blastDepth).map(Number));
  highlightIds.add(node.id);

  setHighlight(highlightIds, blastDepth, node.id);
  setEdgeOpacity(0.03);

  // Dispatch custom event for UI to listen to
  window.dispatchEvent(new CustomEvent('node-selected', {
    detail: {
      node,
      blastDepth,
      blastCount: Object.keys(blastDepth).length,
      cochanges: cochangeByNode[String(node.id)] || [],
    },
  }));

  markDirty();
}

export function clearSelection() {
  selectedNode = null;
  setHighlight(null, null, null);
  updateColors(currentMode);
  setEdgeOpacity(null);

  window.dispatchEvent(new CustomEvent('node-deselected'));
  markDirty();
}

export function setMode(mode) {
  currentMode = mode;
  if (!selectedNode) {
    updateColors(mode);
  }
}

export function getSelectedNode() {
  return selectedNode;
}
