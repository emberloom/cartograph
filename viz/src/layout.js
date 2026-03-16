import { treemap, hierarchy } from 'd3-hierarchy';
import { dirColor } from './colors.js';

// Module-level state — computed once, read by nodes.js, edges.js, interaction.js
export let regions = [];
export let fileNodes = [];
export let filePositions = new Map();

// Layout dimensions (world units)
const LAYOUT_W = 1000;
const LAYOUT_H = 1000;

/**
 * Compute treemap layout from data.tree.
 * Populates regions[], fileNodes[], filePositions.
 */
export function computeLayout(tree) {
  const root = hierarchy(tree)
    .sum(d => d.degree || 1)
    .sort((a, b) => b.value - a.value);

  treemap()
    .size([LAYOUT_W, LAYOUT_H])
    .paddingInner(2)
    .paddingOuter(4)
    .paddingTop(18)
    .round(true)(root);

  // Assign top-level dir index to each node
  const topLevelDirs = root.children
    ? root.children.map(c => c.data.name)
    : [];
  const topDirIdx = Object.fromEntries(topLevelDirs.map((n, i) => [n, i]));

  regions = [];
  fileNodes = [];
  filePositions = new Map();

  root.each(node => {
    // Determine top-level directory
    let topDir = '';
    let topIdx = 0;
    const ancestors = node.ancestors().reverse();
    if (ancestors.length >= 2) {
      topDir = ancestors[1].data.name;
      topIdx = topDirIdx[topDir] ?? 0;
    }

    if (node.children) {
      // Directory node
      regions.push({
        name: node.data.name,
        x0: node.x0,
        y0: node.y0,
        x1: node.x1,
        y1: node.y1,
        depth: node.depth,
        color: dirColor(topIdx),
        topLevelDir: topDir,
      });
    } else if (node.data.id !== undefined) {
      // File (leaf) node
      const x = (node.x0 + node.x1) / 2;
      const y = (node.y0 + node.y1) / 2;
      const fn = {
        id: node.data.id,
        x,
        y,
        width: node.x1 - node.x0,
        height: node.y1 - node.y0,
        path: node.data.path,
        name: node.data.name,
        degree: node.data.degree || 0,
        cochangeCount: node.data.cochangeCount || 0,
        riskScore: node.data.riskScore || 0,
        owner: node.data.owner || 'unowned',
        topLevelDir: topDir,
        topLevelDirIdx: topIdx,
      };
      fileNodes.push(fn);
      filePositions.set(fn.id, { x, y });
    }
  });

  // Sort fileNodes by id for consistent indexing
  fileNodes.sort((a, b) => a.id - b.id);

  return { regions, fileNodes, filePositions, bounds: { w: LAYOUT_W, h: LAYOUT_H } };
}
