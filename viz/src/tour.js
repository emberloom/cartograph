/**
 * Guided tour: 5-step walkthrough for first-time visitors.
 * Auto-starts on first visit (localStorage flag), replayable via ? button.
 */
import { switchMode } from './modes.js';
import { flyTo, getCamInitial, clearSelection } from './interaction.js';
import { fileNodes } from './layout.js';

const STORAGE_KEY = 'cartograph_tour_seen';

let _hotspot = null;  // data.hotspots[0] enriched with x,y from fileNodes
let _card = null;     // current tooltip DOM element
let _step = 0;
const TOTAL_STEPS = 5;

const STEPS = [
  {
    title: 'Your architecture',
    body: 'Color shows module structure. Each region is a directory — size reflects file connectivity.',
    pos: 'top-right',
  },
  {
    title: 'Your hotspots',
    body: 'The largest nodes are your most-connected files. Click any node to inspect it.',
    pos: 'bottom-left',
  },
  {
    title: 'Blast radius',
    body: 'The blast radius shows what breaks if this file changes. Red = direct dep, orange = 2 hops, yellow = 3 hops.',
    pos: 'bottom-left',
  },
  {
    title: 'Risk map',
    body: 'Risk mode recolors by churn × blast radius. Red = files that change often and break a lot.',
    pos: 'top-right',
  },
  {
    title: 'Ownership',
    body: 'Ownership mode shows who touches what. Gray = unowned. Patchwork regions = shared ownership risk.',
    pos: 'top-right',
  },
];

/**
 * Initialize tour with data. Call once from main.js after data loaded.
 * Looks up hotspot x,y from fileNodes since data.hotspots only carries id/degree.
 * @param {Object} data - full data.json
 */
export function initTour(data) {
  const hotspotData = data.hotspots?.[0] ?? null;
  if (hotspotData) {
    // fileNodes[id] is O(1) since IDs are 0..N-1 contiguous
    const fn = fileNodes.find(n => n.id === hotspotData.id);
    _hotspot = fn ? { ...hotspotData, x: fn.x, y: fn.y } : hotspotData;
  }

  // Auto-start on first visit
  if (!localStorage.getItem(STORAGE_KEY)) {
    setTimeout(() => startTour(), 500);
  }
}

export function startTour() {
  _step = 0;
  _showStep(0);
}

function _showStep(index) {
  _removeCard();
  if (index >= TOTAL_STEPS) {
    _endTour();
    return;
  }

  const spec = STEPS[index];

  // Perform the action for this step, then show the card
  _runStepAction(index, () => {
    _card = _buildCard(index, spec);
    document.body.appendChild(_card);
    // Fade in
    requestAnimationFrame(() => {
      _card.style.opacity = '1';
    });
  });
}

function _runStepAction(index, onReady) {
  const cam = getCamInitial();
  const centerX = (cam.right + cam.left) / 2;
  const centerY = (cam.top + cam.bottom) / 2;

  if (index === 0) {
    clearSelection(); // clear any active ripple/selection before starting
    switchMode('architecture');
    flyTo(0, 0, 1, 400, onReady);
  } else if (index === 1) {
    if (_hotspot) {
      const panX = _hotspot.x !== undefined
        ? _hotspot.x - centerX
        : 0;
      const panY = _hotspot.y !== undefined
        ? -_hotspot.y - centerY
        : 0;
      flyTo(panX, panY, 4, 400, () => {
        window.dispatchEvent(new CustomEvent('navigate-to-node', { detail: { id: _hotspot.id } }));
        // Show card 400ms after navigate animation starts (ripple will be in progress)
        setTimeout(onReady, 400);
      });
    } else {
      onReady();
    }
  } else if (index === 2) {
    // Ripple already playing from step 1 — just show card
    onReady();
  } else if (index === 3) {
    clearSelection();
    switchMode('risk');
    setTimeout(onReady, 450); // wait for mode transition
  } else if (index === 4) {
    switchMode('ownership');
    setTimeout(onReady, 450);
  }
}

function _buildCard(index, spec) {
  const card = document.createElement('div');

  const posStyle = spec.pos === 'top-right'
    ? 'top:16px; right:290px;'  // left of the stats panel
    : 'bottom:80px; left:16px;';

  card.style.cssText = `
    position:fixed; ${posStyle}
    width:300px; z-index:200;
    background:rgba(13,17,23,0.92); border:1px solid #30363d;
    border-radius:10px; padding:16px;
    font-size:12px; line-height:1.5; color:#e6edf3;
    opacity:0; transition:opacity 0.2s;
    backdrop-filter:blur(12px);
  `;

  card.innerHTML = `
    <div style="font-weight:600; font-size:14px; margin-bottom:8px; color:#58a6ff">${_esc(spec.title)}</div>
    <div style="color:#8b949e; margin-bottom:12px">${_esc(spec.body)}</div>
    <div style="display:flex; justify-content:space-between; align-items:center">
      <span style="color:#6e7681; font-size:11px">${index + 1} / ${TOTAL_STEPS}</span>
      <div style="display:flex; gap:8px">
        <button id="tour-skip" style="
          background:none; border:1px solid #30363d; border-radius:6px;
          color:#6e7681; padding:4px 10px; font-size:11px; cursor:pointer; font-family:inherit;
        ">Skip tour</button>
        <button id="tour-next" style="
          background:#58a6ff; border:none; border-radius:6px;
          color:#0d1117; padding:4px 10px; font-size:11px; cursor:pointer;
          font-weight:600; font-family:inherit;
        ">${index === TOTAL_STEPS - 1 ? 'Done' : 'Next →'}</button>
      </div>
    </div>
  `;

  card.querySelector('#tour-next').addEventListener('click', () => {
    _removeCard();
    setTimeout(() => _showStep(index + 1), 150);
  });
  card.querySelector('#tour-skip').addEventListener('click', () => {
    _endTour();
  });

  return card;
}

function _removeCard() {
  if (_card) {
    _card.remove();
    _card = null;
  }
}

function _endTour() {
  _removeCard();
  localStorage.setItem(STORAGE_KEY, '1');
}

function _esc(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
