#!/usr/bin/env python3
"""
Emberloom Cartograph — Interactive Visualizer

Generates a self-contained HTML visualization from a .cartograph/db.sqlite database.

Usage:
    python3 scripts/viz.py \\
        --db /path/to/repo/.cartograph/db.sqlite \\
        --repo-name "ripgrep" \\
        --out docs/demo.html

The output is a single HTML file with no external dependencies.
Open it in any browser.
"""

import sqlite3
import json
import argparse
import sys
import math
from pathlib import Path
from collections import defaultdict


# ---------------------------------------------------------------------------
# Data loading
# ---------------------------------------------------------------------------

def load_data(db_path: str, cochange_threshold: float = 0.25):
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row

    # File entities only
    files = conn.execute("""
        SELECT id, name, path FROM entities WHERE kind = 'File' ORDER BY path
    """).fetchall()

    file_ids = {r["id"] for r in files}

    # All edges
    all_edges = conn.execute("""
        SELECT from_id, to_id, kind, confidence FROM edges
    """).fetchall()

    # Person name lookup
    persons = conn.execute("""
        SELECT id, name, path as email FROM entities WHERE kind = 'Person'
    """).fetchall()
    person_name = {r["id"]: r["name"] for r in persons}
    person_email = {r["id"]: r["email"] for r in persons}

    conn.close()

    # Build primary owner per file (highest confidence owned_by edge)
    owner_of: dict[str, tuple[str, str]] = {}  # file_id -> (name, email)
    owner_confidence: dict[str, float] = {}
    for e in all_edges:
        if e["kind"] == "owned_by" and e["from_id"] in file_ids:
            pid = e["to_id"]
            conf = e["confidence"]
            if e["from_id"] not in owner_confidence or conf > owner_confidence[e["from_id"]]:
                owner_confidence[e["from_id"]] = conf
                owner_of[e["from_id"]] = (
                    person_name.get(pid, "unknown"),
                    person_email.get(pid, ""),
                )

    # Structural edges between files
    struct_edges = [
        e for e in all_edges
        if e["kind"] in ("imports", "depends_on")
        and e["from_id"] in file_ids
        and e["to_id"] in file_ids
    ]

    # Co-change edges between files, filtered by threshold
    cochange_edges = [
        e for e in all_edges
        if e["kind"] == "co_changes_with"
        and e["from_id"] in file_ids
        and e["to_id"] in file_ids
        and e["confidence"] >= cochange_threshold
    ]

    # Degree (structural only, for node sizing)
    degree: dict[str, int] = defaultdict(int)
    for e in struct_edges:
        degree[e["from_id"]] += 1
        degree[e["to_id"]] += 1
    # Add co-change degree too
    for e in cochange_edges:
        degree[e["from_id"]] += 1
        degree[e["to_id"]] += 1

    return files, struct_edges, cochange_edges, owner_of, degree


def extract_crate(path: str) -> str:
    """Extract the crate/top-level group from a file path."""
    parts = path.split("/")
    if parts[0] == "crates" and len(parts) >= 2:
        return parts[1]
    if len(parts) == 1:
        return "(root)"
    return parts[0] if parts[0] else "(root)"


def short_label(path: str) -> str:
    """Short display name for a file."""
    return Path(path).name


