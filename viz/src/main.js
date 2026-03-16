import { loadData } from './data.js';
import { computeLayout, fileNodes } from './layout.js';
import { initRenderer, addRegions, markDirty } from './renderer.js';
import { createNodes } from './nodes.js';
import { createEdges } from './edges.js';
import { initInteraction, getBlastCounts } from './interaction.js';
import { initUI } from './ui.js';
import { initFilters } from './filters.js';
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

  // 3. initFilters stores blast counts (no DOM access at this point)
  initFilters(getBlastCounts());

  // 4. Set slider max values from actual data — must come after initUI (sliders exist).
  //    Use reduce to avoid call stack limit on large arrays.
  //    Math.max(..., 1) ensures sliders are never collapsed to zero-range on degenerate data.
  const blastCounts = getBlastCounts();
  const maxDegree = Math.max(fileNodes.reduce((m, fn) => Math.max(m, fn.degree), 0), 1);
  const maxReachable = Math.max(blastCounts.reduce((m, v) => Math.max(m, v), 0), 1);
  const degSlider = document.getElementById('degree-slider');
  const reachSlider = document.getElementById('reachable-slider');
  if (degSlider) degSlider.max = maxDegree;
  if (reachSlider) reachSlider.max = maxReachable;

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
