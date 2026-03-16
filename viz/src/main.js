import { loadData } from './data.js';
import { computeLayout, fileNodes } from './layout.js';
import { initRenderer, addRegions, markDirty } from './renderer.js';
import { createNodes } from './nodes.js';
import { createEdges } from './edges.js';
import { initInteraction } from './interaction.js';
import { initUI } from './ui.js';
import { initScrubber } from './scrubber.js';
import { initTour } from './tour.js';

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

  status.textContent = 'Drawing nodes...';
  createNodes(layout.fileNodes);

  status.textContent = 'Drawing edges...';
  createEdges(data.struct_edges);

  status.textContent = 'Setting up interaction...';
  initInteraction(data);

  // Collect top-level directory names for legend
  const topDirs = [...new Set(layout.fileNodes.map(fn => fn.topLevelDir))].filter(Boolean).sort();

  status.textContent = 'Building UI...';
  initUI(data, topDirs);

  // Time scrubber
  const scrubberEl = document.getElementById('scrubber-container');
  if (scrubberEl) initScrubber(data.commits || null, scrubberEl);

  // Guided tour
  initTour(data);

  // Dismiss loading overlay
  const overlay = document.getElementById('loading-overlay');
  overlay.classList.add('fade-out');
  setTimeout(() => overlay.remove(), 500);
}

init().catch(err => console.error('Init failed:', err));