def build_graph_json(files, struct_edges, cochange_edges, owner_of, degree, repo_name: str) -> str:
    # Assign stable integer indices
    id_to_idx = {r["id"]: i for i, r in enumerate(files)}

    # Collect unique owners for legend + color assignment
    owner_counts: dict[str, int] = defaultdict(int)
    for fid, (name, email) in owner_of.items():
        owner_counts[name] += 1
    top_owners = sorted(owner_counts.items(), key=lambda x: -x[1])
    owner_to_idx = {name: i for i, (name, _) in enumerate(top_owners)}

    # Collect crates
    crates = sorted(set(extract_crate(r["path"]) for r in files))
    crate_to_idx = {c: i for i, c in enumerate(crates)}

    # Build nodes
    nodes = []
    for r in files:
        fid = r["id"]
        owner_name, owner_email = owner_of.get(fid, ("unowned", ""))
        crate = extract_crate(r["path"])
        nodes.append({
            "id": id_to_idx[fid],
            "label": short_label(r["path"]),
            "path": r["path"],
            "crate": crate,
            "crate_idx": crate_to_idx[crate],
            "owner": owner_name,
            "owner_email": owner_email,
            "owner_idx": owner_to_idx.get(owner_name, -1),
            "degree": degree.get(fid, 0),
        })

    # Build structural links
    struct_links = []
    for e in struct_edges:
        if e["from_id"] in id_to_idx and e["to_id"] in id_to_idx:
            struct_links.append({
                "source": id_to_idx[e["from_id"]],
                "target": id_to_idx[e["to_id"]],
                "kind": "struct",
            })

    # Build co-change links
    cochange_links = []
    seen = set()
    for e in cochange_edges:
        if e["from_id"] in id_to_idx and e["to_id"] in id_to_idx:
            a, b = id_to_idx[e["from_id"]], id_to_idx[e["to_id"]]
            key = (min(a, b), max(a, b))
            if key not in seen:
                seen.add(key)
                cochange_links.append({
                    "source": a,
                    "target": b,
                    "kind": "cochange",
                    "confidence": round(e["confidence"], 3),
                })

    # Build adjacency for blast-radius (structural only)
    adj: dict[int, list[int]] = defaultdict(list)
    for lk in struct_links:
        adj[lk["source"]].append(lk["target"])
        adj[lk["target"]].append(lk["source"])

    # Pre-compute blast radius (BFS depth 3) per node
    blast: dict[int, list[int]] = {}
    for node in nodes:
        nid = node["id"]
        visited = {nid}
        frontier = [nid]
        for _ in range(3):
            next_f = []
            for n in frontier:
                for nb in adj[n]:
                    if nb not in visited:
                        visited.add(nb)
                        next_f.append(nb)
            frontier = next_f
        blast[nid] = [x for x in visited if x != nid]

    # Top hotspots (by degree)
    hotspots = sorted(nodes, key=lambda n: -n["degree"])[:10]

    data = {
        "repo": repo_name,
        "stats": {
            "files": len(nodes),
            "struct_edges": len(struct_links),
            "cochange_edges": len(cochange_links),
            "owners": len(top_owners),
            "crates": len(crates),
        },
        "nodes": nodes,
        "struct_links": struct_links,
        "cochange_links": cochange_links,
        "blast": blast,
        "hotspots": [{"id": h["id"], "label": h["label"], "path": h["path"], "degree": h["degree"]} for h in hotspots],
        "owners": [{"name": n, "count": c} for n, c in top_owners],
        "crates": crates,
    }
    return json.dumps(data, separators=(",", ":"))


# ---------------------------------------------------------------------------
# HTML template
# ---------------------------------------------------------------------------

