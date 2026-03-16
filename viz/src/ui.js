import { switchMode } from './modes.js';
import { clearSelection } from './interaction.js';
import { fileNodes } from './layout.js';
import { dirColor } from './colors.js';

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
      <div style="padding:12px 16px; border-bottom:1px solid #21262d">
        <div style="font-size:10px; text-transform:uppercase; letter-spacing:0.8px; color:#6e7681; margin-bottom:8px">Top Hotspots</div>
        <div id="hotspot-list"></div>
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
        border-radius:8px; padding:8px 12px; font-size:11px;
      ">
        <div style="display:flex; align-items:center; gap:8px">
          <div style="width:100px; height:8px; border-radius:4px; background:linear-gradient(90deg,#3fb950,#e3b341,#ff6b6b)"></div>
          <span style="color:#8b949e">Low → High risk</span>
        </div>
      </div>
    </div>
  `;

  const uiContainer = document.createElement('div');
  uiContainer.innerHTML = uiHTML;
  document.body.appendChild(uiContainer);

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
}
