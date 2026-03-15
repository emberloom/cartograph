#!/usr/bin/env python3
"""
Cartograph — Data Exporter

Reads a .cartograph/db.sqlite database and outputs data.json
for the Cartograph web visualization.

Usage:
    python3 scripts/viz.py \
        --db /path/to/.cartograph/db.sqlite \
        --repo-name "myrepo" \
        --out viz/public/data.json
"""

import sqlite3
import json
import argparse
from pathlib import Path
from collections import defaultdict


def load_entities_and_edges(db_path: str, cochange_threshold: float):
    """Load file entities and edges from SQLite."""
    conn = sqlite3.connect(db_path)
    conn.row_factory = sqlite3.Row

    files = conn.execute(
        "SELECT id, name, path FROM entities WHERE kind = 'File' ORDER BY path"
    ).fetchall()

    all_edges = conn.execute(
        "SELECT from_id, to_id, kind, confidence FROM edges"
    ).fetchall()

    file_ids = {r["id"] for r in files}

    # Structural edges (imports only)
    struct_edges = [
        e for e in all_edges
        if e["kind"] == "imports"
        and e["from_id"] in file_ids
        and e["to_id"] in file_ids
    ]

    # Co-change edges
    cochange_edges = [
        e for e in all_edges
        if e["kind"] == "co_changes_with"
        and e["from_id"] in file_ids
        and e["to_id"] in file_ids
        and e["confidence"] >= cochange_threshold
    ]

    # Ownership: persons + owned_by edges
    persons = conn.execute(
        "SELECT id, name, path as email FROM entities WHERE kind = 'Person'"
    ).fetchall()
    person_name = {r["id"]: r["name"] for r in persons}
    person_email = {r["id"]: r["email"] for r in persons}

    conn.close()

    owner_of = {}
    owner_conf = {}
    for e in all_edges:
        if e["kind"] == "owned_by" and e["from_id"] in file_ids:
            pid = e["to_id"]
            conf = e["confidence"]
            if e["from_id"] not in owner_conf or conf > owner_conf[e["from_id"]]:
                owner_conf[e["from_id"]] = conf
                owner_of[e["from_id"]] = (
                    person_name.get(pid, "unknown"),
                    person_email.get(pid, ""),
                )

    return files, struct_edges, cochange_edges, owner_of


def build_tree(files, file_idx, struct_degree, cochange_count,
               risk_scores, owner_of):
    """Build nested directory tree from flat file list."""
    root = {"name": "root", "children": []}

    for r in files:
        path = r["path"]
        if not path:
            continue
        parts = path.split("/")
        node = root
        for part in parts[:-1]:
            # Find or create directory node
            child = next((c for c in node["children"] if c["name"] == part and "children" in c), None)
            if child is None:
                child = {"name": part, "children": []}
                node["children"].append(child)
            node = child

        # Leaf (file) node
        fid = r["id"]
        idx = file_idx[fid]
        owner_name, owner_email = owner_of.get(fid, ("unowned", ""))
        node["children"].append({
            "name": parts[-1],
            "path": path,
            "id": idx,
            "degree": struct_degree.get(fid, 0),
            "cochangeCount": cochange_count.get(fid, 0),
            "riskScore": round(risk_scores.get(fid, 0.0), 4),
            "owner": owner_name,
            "ownerEmail": owner_email,
        })

    return root


def blast_radius_bfs(file_id, adj, depth=3):
    """BFS from file_id on structural adjacency, return set of reachable IDs."""
    visited = {file_id}
    frontier = [file_id]
    for _ in range(depth):
        next_f = []
        for n in frontier:
            for nb in adj.get(n, []):
                if nb not in visited:
                    visited.add(nb)
                    next_f.append(nb)
        frontier = next_f
    visited.discard(file_id)
    return visited


