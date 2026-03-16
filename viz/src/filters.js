import { setFilterMask } from './nodes.js';
import { fileNodes } from './layout.js';

// ── Filter state ──
let _ownerFilter = null;   // string | null
let _riskMin = null;       // number | null  (0–1)
let _degreeMin = null;     // number | null  (>0 only — 0 normalised to null at setter)
let _reachableMin = null;  // number | null  (>0 only — 0 normalised to null at setter)
let _blastCounts = [];     // number[], indexed by node id (contiguous 0..N-1)

export const RISK_BANDS = { low: 0.01, med: 0.10, high: 0.33 };

/**
 * Call once from main.js after initInteraction (blast counts must be ready).
 * Must be called before any filter setter that involves reachable counts.
 * If called after a setter fires, _blastCounts is [] and all nodes get 0, which is
 * handled safely by (_blastCounts[fn.id] || 0) — nodes are incorrectly excluded from
 * a reachable filter, but this ordering violation cannot happen given main.js call order.
 * @param {number[]} blastCounts - reachable count per node id, length === fileNodes.length
 */
export function initFilters(blastCounts) {
  _blastCounts = blastCounts;
}

// Setters normalise 0 → null for degree/reachable so _apply() has a clean invariant
export function setOwnerFilter(name)    { _ownerFilter = name || null;             _apply(); }
export function setRiskFilter(min)      { _riskMin = min;                         _apply(); }
export function setDegreeFilter(min)    { _degreeMin = (min > 0) ? min : null;    _apply(); }
export function setReachableFilter(min) { _reachableMin = (min > 0) ? min : null; _apply(); }

export function getOwnerFilter()  { return _ownerFilter; }
export function getRiskMin()      { return _riskMin; }
export function getDegreeMin()    { return _degreeMin; }
export function getReachableMin() { return _reachableMin; }

function _apply() {
  const anyActive = _ownerFilter !== null || _riskMin !== null ||
    _degreeMin !== null || _reachableMin !== null;

  if (!anyActive) {
    setFilterMask(null);
  } else {
    const active = new Set();
    for (const fn of fileNodes) {
      if (_ownerFilter !== null && fn.owner !== _ownerFilter) continue;
      if (_riskMin !== null && fn.riskScore < _riskMin) continue;
      if (_degreeMin !== null && fn.degree < _degreeMin) continue;
      if (_reachableMin !== null && (_blastCounts[fn.id] || 0) < _reachableMin) continue;
      active.add(fn.id);
    }
    setFilterMask(active);
  }

  _updateAllUI();
}

function _updateAllUI() {
  // Owner chip
  _setChip('owner-filter-chip', 'owner-filter-label',
    _ownerFilter ? _ownerFilter : null);

  // Risk chip
  _setChip('risk-filter-chip', 'risk-filter-label',
    _riskMin !== null ? `≥ ${Math.round(_riskMin * 100)}%` : null);

  // Degree chip
  _setChip('degree-filter-chip', 'degree-filter-label',
    _degreeMin !== null ? `≥ ${_degreeMin}` : null);

  // Reachable chip
  _setChip('reachable-filter-chip', 'reachable-filter-label',
    _reachableMin !== null ? `≥ ${_reachableMin}` : null);

  // Owner legend rows
  document.querySelectorAll('.owner-legend-row').forEach(row => {
    const isActive = _ownerFilter !== null && row.dataset.owner === _ownerFilter;
    row.style.background = isActive ? 'rgba(88,166,255,0.1)' : '';
    row.style.borderRadius = isActive ? '4px' : '';
  });

  // Risk band buttons
  for (const [band, threshold] of Object.entries(RISK_BANDS)) {
    const btn = document.getElementById(`risk-band-${band}`);
    if (!btn) continue;
    const isActive = _riskMin !== null && Math.abs(_riskMin - threshold) < 0.001;
    btn.style.background = isActive ? 'rgba(88,166,255,0.15)' : 'rgba(13,17,23,0.9)';
    btn.style.color = isActive ? '#58a6ff' : '#8b949e';
    btn.style.borderColor = isActive ? 'rgba(88,166,255,0.4)' : '#30363d';
  }

  // Risk slider thumb position
  const riskSlider = document.getElementById('risk-filter-slider');
  if (riskSlider) riskSlider.value = _riskMin !== null ? _riskMin : 0;

  // Show/hide chip section container
  const chipSection = document.getElementById('filter-chips-section');
  if (chipSection) {
    const anyChip = _ownerFilter !== null || _riskMin !== null ||
      _degreeMin !== null || _reachableMin !== null;
    chipSection.style.display = anyChip ? 'block' : 'none';
  }
}

function _setChip(chipId, labelId, text) {
  const chip = document.getElementById(chipId);
  if (!chip) return;
  if (text) {
    chip.style.display = 'flex';
    chip.querySelector(`#${labelId}`).textContent = text;
  } else {
    chip.style.display = 'none';
  }
}

// Re-apply owner legend row highlights after legend is rebuilt on mode switch.
// ui.js fires mode-changed BEFORE rebuilding the legend DOM, so a microtask
// defers one turn and reliably sees the freshly-appended rows.
window.addEventListener('mode-changed', (e) => {
  if (e.detail.mode === 'ownership' && _ownerFilter) {
    Promise.resolve().then(() => {
      document.querySelectorAll('.owner-legend-row').forEach(row => {
        const isActive = row.dataset.owner === _ownerFilter;
        row.style.background = isActive ? 'rgba(88,166,255,0.1)' : '';
        row.style.borderRadius = isActive ? '4px' : '';
      });
    });
  }
});
