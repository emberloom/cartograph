import { switchMode } from './modes.js';
import { clearSelection } from './interaction.js';
import { startTour } from './tour.js';
import { fileNodes } from './layout.js';
import { dirColor, ownerColor } from './colors.js';
import { setOwnerFilter, setRiskFilter, setDegreeFilter, setReachableFilter,
         getOwnerFilter, getRiskMin, RISK_BANDS } from './filters.js';

/** Escape HTML special characters to prevent XSS from repo data. */
function esc(str) {
  return String(str)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

/**
 * Initialize all DOM UI panels.
 * @param {Object} data - full data.json
 * @param {Array} topDirs - unique top-level directory names
 */
export function initUI(data, topDirs) {
  // ── Build UI HTML ──
  const uiHTML = `
    <!-- Top-right: stats + hotspots -->
    <div id="panel" style="
      position:fixed; top:16px; right:16px; width:260px;
      background:rgba(13,17,23,0.92); border:1px solid #30363d;
      border-radius:10px; backdrop-filter:blur(12px);
      font-size:12px; line-height:1.5; z-index:10;
      max-height:calc(100vh - 32px); overflow-y:auto;
    ">
      <div style="padding:14px 16px 10px; border-bottom:1px solid #21262d">
        <div style="font-size:15px; font-weight:700; color:#58a6ff">Cartograph</div>
        <div style="color:#8b949e; font-size:11px">${esc(data.repo)}</div>
      </div>
      <div style="padding:12px 16px; border-bottom:1px solid #21262d">
        <div style="font-size:10px; text-transform:uppercase; letter-spacing:0.8px; color:#6e7681; margin-bottom:8px">Stats</div>
        <div style="display:flex; justify-content:space-between; margin-bottom:4px"><span style="color:#8b949e">Files</span><span style="font-weight:600">${esc(data.stats.files)}${data.stats.total_files > data.stats.files ? ' / ' + esc(data.stats.total_files) : ''}</span></div>
        <div style="display:flex; justify-content:space-between; margin-bottom:4px"><span style="color:#8b949e">Directories</span><span style="font-weight:600">${esc(data.stats.directories)}</span></div>
        <div style="display:flex; justify-content:space-between; margin-bottom:4px"><span style="color:#8b949e">Import edges</span><span style="font-weight:600">${esc(data.stats.struct_edges)}</span></div>
        <div style="display:flex; justify-content:space-between; margin-bottom:4px"><span style="color:#8b949e">Co-change pairs</span><span style="font-weight:600">${esc(data.stats.cochange_edges)}</span></div>
      <div style="display:flex; justify-content:space-between; margin-bottom:4px"><span style="color:#8b949e">Owners</span><span style="font-weight:600">${data.stats.owners ? esc(data.stats.owners) : '—'}</span></div>
      </div>
      <div id="filter-chips-section" style="display:none; padding:8px 16px; border-bottom:1px solid #21262d">
        <div id="filter-chips" style="display:flex; flex-direction:column; gap:4px;"></div>
      </div>
      <div style="padding:12px 16px; border-bottom:1px solid #21262d">
        <div style="font-size:10px; text-transform:uppercase; letter-spacing:0.8px; color:#6e7681; margin-bottom:8px">Top Hotspots</div>
        <div id="hotspot-list"></div>
      </div>
      <div style="padding:8px 16px; border-top:1px solid #21262d;">
        <div style="color:#6e7681;font-size:10px;margin-bottom:6px;text-transform:uppercase;letter-spacing:0.05em">Connectivity</div>
        <div style="display:flex;flex-direction:column;gap:6px;">
          <div>
            <div style="display:flex;justify-content:space-between;color:#8b949e;font-size:10px;margin-bottom:2px;">
              <span>Degree</span><span id="degree-readout">≥ 0</span>
            </div>
            <input id="degree-slider" type="range" min="0" max="57" step="1" value="0"
              style="width:100%;accent-color:#58a6ff;cursor:pointer;">
          </div>
          <div>
            <div style="display:flex;justify-content:space-between;color:#8b949e;font-size:10px;margin-bottom:2px;">
              <span>Reach (3-hop)</span><span id="reachable-readout">≥ 0</span>
            </div>
            <input id="reachable-slider" type="range" min="0" max="100" step="1" value="0"
              style="width:100%;accent-color:#58a6ff;cursor:pointer;">
          </div>
        </div>
      </div>
    </div>

    <!-- Top-left: controls -->
    <div id="controls" style="
      position:fixed; top:16px; left:16px; z-index:10;
      display:flex; flex-direction:column; gap:8px;
    ">
      <div style="display:flex; gap:4px">
        <button class="mode-btn active" data-mode="architecture" style="
          background:rgba(13,17,23,0.9); border:1px solid #30363d;
          border-radius:6px; color:#58a6ff; padding:6px 12px;
          font-size:11px; font-family:inherit; cursor:pointer;
        ">Architecture</button>
        <button class="mode-btn" data-mode="risk" style="
          background:rgba(13,17,23,0.9); border:1px solid #30363d;
          border-radius:6px; color:#8b949e; padding:6px 12px;
          font-size:11px; font-family:inherit; cursor:pointer;
        ">Risk</button>
        <button class="mode-btn" data-mode="ownership" style="
          background:rgba(13,17,23,0.9); border:1px solid #30363d;
          border-radius:6px; color:#8b949e; padding:6px 12px;
          font-size:11px; font-family:inherit; cursor:pointer;
        ">Ownership</button>
      </div>
      <div style="display:flex; gap:4px">
        <button class="layer-btn active" data-layer="imports" style="
          background:rgba(13,17,23,0.9); border:1px solid #30363d;
          border-radius:6px; color:#58a6ff; padding:6px 12px;
          font-size:11px; font-family:inherit; cursor:pointer;
        ">Imports</button>
        <button class="layer-btn" data-layer="cochange" style="
          background:rgba(13,17,23,0.9); border:1px solid #30363d;
          border-radius:6px; color:#8b949e; padding:6px 12px;
          font-size:11px; font-family:inherit; cursor:pointer;
        ">Co-change</button>
      </div>
      <div style="position:relative">
        <input id="search-input" type="text" placeholder="Search files..."
          style="
            width:200px; background:rgba(13,17,23,0.9);
            border:1px solid #30363d; border-radius:6px;
            color:#e6edf3; padding:6px 12px; font-size:11px;
            font-family:inherit; outline:none;
          "
        />
        <div id="search-results" style="
          display:none; position:absolute; top:100%; left:0;
          width:300px; max-height:200px; overflow-y:auto;
          background:rgba(13,17,23,0.95); border:1px solid #30363d;
          border-radius:6px; margin-top:4px; z-index:20;
        "></div>
      </div>
      <button id="tour-btn" style="
        background:rgba(13,17,23,0.9); border:1px solid #30363d;
        border-radius:6px; color:#8b949e; padding:6px 10px;
        font-size:11px; font-family:inherit; cursor:pointer; align-self:flex-start;
      ">?</button>
    </div>

    <!-- Bottom-left: selection detail -->
    <div id="selected-panel" style="
      display:none; position:fixed; bottom:16px; left:16px; width:300px;
      background:rgba(13,17,23,0.92); border:1px solid #30363d;
      border-radius:10px; backdrop-filter:blur(12px);
      font-size:12px; line-height:1.5; z-index:10; padding:14px 16px;
    ">
      <button id="close-sel" style="
        position:absolute; top:10px; right:12px;
        background:none; border:none; color:#6e7681;
        cursor:pointer; font-size:16px;
      ">×</button>
      <div id="sel-name" style="font-size:13px; color:#58a6ff; margin-bottom:8px; word-break:break-all"></div>
      <div id="sel-details"></div>
      <div id="sel-cochanges" style="margin-top:8px; display:none">
        <div style="font-size:10px; text-transform:uppercase; letter-spacing:0.8px; color:#6e7681; margin-bottom:4px">Co-changes</div>
        <div id="sel-cochange-list"></div>
      </div>
    </div>

    <!-- Bottom-right: legend -->
    <div style="position:fixed; bottom:16px; right:16px; z-index:10">
      <div id="legend-arch" style="
        background:rgba(13,17,23,0.85); border:1px solid #30363d;
        border-radius:8px; padding:8px 12px; font-size:11px;
        display:flex; flex-direction:column; gap:4px; max-height:200px; overflow-y:auto;
      "></div>
      <div id="legend-risk" style="
        display:none; background:rgba(13,17,23,0.85); border:1px solid #30363d;
        border-radius:8px; padding:8px 12px; font-size:11px; flex-direction:column; gap:8px;
      ">
        <div style="display:flex; gap:4px;">
          <button id="risk-band-low"  style="flex:1;border:1px solid #30363d;border-radius:5px;background:rgba(13,17,23,0.9);color:#8b949e;padding:4px;font-size:10px;cursor:pointer;font-family:inherit;">Low ≥1%</button>
          <button id="risk-band-med"  style="flex:1;border:1px solid #30363d;border-radius:5px;background:rgba(13,17,23,0.9);color:#8b949e;padding:4px;font-size:10px;cursor:pointer;font-family:inherit;">Med ≥10%</button>
          <button id="risk-band-high" style="flex:1;border:1px solid #30363d;border-radius:5px;background:rgba(13,17,23,0.9);color:#8b949e;padding:4px;font-size:10px;cursor:pointer;font-family:inherit;">High ≥33%</button>
        </div>
        <input id="risk-filter-slider" type="range" min="0" max="1" step="0.01" value="0"
          style="width:100%;accent-color:#58a6ff;cursor:pointer;">
        <div style="display:flex;justify-content:space-between;color:#6e7681;font-size:10px;">
          <span>0%</span><span>100%</span>
        </div>
      </div>
      <div id="legend-ownership" style="
        display:none; background:rgba(13,17,23,0.85); border:1px solid #30363d;
        border-radius:8px; padding:8px 12px; font-size:11px;
        flex-direction:column; gap:4px; max-height:200px; overflow-y:auto;
      "></div>
    </div>
  `;

  const uiContainer = document.createElement('div');
  uiContainer.innerHTML = uiHTML;
  document.body.appendChild(uiContainer);

  // ── Filter chips ──
  const _CHIP_PREFIXES = {
    'owner-filter-chip': 'Owner', 'risk-filter-chip': 'Risk',
    'degree-filter-chip': 'Degree', 'reachable-filter-chip': 'Reach',
  };
  function _makeChip(id, labelId, onClear) {
    const chip = document.createElement('div');
    chip.id = id;
    chip.style.cssText = `
      display:none; align-items:center; gap:6px;
      background:rgba(88,166,255,0.12); border:1px solid rgba(88,166,255,0.3);
      border-radius:6px; padding:4px 8px; font-size:11px; color:#58a6ff;
    `;
    chip.innerHTML = `
      <span style="color:#6e7681;font-size:10px">${_CHIP_PREFIXES[id] || ''}</span>
      <span id="${labelId}" style="font-weight:600;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;max-width:140px"></span>
      <button style="background:none;border:none;color:#58a6ff;cursor:pointer;font-size:13px;padding:0;line-height:1;margin-left:auto">×</button>
    `;
    chip.querySelector('button').addEventListener('click', onClear);
    return chip;
  }
  const chipsContainer = document.getElementById('filter-chips');
  chipsContainer.appendChild(_makeChip('owner-filter-chip',    'owner-filter-label',
    () => setOwnerFilter(null)));
  chipsContainer.appendChild(_makeChip('risk-filter-chip',     'risk-filter-label',
    () => { setRiskFilter(null); document.getElementById('risk-filter-slider').value = 0; }));
  chipsContainer.appendChild(_makeChip('degree-filter-chip',   'degree-filter-label',
    () => {
      setDegreeFilter(null);
      document.getElementById('degree-slider').value = 0;
      document.getElementById('degree-readout').textContent = '≥ 0';
    }));
  chipsContainer.appendChild(_makeChip('reachable-filter-chip','reachable-filter-label',
    () => {
      setReachableFilter(null);
      document.getElementById('reachable-slider').value = 0;
      document.getElementById('reachable-readout').textContent = '≥ 0';
    }));

  // ── Risk band buttons and slider ──
  ['low', 'med', 'high'].forEach(band => {
    document.getElementById(`risk-band-${band}`).addEventListener('click', () => {
      const threshold = RISK_BANDS[band];
      const current = getRiskMin();
      const next = (current !== null && Math.abs(current - threshold) < 0.001) ? null : threshold;
      setRiskFilter(next);
      document.getElementById('risk-filter-slider').value = next !== null ? next : 0;
    });
  });
  document.getElementById('risk-filter-slider').addEventListener('input', (e) => {
    const val = parseFloat(e.target.value);
    setRiskFilter(val > 0 ? val : null);
  });

  // ── Connectivity sliders ──
  document.getElementById('degree-slider').addEventListener('input', (e) => {
    const val = parseInt(e.target.value);
    document.getElementById('degree-readout').textContent = `≥ ${val}`;
    setDegreeFilter(val);
  });
  document.getElementById('reachable-slider').addEventListener('input', (e) => {
    const val = parseInt(e.target.value);
    document.getElementById('reachable-readout').textContent = `≥ ${val}`;
    setReachableFilter(val);
  });

  // ── Hotspots ──
  const hotspotList = document.getElementById('hotspot-list');
  const nodeById = Object.fromEntries(fileNodes.map(fn => [fn.id, fn]));
  for (const h of data.hotspots) {
    const div = document.createElement('div');
    div.style.cssText = 'display:flex; align-items:center; gap:8px; padding:4px 6px; border-radius:5px; cursor:pointer;';
    div.onmouseenter = () => div.style.background = '#161b22';
    div.onmouseleave = () => div.style.background = '';
    const fn = nodeById[h.id];
    const color = fn ? dirColor(fn.topLevelDirIdx) : '#58a6ff';
    div.innerHTML = `
      <div style="width:8px;height:8px;border-radius:50%;background:${color};flex-shrink:0"></div>
      <span style="flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap" title="${esc(h.path)}">${esc(h.label)}</span>
      <span style="color:#6e7681">${h.degree}</span>
    `;
    div.onclick = () => {
      window.dispatchEvent(new CustomEvent('navigate-to-node', { detail: { id: h.id } }));
    };
    hotspotList.appendChild(div);
  }

  // ── Architecture legend ──
  const legendArch = document.getElementById('legend-arch');
  for (let i = 0; i < topDirs.length && i < 20; i++) {
    const row = document.createElement('div');
    row.style.cssText = 'display:flex; align-items:center; gap:8px;';
    row.innerHTML = `
      <div style="width:10px;height:10px;border-radius:50%;background:${dirColor(i)};flex-shrink:0"></div>
      <span style="color:#8b949e">${esc(topDirs[i])}</span>
    `;
    legendArch.appendChild(row);
  }

  // ── Ownership legend (populated on mode-changed event) ──
  window.addEventListener('mode-changed', (e) => {
    if (e.detail.mode !== 'ownership') return;
    const legendOwnership = document.getElementById('legend-ownership');
    if (!legendOwnership) return;
    legendOwnership.innerHTML = '';

    // Count files per owner
    const ownerFileCounts = new Map();
    for (const fn of fileNodes) {
      const o = fn.owner || 'unowned';
      ownerFileCounts.set(o, (ownerFileCounts.get(o) || 0) + 1);
    }

    // Sort: desc by file count, then alpha by name. "unowned" always last.
    const owners = [...ownerFileCounts.entries()]
      .filter(([o]) => o !== 'unowned')
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .slice(0, 10);

    const unownedCount = ownerFileCounts.get('unowned') || 0;

    for (const [owner, count] of owners) {
      const row = document.createElement('div');
      row.className = 'owner-legend-row';
      row.dataset.owner = owner;
      row.style.cssText = 'display:flex; align-items:center; gap:8px; cursor:pointer; padding:2px 4px;';
      row.onmouseenter = () => { if (getOwnerFilter() !== owner) row.style.background = 'rgba(255,255,255,0.05)'; };
      row.onmouseleave = () => { if (getOwnerFilter() !== owner) row.style.background = ''; };
      row.innerHTML = `
        <div style="width:10px;height:10px;border-radius:50%;background:${ownerColor(owner)};flex-shrink:0"></div>
        <span style="color:#8b949e;flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">${esc(owner)}</span>
        <span style="color:#6e7681">${count}</span>
      `;
      row.addEventListener('click', () => {
        const current = getOwnerFilter();
        setOwnerFilter(current === owner ? null : owner); // toggle
      });
      legendOwnership.appendChild(row);
    }

    if (unownedCount > 0) {
      const row = document.createElement('div');
      row.style.cssText = 'display:flex; align-items:center; gap:8px;';
      row.innerHTML = `
        <div style="width:10px;height:10px;border-radius:50%;background:#484f58;flex-shrink:0"></div>
        <span style="color:#6e7681;flex:1">Unowned</span>
        <span style="color:#6e7681">${unownedCount}</span>
      `;
      legendOwnership.appendChild(row);
    }
  });

  // ── Mode buttons ──
  document.querySelectorAll('.mode-btn').forEach(btn => {
    btn.addEventListener('click', () => switchMode(btn.dataset.mode));
  });

  // ── Layer toggles ──
  const layerState = { imports: true, cochange: false };
  document.querySelectorAll('.layer-btn').forEach(btn => {
    btn.addEventListener('click', () => {
      const layer = btn.dataset.layer;
      layerState[layer] = !layerState[layer];
      btn.style.color = layerState[layer] ? '#58a6ff' : '#8b949e';
      btn.classList.toggle('active', layerState[layer]);
      window.dispatchEvent(new CustomEvent('layer-toggle', {
        detail: { layer, visible: layerState[layer] },
      }));
    });
  });

  // ── Tour button ──
  document.getElementById('tour-btn').addEventListener('click', () => startTour());

  // ── Close selection ──
  document.getElementById('close-sel').addEventListener('click', clearSelection);

  // ── Search ──
  const searchInput = document.getElementById('search-input');
  const searchResults = document.getElementById('search-results');

  searchInput.addEventListener('input', () => {
    const q = searchInput.value.toLowerCase().trim();
    if (q.length < 2) {
      searchResults.style.display = 'none';
      return;
    }
    const matches = fileNodes
      .filter(fn => fn.path.toLowerCase().includes(q))
      .slice(0, 10);

    if (matches.length === 0) {
      searchResults.style.display = 'none';
      return;
    }

    searchResults.innerHTML = matches.map(fn => `
      <div class="search-item" data-id="${fn.id}" style="
        padding:6px 12px; cursor:pointer; color:#e6edf3;
        border-bottom:1px solid #21262d; font-size:11px;
      ">${esc(fn.path)}</div>
    `).join('');
    searchResults.style.display = 'block';

    searchResults.querySelectorAll('.search-item').forEach(item => {
      item.onmouseenter = () => item.style.background = '#161b22';
      item.onmouseleave = () => item.style.background = '';
      item.onclick = () => {
        const id = parseInt(item.dataset.id);
        window.dispatchEvent(new CustomEvent('navigate-to-node', { detail: { id } }));
        searchResults.style.display = 'none';
        searchInput.value = '';
      };
    });
  });

  searchInput.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') {
      searchResults.style.display = 'none';
      searchInput.value = '';
      searchInput.blur();
    }
  });

  // ── Selection events ──
  window.addEventListener('node-selected', (e) => {
    const { node, blastCount, cochanges } = e.detail;
    const panel = document.getElementById('selected-panel');
    panel.style.display = 'block';
    document.getElementById('sel-name').textContent = node.name;
    document.getElementById('sel-details').innerHTML = `
      <div style="display:flex;gap:8px;margin-bottom:4px"><span style="color:#6e7681;width:80px">Path</span><span style="word-break:break-all">${esc(node.path)}</span></div>
      <div style="display:flex;gap:8px;margin-bottom:4px"><span style="color:#6e7681;width:80px">Directory</span><span>${esc(node.topLevelDir)}</span></div>
      <div style="display:flex;gap:8px;margin-bottom:4px"><span style="color:#6e7681;width:80px">Owner</span><span>${esc(node.owner)}</span></div>
      <div style="display:flex;gap:8px;margin-bottom:4px"><span style="color:#6e7681;width:80px">Connections</span><span>${node.degree}</span></div>
      <div style="display:flex;gap:8px;margin-bottom:4px"><span style="color:#6e7681;width:80px">Risk score</span><span>${Math.round(node.riskScore * 100)}%</span></div>
      <div style="display:flex;gap:8px;margin-bottom:4px"><span style="color:#6e7681;width:80px">Blast radius</span><span>${blastCount} files</span></div>
    `;

    const ccPanel = document.getElementById('sel-cochanges');
    const ccList = document.getElementById('sel-cochange-list');
    if (cochanges.length > 0) {
      ccPanel.style.display = 'block';
      const nodeById = Object.fromEntries(fileNodes.map(fn => [fn.id, fn]));
      ccList.innerHTML = cochanges.slice(0, 6).map(cc => {
        const other = nodeById[cc.t];
        const label = other ? other.name : `#${cc.t}`;
        return `<div style="display:flex;justify-content:space-between;padding:3px 0;color:#e3b341;font-size:11px;border-bottom:1px solid #21262d"><span>${esc(label)}</span><span>${Math.round(cc.c * 100)}%</span></div>`;
      }).join('');
    } else {
      ccPanel.style.display = 'none';
    }
  });

  window.addEventListener('node-deselected', () => {
    document.getElementById('selected-panel').style.display = 'none';
  });

  // ── Scrubber container (populated by initScrubber in main.js) ──
  const scrubberContainer = document.createElement('div');
  scrubberContainer.id = 'scrubber-container';
  document.body.appendChild(scrubberContainer);
}