def build_data(files, struct_edges, cochange_edges, owner_of,
               repo_name, max_nodes):
    """Build the full data.json structure."""
    all_file_ids = {r["id"] for r in files}

    # Structural degree (imports only)
    struct_degree = defaultdict(int)
    for e in struct_edges:
        struct_degree[e["from_id"]] += 1
        struct_degree[e["to_id"]] += 1

    # If max_nodes, keep top N by structural degree
    if max_nodes > 0 and len(files) > max_nodes:
        sorted_files = sorted(
            files,
            key=lambda r: struct_degree.get(r["id"], 0),
            reverse=True,
        )
        files = sorted_files[:max_nodes]
        kept = {r["id"] for r in files}
        struct_edges = [e for e in struct_edges
                        if e["from_id"] in kept and e["to_id"] in kept]
        cochange_edges = [e for e in cochange_edges
                          if e["from_id"] in kept and e["to_id"] in kept]
        # Recompute degree after filtering
        struct_degree = defaultdict(int)
        for e in struct_edges:
            struct_degree[e["from_id"]] += 1
            struct_degree[e["to_id"]] += 1

    total_files = len(all_file_ids)

    # Assign contiguous IDs sorted by path
    files_sorted = sorted(files, key=lambda r: r["path"] or "")
    file_idx = {r["id"]: i for i, r in enumerate(files_sorted)}

    # Structural adjacency (using original DB IDs for BFS)
    struct_adj = defaultdict(list)
    for e in struct_edges:
        struct_adj[e["from_id"]].append(e["to_id"])
        struct_adj[e["to_id"]].append(e["from_id"])

    # Co-change count per file (unique partners, deduplicated)
    cochange_partners = defaultdict(set)
    for e in cochange_edges:
        if e["from_id"] in file_idx and e["to_id"] in file_idx:
            cochange_partners[e["from_id"]].add(e["to_id"])
            cochange_partners[e["to_id"]].add(e["from_id"])
    cochange_count = {fid: len(partners) for fid, partners in cochange_partners.items()}

    # Owner count per file (for risk multiplier)
    owner_count_per_file = defaultdict(int)
    for r in files_sorted:
        fid = r["id"]
        if fid in owner_of:
            owner_count_per_file[fid] = 1  # primary owner exists
        # Future: count distinct owners from all owned_by edges

    # Risk scores: cochange_count * blast_size * (1/owner_count if available)
    risk_scores = {}
    for r in files_sorted:
        fid = r["id"]
        blast = blast_radius_bfs(fid, struct_adj, depth=3)
        cc = cochange_count.get(fid, 0)
        bs = len(blast)
        raw = cc * bs
        # Apply ownership multiplier if ownership data exists
        oc = owner_count_per_file.get(fid, 0)
        if oc > 0:
            raw = raw * (1.0 / oc)
        risk_scores[fid] = raw

    max_risk = max(risk_scores.values()) if risk_scores else 1
    if max_risk == 0:
        max_risk = 1
    for fid in risk_scores:
        risk_scores[fid] = risk_scores[fid] / max_risk

    # Build tree
    tree = build_tree(
        files_sorted, file_idx, struct_degree, cochange_count,
        risk_scores, owner_of,
    )

    # Struct edges as [idx, idx] pairs
    struct_edge_pairs = []
    for e in struct_edges:
        if e["from_id"] in file_idx and e["to_id"] in file_idx:
            struct_edge_pairs.append([
                file_idx[e["from_id"]],
                file_idx[e["to_id"]],
            ])

    # Co-change by node (top 20 per file, keyed by string idx)
    cochange_by_node = defaultdict(list)
    seen = set()
    for e in cochange_edges:
        if e["from_id"] in file_idx and e["to_id"] in file_idx:
            a = file_idx[e["from_id"]]
            b = file_idx[e["to_id"]]
            key = (min(a, b), max(a, b))
            if key not in seen:
                seen.add(key)
                conf = round(e["confidence"], 3)
                cochange_by_node[a].append({"t": b, "c": conf})
                cochange_by_node[b].append({"t": a, "c": conf})

    # Sort and trim to top 20
    for nid in cochange_by_node:
        cochange_by_node[nid] = sorted(
            cochange_by_node[nid], key=lambda x: -x["c"]
        )[:20]

    # Convert keys to strings (JSON requirement)
    cochange_by_node_str = {
        str(k): v for k, v in cochange_by_node.items()
    }

    # Count directories
    dir_set = set()
    for r in files_sorted:
        path = r["path"]
        if path:
            parts = path.split("/")
            for i in range(1, len(parts)):
                dir_set.add("/".join(parts[:i]))

    # Hotspots
    hotspots_sorted = sorted(
        files_sorted,
        key=lambda r: struct_degree.get(r["id"], 0),
        reverse=True,
    )[:10]
    hotspots = [
        {
            "id": file_idx[r["id"]],
            "label": Path(r["path"]).name if r["path"] else r["name"],
            "path": r["path"],
            "degree": struct_degree.get(r["id"], 0),
        }
        for r in hotspots_sorted
    ]

    total_cochange = len(seen)

    data = {
        "repo": repo_name,
        "stats": {
            "files": len(files_sorted),
            "total_files": total_files,
            "struct_edges": len(struct_edge_pairs),
            "cochange_edges": total_cochange,
            "owners": len(set(
                owner_of.get(r["id"], ("unowned",))[0]
                for r in files_sorted
            ) - {"unowned"}),
            "directories": len(dir_set),
        },
        "tree": tree,
        "struct_edges": struct_edge_pairs,
        "cochange_by_node": cochange_by_node_str,
        "hotspots": hotspots,
    }

    return data


def main():
    parser = argparse.ArgumentParser(
        description="Cartograph — Data Exporter for Web Visualization"
    )
    parser.add_argument("--db", required=True, help="Path to .cartograph/db.sqlite")
    parser.add_argument("--repo-name", required=True, help="Repository display name")
    parser.add_argument("--out", required=True, help="Output data.json path")
    parser.add_argument(
        "--cochange-threshold", type=float, default=0.25,
        help="Minimum co-change confidence (default: 0.25)",
    )
    parser.add_argument(
        "--max-nodes", type=int, default=0,
        help="Max files to include (top N by degree, 0=all, default: 0)",
    )
    args = parser.parse_args()

    print(f"Loading data from {args.db}...")
    files, struct_edges, cochange_edges, owner_of = load_entities_and_edges(
        args.db, args.cochange_threshold
    )
    print(f"  {len(files)} files, {len(struct_edges)} import edges, "
          f"{len(cochange_edges)} co-change edges (>={args.cochange_threshold})")

    if args.max_nodes > 0 and len(files) > args.max_nodes:
        print(f"  Limiting to top {args.max_nodes} nodes by degree")

    print("Building data...")
    data = build_data(
        files, struct_edges, cochange_edges, owner_of,
        args.repo_name, args.max_nodes,
    )

    print(f"Writing {args.out}...")
    out_path = Path(args.out)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(
        json.dumps(data, separators=(",", ":")),
        encoding="utf-8",
    )

    size_kb = out_path.stat().st_size // 1024
    print(f"Done. {size_kb}KB — {data['stats']['files']} files, "
          f"{data['stats']['struct_edges']} struct edges, "
          f"{data['stats']['cochange_edges']} co-change entries")


if __name__ == "__main__":
    main()