HTML_TEMPLATE = r"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Cartograph — {repo_name}</title>
<script src="https://d3js.org/d3.v7.min.js"></script>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #0d1117; color: #e6edf3; font-family: 'SF Mono', 'Fira Code', monospace; overflow: hidden; }

  #canvas { width: 100vw; height: 100vh; }

  /* ── Panel ── */
  #panel {
    position: fixed; top: 16px; right: 16px;
    width: 280px;
    background: rgba(13,17,23,0.92);
    border: 1px solid #30363d;
    border-radius: 10px;
    backdrop-filter: blur(12px);
    font-size: 12px;
    line-height: 1.5;
    z-index: 10;
    max-height: calc(100vh - 32px);
    overflow-y: auto;
  }
  #panel-header {
    padding: 14px 16px 10px;
    border-bottom: 1px solid #21262d;
  }
  #panel-header h1 {
    font-size: 15px; font-weight: 700; color: #58a6ff;
    letter-spacing: -0.3px;
  }
  #panel-header .subtitle { color: #8b949e; font-size: 11px; margin-top: 2px; }
  .section { padding: 12px 16px; border-bottom: 1px solid #21262d; }
  .section:last-child { border-bottom: none; }
  .section h2 { font-size: 10px; text-transform: uppercase; letter-spacing: 0.8px; color: #6e7681; margin-bottom: 8px; }
  .stat-row { display: flex; justify-content: space-between; margin-bottom: 4px; }
  .stat-label { color: #8b949e; }
  .stat-value { color: #e6edf3; font-weight: 600; }
  .hotspot-item {
    display: flex; align-items: center; gap: 8px;
    padding: 4px 6px; border-radius: 5px; cursor: pointer;
    transition: background 0.15s;
  }
  .hotspot-item:hover { background: #161b22; }
  .hotspot-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .hotspot-name { color: #e6edf3; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hotspot-deg { color: #6e7681; }
  .owner-item { display: flex; align-items: center; gap: 8px; margin-bottom: 5px; }
  .owner-dot { width: 10px; height: 10px; border-radius: 50%; flex-shrink: 0; }
  .owner-name { color: #c9d1d9; flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .owner-count { color: #6e7681; font-size: 11px; }

  /* ── Selected node panel ── */
  #selected-panel {
    position: fixed; bottom: 16px; left: 16px;
    width: 320px;
    background: rgba(13,17,23,0.92);
    border: 1px solid #30363d;
    border-radius: 10px;
    backdrop-filter: blur(12px);
    font-size: 12px;
    line-height: 1.5;
    z-index: 10;
    display: none;
    padding: 14px 16px;
  }
  #selected-panel h2 { font-size: 13px; color: #58a6ff; margin-bottom: 8px; overflow-wrap: break-word; }
  #selected-panel .detail-row { display: flex; gap: 8px; margin-bottom: 4px; }
  #selected-panel .detail-label { color: #6e7681; width: 80px; flex-shrink: 0; }
  #selected-panel .detail-value { color: #e6edf3; }
  #selected-panel .cochange-list { margin-top: 8px; }
  #selected-panel .cochange-item {
    display: flex; justify-content: space-between;
    padding: 3px 0; color: #e3b341; font-size: 11px;
    border-bottom: 1px solid #21262d;
  }
  #selected-panel .cochange-item:last-child { border-bottom: none; }
  #close-btn {
    position: absolute; top: 10px; right: 12px;
    background: none; border: none; color: #6e7681; cursor: pointer;
    font-size: 16px; line-height: 1;
  }
  #close-btn:hover { color: #e6edf3; }

  /* ── Controls ── */
  #controls {
    position: fixed; bottom: 16px; right: 16px;
    display: flex; gap: 8px; z-index: 10;
  }
  .ctrl-btn {
    background: rgba(13,17,23,0.9);
    border: 1px solid #30363d;
    border-radius: 6px;
    color: #8b949e;
    padding: 6px 12px;
    font-size: 11px;
    font-family: inherit;
    cursor: pointer;
    transition: all 0.15s;
  }
  .ctrl-btn:hover { background: #161b22; color: #e6edf3; border-color: #58a6ff; }
  .ctrl-btn.active { color: #58a6ff; border-color: #58a6ff; }

  /* ── Legend toggle ── */
  #legend-toggle {
    position: fixed; top: 16px; left: 16px;
    background: rgba(13,17,23,0.85);
    border: 1px solid #30363d;
    border-radius: 8px;
    padding: 8px 12px;
    font-size: 11px;
    z-index: 10;
    display: flex; flex-direction: column; gap: 5px;
  }
  .legend-row { display: flex; align-items: center; gap: 8px; }
  .legend-line { width: 24px; height: 2px; border-radius: 1px; }
  .legend-dash {
    width: 24px; height: 0px;
    border-bottom: 2px dashed #e3b341;
  }
  .legend-text { color: #8b949e; }

  /* ── Tooltip ── */
  #tooltip {
    position: fixed;
    background: rgba(13,17,23,0.95);
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 8px 12px;
    font-size: 11px;
    pointer-events: none;
    z-index: 20;
    display: none;
    max-width: 260px;
  }
  #tooltip .tt-path { color: #58a6ff; font-weight: 600; margin-bottom: 4px; overflow-wrap: break-word; }
  #tooltip .tt-row { display: flex; justify-content: space-between; gap: 16px; }
  #tooltip .tt-label { color: #6e7681; }
  #tooltip .tt-val { color: #e6edf3; }

  /* ── SVG styles ── */
  .node { cursor: pointer; }
  .node circle { stroke-width: 1.5; transition: r 0.2s; }
  .node text { pointer-events: none; font-size: 9px; fill: #8b949e; }
  .link-struct { stroke: #21262d; stroke-width: 1; fill: none; }
  .link-cochange { fill: none; stroke-linecap: round; }
  .dimmed { opacity: 0.08 !important; }
  .highlighted circle { stroke: #fff !important; stroke-width: 2.5 !important; }
  .blast-1 circle { stroke: #ff6b6b !important; stroke-width: 2 !important; }
  .blast-2 circle { stroke: #ff9f43 !important; stroke-width: 1.5 !important; }
  .blast-3 circle { stroke: #ffd93d !important; stroke-width: 1 !important; }
</style>
</head>
<body>
<svg id="canvas"></svg>

<div id="panel">
  <div id="panel-header">
    <h1>🗺 Cartograph</h1>
    <div class="subtitle" id="repo-subtitle"></div>
  </div>
  <div class="section" id="stats-section">
    <h2>Stats</h2>
    <div class="stat-row"><span class="stat-label">Files</span><span class="stat-value" id="s-files">—</span></div>
    <div class="stat-row"><span class="stat-label">Crates</span><span class="stat-value" id="s-crates">—</span></div>
    <div class="stat-row"><span class="stat-label">Import edges</span><span class="stat-value" id="s-struct">—</span></div>
    <div class="stat-row"><span class="stat-label">Co-change edges</span><span class="stat-value" id="s-cochange">—</span></div>
    <div class="stat-row"><span class="stat-label">Contributors</span><span class="stat-value" id="s-owners">—</span></div>
  </div>
  <div class="section">
    <h2>Top Hotspots</h2>
    <div id="hotspot-list"></div>
  </div>
  <div class="section">
    <h2>Contributors</h2>
    <div id="owner-list"></div>
  </div>
</div>

<div id="selected-panel">
  <button id="close-btn" onclick="clearSelection()">×</button>
  <h2 id="sel-name"></h2>
  <div class="detail-row"><span class="detail-label">Path</span><span class="detail-value" id="sel-path"></span></div>
  <div class="detail-row"><span class="detail-label">Crate</span><span class="detail-value" id="sel-crate"></span></div>
  <div class="detail-row"><span class="detail-label">Owner</span><span class="detail-value" id="sel-owner"></span></div>
  <div class="detail-row"><span class="detail-label">Connections</span><span class="detail-value" id="sel-degree"></span></div>
  <div class="detail-row"><span class="detail-label">Blast radius</span><span class="detail-value" id="sel-blast"></span></div>
  <div class="cochange-list" id="sel-cochanges-wrap" style="display:none">
    <div style="color:#6e7681;font-size:10px;text-transform:uppercase;letter-spacing:0.8px;margin-bottom:4px">Co-changes</div>
    <div id="sel-cochanges"></div>
  </div>
</div>

<div id="controls">
  <button class="ctrl-btn active" id="btn-struct" onclick="toggleLayer('struct')">Imports</button>
  <button class="ctrl-btn active" id="btn-cochange" onclick="toggleLayer('cochange')">Co-changes</button>
  <button class="ctrl-btn" onclick="zoomFit()">Fit</button>
</div>

<div id="legend-toggle">
  <div class="legend-row"><div class="legend-line" style="background:#30363d"></div><span class="legend-text">Import edge</span></div>
  <div class="legend-row"><div class="legend-dash"></div><span class="legend-text">Co-change edge</span></div>
  <div class="legend-row" style="margin-top:2px">
    <div style="width:24px;height:8px;border-radius:4px;background:linear-gradient(90deg,#58a6ff,#3fb950,#d2a8ff,#ffa657)"></div>
    <span class="legend-text">Crate group</span>
  </div>
</div>

<div id="tooltip">
  <div class="tt-path" id="tt-path"></div>
  <div class="tt-row"><span class="tt-label">Crate</span><span class="tt-val" id="tt-crate"></span></div>
  <div class="tt-row"><span class="tt-label">Owner</span><span class="tt-val" id="tt-owner"></span></div>
  <div class="tt-row"><span class="tt-label">Connections</span><span class="tt-val" id="tt-degree"></span></div>
  <div class="tt-row"><span class="tt-label">Blast radius</span><span class="tt-val" id="tt-blast"></span></div>
</div>

<script>
const DATA = {graph_data};

// ── Color palette (by crate) ──
const CRATE_COLORS = [
  "#58a6ff","#3fb950","#d2a8ff","#ffa657",
  "#f78166","#79c0ff","#56d364","#e3b341",
  "#ff7b72","#a5d6ff","#7ee787","#ffa198",
];
const crateColor = (idx) => CRATE_COLORS[idx % CRATE_COLORS.length];

// ── Layers visibility ──
const visible = { struct: true, cochange: true };

// ── Setup SVG ──
const svg = d3.select("#canvas");
const W = window.innerWidth, H = window.innerHeight;

const defs = svg.append("defs");

// Glow filter
const glow = defs.append("filter").attr("id","glow").attr("x","-50%").attr("y","-50%").attr("width","200%").attr("height","200%");
glow.append("feGaussianBlur").attr("stdDeviation","3").attr("result","coloredBlur");
const feMerge = glow.append("feMerge");
feMerge.append("feMergeNode").attr("in","coloredBlur");
feMerge.append("feMergeNode").attr("in","SourceGraphic");

// Arrowhead
defs.append("marker").attr("id","arrow").attr("viewBox","0 -4 8 8").attr("refX",14).attr("refY",0).attr("markerWidth",6).attr("markerHeight",6).attr("orient","auto")
  .append("path").attr("d","M0,-4L8,0L0,4").attr("fill","#30363d");

const g = svg.append("g");

// Zoom
const zoom = d3.zoom().scaleExtent([0.1, 8]).on("zoom", (e) => g.attr("transform", e.transform));
svg.call(zoom).on("dblclick.zoom", null);

// Click background to deselect
svg.on("click", (e) => { if (e.target === svg.node()) clearSelection(); });

// ── Radius scale ──
const maxDeg = d3.max(DATA.nodes, d => d.degree) || 1;
const radius = d => Math.max(6, Math.min(28, 5 + Math.sqrt(d.degree) * 4.5));

// ── Build links arrays ──
const nodes = DATA.nodes.map(d => ({...d}));
const nodeById = Object.fromEntries(nodes.map(d => [d.id, d]));

const structLinks = DATA.struct_links.map(d => ({...d}));
const cochangeLinks = DATA.cochange_links.map(d => ({...d}));
const allLinks = [...structLinks, ...cochangeLinks];

// ── Force simulation ──
const sim = d3.forceSimulation(nodes)
  .force("link", d3.forceLink(allLinks).id(d => d.id)
    .distance(d => d.kind === "cochange" ? 120 + (1 - d.confidence) * 80 : 70)
    .strength(d => d.kind === "cochange" ? d.confidence * 0.3 : 0.7))
  .force("charge", d3.forceManyBody().strength(-220).distanceMax(300))
  .force("center", d3.forceCenter(W / 2, H / 2))
  .force("collide", d3.forceCollide(d => radius(d) + 4))
  .alphaDecay(0.025);

// ── Draw edges ──
const structLinkEl = g.append("g").attr("class","links-struct").selectAll("line")
  .data(structLinks).join("line")
  .attr("class","link-struct")
  .attr("marker-end","url(#arrow)");

const cochangeLinkEl = g.append("g").attr("class","links-cochange").selectAll("path")
  .data(cochangeLinks).join("path")
  .attr("class","link-cochange")
  .attr("stroke","#e3b341")
  .attr("stroke-width", d => 1 + d.confidence * 3)
  .attr("stroke-opacity", d => 0.3 + d.confidence * 0.5)
  .attr("stroke-dasharray","4 3");

// ── Draw nodes ──
const nodeEl = g.append("g").attr("class","nodes").selectAll("g")
  .data(nodes).join("g")
  .attr("class","node")
  .call(d3.drag()
    .on("start", (e, d) => { if (!e.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y; })
    .on("drag",  (e, d) => { d.fx = e.x; d.fy = e.y; })
    .on("end",   (e, d) => { if (!e.active) sim.alphaTarget(0); d.fx = null; d.fy = null; }))
  .on("click", (e, d) => { e.stopPropagation(); selectNode(d); })
  .on("mouseover", (e, d) => showTooltip(e, d))
  .on("mousemove", (e) => moveTooltip(e))
  .on("mouseout", hideTooltip);

nodeEl.append("circle")
  .attr("r", d => radius(d))
  .attr("fill", d => crateColor(d.crate_idx))
  .attr("fill-opacity", 0.85)
  .attr("stroke", d => d3.color(crateColor(d.crate_idx)).darker(0.5))
  .style("filter", "url(#glow)");

nodeEl.append("text")
  .attr("dy", d => radius(d) + 10)
  .attr("text-anchor","middle")
  .text(d => d.label);

// ── Tick ──
sim.on("tick", () => {
  structLinkEl
    .attr("x1", d => d.source.x).attr("y1", d => d.source.y)
    .attr("x2", d => d.target.x).attr("y2", d => d.target.y);

  cochangeLinkEl.attr("d", d => {
    const dx = d.target.x - d.source.x, dy = d.target.y - d.source.y;
    const dr = Math.sqrt(dx*dx + dy*dy) * 1.4;
    return `M${d.source.x},${d.source.y}A${dr},${dr} 0 0,1 ${d.target.x},${d.target.y}`;
  });

  nodeEl.attr("transform", d => `translate(${d.x},${d.y})`);
});

// ── Populate panel ──
document.getElementById("repo-subtitle").textContent = DATA.repo;
document.getElementById("s-files").textContent    = DATA.stats.files;
document.getElementById("s-crates").textContent   = DATA.stats.crates;
document.getElementById("s-struct").textContent   = DATA.stats.struct_edges;
document.getElementById("s-cochange").textContent = DATA.stats.cochange_edges + " (≥0.25 conf)";
document.getElementById("s-owners").textContent   = DATA.stats.owners;

// Hotspots
const hl = document.getElementById("hotspot-list");
DATA.hotspots.forEach(h => {
  const div = document.createElement("div");
  div.className = "hotspot-item";
  div.innerHTML = `
    <div class="hotspot-dot" style="background:${crateColor(nodeById[h.id]?.crate_idx ?? 0)}"></div>
    <span class="hotspot-name" title="${h.path}">${h.label}</span>
    <span class="hotspot-deg">${h.degree}</span>`;
  div.onclick = () => selectNode(nodeById[h.id]);
  hl.appendChild(div);
});

// Owners
const ol = document.getElementById("owner-list");
DATA.owners.slice(0, 8).forEach((o, i) => {
  const div = document.createElement("div");
  div.className = "owner-item";
  // pick a distinct color for owner vs crate - use a complementary palette
  const ocol = ["#58a6ff","#3fb950","#d2a8ff","#ffa657","#f78166","#79c0ff","#56d364","#e3b341"][i % 8];
  div.innerHTML = `
    <div class="owner-dot" style="background:${ocol}"></div>
    <span class="owner-name">${o.name}</span>
    <span class="owner-count">${o.count} files</span>`;
  ol.appendChild(div);
});

// ── Zoom fit ──
function zoomFit() {
  const bounds = g.node().getBBox();
  const scale = 0.85 / Math.max(bounds.width / W, bounds.height / H);
  const tx = (W - scale * (bounds.x * 2 + bounds.width)) / 2;
  const ty = (H - scale * (bounds.y * 2 + bounds.height)) / 2;
  svg.transition().duration(600).call(zoom.transform, d3.zoomIdentity.translate(tx, ty).scale(scale));
}

// Fit after simulation settles
sim.on("end", zoomFit);

// ── Toggle layers ──
function toggleLayer(kind) {
  visible[kind] = !visible[kind];
  document.getElementById(`btn-${kind}`).classList.toggle("active", visible[kind]);
  if (kind === "struct") {
    structLinkEl.style("display", visible.struct ? null : "none");
  } else {
    cochangeLinkEl.style("display", visible.cochange ? null : "none");
  }
}

// ── Tooltip ──
const tooltip = document.getElementById("tooltip");
function showTooltip(e, d) {
  tooltip.style.display = "block";
  document.getElementById("tt-path").textContent = d.path;
  document.getElementById("tt-crate").textContent = d.crate;
  document.getElementById("tt-owner").textContent = d.owner;
  document.getElementById("tt-degree").textContent = d.degree;
  document.getElementById("tt-blast").textContent = (DATA.blast[d.id] || []).length + " files";
  moveTooltip(e);
}
function moveTooltip(e) {
  const x = e.clientX + 14, y = e.clientY - 10;
  tooltip.style.left = (x + 260 > W ? x - 280 : x) + "px";
  tooltip.style.top  = (y + 80  > H ? y - 90  : y) + "px";
}
function hideTooltip() { tooltip.style.display = "none"; }

// ── Selection + blast radius ──
let selected = null;

function selectNode(d) {
  if (!d) return;
  selected = d;
  const blast = DATA.blast[d.id] || [];
  const blastSet = new Set(blast);
  const blastDepth = {};
  // recompute depth for styling
  const adj = {};
  DATA.struct_links.forEach(l => {
    const s = typeof l.source === "object" ? l.source.id : l.source;
    const t = typeof l.target === "object" ? l.target.id : l.target;
    if (!adj[s]) adj[s] = [];
    if (!adj[t]) adj[t] = [];
    adj[s].push(t); adj[t].push(s);
  });
  let frontier = [d.id], visited = new Set([d.id]);
  for (let depth = 1; depth <= 3; depth++) {
    const next = [];
    frontier.forEach(n => (adj[n]||[]).forEach(nb => {
      if (!visited.has(nb)) { visited.add(nb); blastDepth[nb] = depth; next.push(nb); }
    }));
    frontier = next;
  }

  // Style nodes
  nodeEl
    .classed("dimmed", nd => nd.id !== d.id && !blastSet.has(nd.id))
    .classed("highlighted", nd => nd.id === d.id)
    .classed("blast-1", nd => blastDepth[nd.id] === 1)
    .classed("blast-2", nd => blastDepth[nd.id] === 2)
    .classed("blast-3", nd => blastDepth[nd.id] === 3);

  // Style edges
  structLinkEl.style("opacity", l => {
    const s = typeof l.source === "object" ? l.source.id : l.source;
    const t = typeof l.target === "object" ? l.target.id : l.target;
    return (s === d.id || t === d.id || blastSet.has(s) || blastSet.has(t)) ? 0.9 : 0.03;
  });
  cochangeLinkEl.style("opacity", l => {
    const s = typeof l.source === "object" ? l.source.id : l.source;
    const t = typeof l.target === "object" ? l.target.id : l.target;
    return (s === d.id || t === d.id) ? 0.9 : 0.03;
  });

  // Co-changes for this node
  const myCochanges = DATA.cochange_links
    .filter(l => {
      const s = typeof l.source === "object" ? l.source.id : l.source;
      const t = typeof l.target === "object" ? l.target.id : l.target;
      return s === d.id || t === d.id;
    })
    .map(l => {
      const otherId = (typeof l.source === "object" ? l.source.id : l.source) === d.id
        ? (typeof l.target === "object" ? l.target.id : l.target)
        : (typeof l.source === "object" ? l.source.id : l.source);
      return { label: nodeById[otherId]?.label || otherId, confidence: l.confidence };
    })
    .sort((a, b) => b.confidence - a.confidence);

  // Fill selected panel
  document.getElementById("sel-name").textContent = d.label;
  document.getElementById("sel-path").textContent = d.path;
  document.getElementById("sel-crate").textContent = d.crate;
  document.getElementById("sel-owner").textContent = d.owner || "—";
  document.getElementById("sel-degree").textContent = d.degree;
  document.getElementById("sel-blast").textContent = blast.length + " files affected";

  const ccWrap = document.getElementById("sel-cochanges-wrap");
  const ccList = document.getElementById("sel-cochanges");
  if (myCochanges.length) {
    ccWrap.style.display = "block";
    ccList.innerHTML = myCochanges.slice(0, 6).map(c =>
      `<div class="cochange-item"><span>${c.label}</span><span>${Math.round(c.confidence*100)}%</span></div>`
    ).join("");
  } else {
    ccWrap.style.display = "none";
  }

  document.getElementById("selected-panel").style.display = "block";
  hideTooltip();

  // Zoom toward selected node
  const t = d3.zoomTransform(svg.node());
  const nx = d.x || W/2, ny = d.y || H/2;
  const scale = Math.max(t.k, 1.2);
  svg.transition().duration(500).call(
    zoom.transform,
    d3.zoomIdentity.translate(W/2 - nx * scale, H/2 - ny * scale).scale(scale)
  );
}

function clearSelection() {
  selected = null;
  nodeEl.classed("dimmed highlighted blast-1 blast-2 blast-3", false);
  structLinkEl.style("opacity", null);
  cochangeLinkEl.style("opacity", null);
  document.getElementById("selected-panel").style.display = "none";
}

// ── Crate color legend (top-left) ──
const legendDiv = document.getElementById("legend-toggle");
const crateDiv = document.createElement("div");
crateDiv.style.cssText = "margin-top:6px;border-top:1px solid #21262d;padding-top:6px;display:flex;flex-direction:column;gap:4px";
crateDiv.innerHTML = "<div style='font-size:10px;text-transform:uppercase;letter-spacing:0.8px;color:#6e7681;margin-bottom:2px'>Crates</div>";
DATA.crates.forEach((crate, i) => {
  const row = document.createElement("div");
  row.className = "legend-row";
  row.innerHTML = `<div style="width:10px;height:10px;border-radius:50%;background:${crateColor(i)};flex-shrink:0"></div><span class="legend-text">${crate}</span>`;
  crateDiv.appendChild(row);
});
legendDiv.appendChild(crateDiv);
</script>
</body>
</html>
"""


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description="Emberloom Cartograph — Interactive Visualizer")
    parser.add_argument("--db", required=True, help="Path to .cartograph/db.sqlite")
    parser.add_argument("--repo-name", required=True, help="Repository display name")
    parser.add_argument("--out", required=True, help="Output HTML file path")
    parser.add_argument("--cochange-threshold", type=float, default=0.25,
                        help="Minimum co-change confidence to show (default: 0.25)")
    args = parser.parse_args()

    print(f"Loading data from {args.db}...")
    files, struct_edges, cochange_edges, owner_of, degree = load_data(args.db, args.cochange_threshold)
    print(f"  {len(files)} files, {len(struct_edges)} import edges, {len(cochange_edges)} co-change edges (>={args.cochange_threshold})")

    print("Building graph JSON...")
    graph_data = build_graph_json(files, struct_edges, cochange_edges, owner_of, degree, args.repo_name)

    print(f"Writing {args.out}...")
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)

    html = HTML_TEMPLATE.replace("{repo_name}", args.repo_name).replace("{graph_data}", graph_data)
    out_path.write_text(html, encoding="utf-8")

    size_kb = out_path.stat().st_size // 1024
    print(f"Done. {size_kb}KB — open {args.out} in your browser.")


if __name__ == "__main__":
    main()
