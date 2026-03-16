import { loadData } from './data.js';
import { computeLayout } from './layout.js';
import { initRenderer, addRegions, markDirty } from './renderer.js';

async function init() {
  const status = document.getElementById('load-status');

  status.textContent = 'Loading data...';
  const data = await loadData();
  console.log(`Loaded: ${data.stats.files} files, ${data.stats.struct_edges} edges`);

  status.textContent = 'Computing layout...';
  const layout = computeLayout(data.tree);

  status.textContent = 'Initializing renderer...';
  const container = document.getElementById('canvas-container');
  initRenderer(container, layout.bounds);

  status.textContent = 'Drawing regions...';
  addRegions(layout.regions);

  // Dismiss loading overlay
  const overlay = document.getElementById('loading-overlay');
  overlay.classList.add('fade-out');
  setTimeout(() => overlay.remove(), 500);
}

init().catch(err => console.error('Init failed:', err));
