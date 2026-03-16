/**
 * Time scrubber: histogram of monthly commit activity with a draggable handle.
 *
 * Dragging the handle dims files not touched before the cutoff date.
 * Default state (handle at far right) = no dimming = identical to Phase 1.
 *
 * The handle is absolutely positioned over the canvas and visually tracks
 * the drag position across the full histogram width.
 */
import { setScrubberMask } from './nodes.js';

let _commits = null; // { first, last, buckets: [{t, files}] }
let _container = null;
let _canvas = null;
let _cursor = null;  // movable div: handle line + label, positioned by left%
let _label = null;

// Current cutoff unix timestamp. null = all active.
let _cutoff = null;

/**
 * Initialize the scrubber with commit data.
 * @param {Object} commits - data.json "commits" field
 * @param {HTMLElement} container - the scrubber bar container element
 */
export function initScrubber(commits, container) {
  if (!commits || !commits.buckets || commits.buckets.length === 0) {
    container.style.display = 'none';
    return;
  }

  _commits = commits;
  _container = container;
  _cutoff = commits.last; // default: all active

  container.style.cssText = `
    position:fixed; bottom:0; left:0; right:0; height:60px;
    background:rgba(13,17,23,0.85); border-top:1px solid #30363d;
    z-index:10;
  `;

  // Inner wrapper for relative positioning
  const inner = document.createElement('div');
  inner.style.cssText = 'position:relative; width:100%; height:100%;';
  container.appendChild(inner);

  // Histogram canvas (fills inner)
  _canvas = document.createElement('canvas');
  _canvas.style.cssText = 'position:absolute; left:0; top:0; width:100%; height:100%; cursor:crosshair;';
  inner.appendChild(_canvas);

  // Cursor: movable 20px-wide hit area containing the handle line + label.
  // Positioned by left% so it tracks drag position across the full width.
  _cursor = document.createElement('div');
  _cursor.style.cssText = `
    position:absolute; top:0; bottom:0; left:100%;
    width:20px; transform:translateX(-50%); cursor:ew-resize;
  `;

  const handleLine = document.createElement('div');
  handleLine.style.cssText = `
    position:absolute; top:0; bottom:0; left:50%; width:2px;
    background:#58a6ff; transform:translateX(-50%); pointer-events:none;
  `;

  _label = document.createElement('div');
  _label.style.cssText = `
    position:absolute; bottom:100%; left:50%; transform:translateX(-50%);
    background:rgba(13,17,23,0.9); border:1px solid #30363d;
    border-radius:4px; padding:2px 6px; font-size:10px; color:#e6edf3;
    white-space:nowrap; pointer-events:none;
  `;

  _cursor.appendChild(handleLine);
  _cursor.appendChild(_label);
  inner.appendChild(_cursor);

  _drawHistogram(1.0); // full range highlighted
  _updateLabel(_commits.last);

  // Drag logic on the canvas (click-to-jump) and cursor (drag-from-handle)
  let dragging = false;

  function onDragMove(e) {
    if (!dragging) return;
    e.preventDefault();
    const rect = _canvas.getBoundingClientRect();
    const x = Math.max(0, Math.min(e.clientX - rect.left, rect.width));
    const frac = x / rect.width;
    _cutoff = _commits.first + frac * (_commits.last - _commits.first);
    _cursor.style.left = `${frac * 100}%`;
    _applyMask();
    _drawHistogram(frac);
    _updateLabel(_cutoff);
  }

  function onDragEnd() {
    dragging = false;
    window.removeEventListener('mousemove', onDragMove);
    window.removeEventListener('mouseup', onDragEnd);
  }

  _cursor.addEventListener('mousedown', (e) => {
    dragging = true;
    e.preventDefault();
    window.addEventListener('mousemove', onDragMove);
    window.addEventListener('mouseup', onDragEnd);
  });

  _canvas.addEventListener('mousedown', (e) => {
    dragging = true;
    e.preventDefault();
    onDragMove(e); // jump to clicked position immediately
    window.addEventListener('mousemove', onDragMove);
    window.addEventListener('mouseup', onDragEnd);
  });

  // Resize: redraw histogram
  window.addEventListener('resize', () => {
    const frac = _commits.last > _commits.first
      ? (_cutoff - _commits.first) / (_commits.last - _commits.first)
      : 1;
    _drawHistogram(Math.max(0, Math.min(1, frac)));
  });
}

function _applyMask() {
  if (_cutoff >= _commits.last) {
    setScrubberMask(null);
    return;
  }
  const activeSet = new Set();
  for (const bucket of _commits.buckets) {
    if (bucket.t <= _cutoff) {
      for (const id of bucket.files) activeSet.add(id);
    }
  }
  setScrubberMask(activeSet);
}

function _drawHistogram(activeFrac) {
  if (!_canvas || !_commits) return;
  const dpr = window.devicePixelRatio || 1;
  const w = _canvas.clientWidth;
  const h = _canvas.clientHeight;
  if (w === 0 || h === 0) return;

  _canvas.width = Math.round(w * dpr);
  _canvas.height = Math.round(h * dpr);
  const ctx = _canvas.getContext('2d');
  ctx.scale(dpr, dpr);

  const buckets = _commits.buckets;
  const maxFiles = Math.max(...buckets.map(b => b.files.length), 1);
  const barW = w / buckets.length;
  const maxBarH = h - 10;
  const activeX = activeFrac * w;

  for (let i = 0; i < buckets.length; i++) {
    const bh = (buckets[i].files.length / maxFiles) * maxBarH;
    const bx = i * barW;
    const isActive = bx <= activeX;
    ctx.fillStyle = isActive ? 'rgba(88,166,255,0.7)' : 'rgba(88,166,255,0.2)';
    ctx.fillRect(bx + 1, h - bh, barW - 2, bh);
  }
}

function _updateLabel(ts) {
  if (!_label) return;
  const d = new Date(ts * 1000);
  const months = ['Jan','Feb','Mar','Apr','May','Jun','Jul','Aug','Sep','Oct','Nov','Dec'];
  _label.textContent = `${months[d.getUTCMonth()]} ${d.getUTCFullYear()}`;
}
