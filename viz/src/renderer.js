import * as THREE from 'three';
import { EffectComposer } from 'three/addons/postprocessing/EffectComposer.js';
import { RenderPass } from 'three/addons/postprocessing/RenderPass.js';
import { UnrealBloomPass } from 'three/addons/postprocessing/UnrealBloomPass.js';
import { BG_COLOR } from './colors.js';

export let scene, camera, renderer, composer;
export let dirty = true;

let _onResizeCallback = null;

export function setResizeCallback(fn) {
  _onResizeCallback = fn;
}

const BLOOM_STRENGTH = 0.4;
const BLOOM_RADIUS = 0.8;
const BLOOM_THRESHOLD = 0.6;

/**
 * Initialize Three.js scene with orthographic camera + bloom.
 * @param {HTMLElement} container
 * @param {{w: number, h: number}} bounds - treemap world-space bounds
 */
export function initRenderer(container, bounds) {
  const W = window.innerWidth;
  const H = window.innerHeight;

  // Scene
  scene = new THREE.Scene();
  scene.background = new THREE.Color(BG_COLOR);

  // Orthographic camera matching treemap bounds
  // Treemap coords: x=[0,W], y=[0,H]. We flip Y in geometry (y → -y),
  // so the visible range is x=[0,W], y=[-H,0].
  const aspect = W / H;
  // Fit to whichever dimension is constraining so the entire treemap is visible
  const viewH = aspect >= 1
    ? bounds.h * 1.05
    : (bounds.w / aspect) * 1.05;
  const viewW = viewH * aspect;
  const cx = bounds.w / 2;
  const cy = -bounds.h / 2; // center of flipped Y range
  camera = new THREE.OrthographicCamera(
    cx - viewW / 2,
    cx + viewW / 2,
    cy + viewH / 2,
    cy - viewH / 2,
    0.1,
    1000,
  );
  camera.position.set(cx, cy, 100);
  camera.lookAt(cx, cy, 0);

  // Renderer
  renderer = new THREE.WebGLRenderer({ antialias: true });
  renderer.setSize(W, H);
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
  container.appendChild(renderer.domElement);

  // Bloom post-processing
  composer = new EffectComposer(renderer);
  composer.addPass(new RenderPass(scene, camera));
  const bloom = new UnrealBloomPass(
    new THREE.Vector2(W, H),
    BLOOM_STRENGTH,
    BLOOM_RADIUS,
    BLOOM_THRESHOLD,
  );
  composer.addPass(bloom);

  // Handle resize
  window.addEventListener('resize', () => {
    const w = window.innerWidth;
    const h = window.innerHeight;
    renderer.setSize(w, h);
    composer.setSize(w, h);
    const newAspect = w / h;
    const newViewH = newAspect >= 1
      ? bounds.h * 1.05
      : (bounds.w / newAspect) * 1.05;
    const newViewW = newViewH * newAspect;
    const cx = bounds.w / 2;
    const cy = -bounds.h / 2;
    camera.left = cx - newViewW / 2;
    camera.right = cx + newViewW / 2;
    camera.top = cy + newViewH / 2;
    camera.bottom = cy - newViewH / 2;
    camera.updateProjectionMatrix();
    markDirty();
    if (_onResizeCallback) _onResizeCallback();
  });

  // Render loop (on-demand)
  function renderLoop() {
    requestAnimationFrame(renderLoop);
    if (dirty) {
      composer.render();
      dirty = false;
    }
  }
  renderLoop();

  return { scene, camera, renderer, composer };
}

export function markDirty() {
  dirty = true;
}

/**
 * Add region planes (directory rectangles) to the scene.
 * @param {Array} regions - from layout.js
 */
export function addRegions(regions) {
  for (const r of regions) {
    const w = r.x1 - r.x0;
    const h = r.y1 - r.y0;
    if (w < 1 || h < 1) continue;

    const cx = r.x0 + w / 2;
    // Flip Y: treemap y=0 is top, Three.js y=0 is bottom
    const cy = r.y0 + h / 2;

    // Fill plane
    const opacity = r.depth <= 1 ? 0.15 : 0.08;
    const color = new THREE.Color(r.color);
    const fillGeo = new THREE.PlaneGeometry(w, h);
    const fillMat = new THREE.MeshBasicMaterial({
      color,
      transparent: true,
      opacity,
      depthWrite: false,
    });
    const fill = new THREE.Mesh(fillGeo, fillMat);
    fill.position.set(cx, -cy, -1); // z=-1 behind nodes
    scene.add(fill);

    // Border
    const borderPoints = [
      new THREE.Vector3(r.x0, -r.y0, -0.5),
      new THREE.Vector3(r.x1, -r.y0, -0.5),
      new THREE.Vector3(r.x1, -r.y1, -0.5),
      new THREE.Vector3(r.x0, -r.y1, -0.5),
      new THREE.Vector3(r.x0, -r.y0, -0.5),
    ];
    const borderGeo = new THREE.BufferGeometry().setFromPoints(borderPoints);
    const borderMat = new THREE.LineBasicMaterial({
      color,
      transparent: true,
      opacity: 0.3,
    });
    const border = new THREE.Line(borderGeo, borderMat);
    scene.add(border);

    // Label sprite (only for regions wide enough)
    const minPixelWidth = 40;
    if (w > minPixelWidth && r.depth <= 2) {
      const canvas = document.createElement('canvas');
      const fontSize = 14;
      const ctx = canvas.getContext('2d');
      ctx.font = `${fontSize}px monospace`;
      const textWidth = ctx.measureText(r.name).width;
      canvas.width = Math.ceil(textWidth) + 4;
      canvas.height = fontSize + 4;
      ctx.font = `${fontSize}px monospace`;
      ctx.fillStyle = r.color;
      ctx.globalAlpha = 0.7;
      ctx.fillText(r.name, 2, fontSize);
      const texture = new THREE.CanvasTexture(canvas);
      texture.minFilter = THREE.LinearFilter;
      const spriteMat = new THREE.SpriteMaterial({
        map: texture,
        transparent: true,
        depthWrite: false,
      });
      const sprite = new THREE.Sprite(spriteMat);
      const scale = w * 0.015;
      sprite.scale.set(
        (canvas.width / canvas.height) * scale * 4,
        scale * 4,
        1,
      );
      const spriteH = scale * 4;
      sprite.position.set(
        r.x0 + w * 0.02 + sprite.scale.x / 2,
        -(r.y0 + 10) - spriteH / 2,
        0.5,
      );
      scene.add(sprite);
    }
  }

  markDirty();
}
