import * as THREE from 'three';
import { scene, markDirty } from './renderer.js';
import { filePositions, fileNodes, regions } from './layout.js';

let edgeMesh = null;

/**
 * Compute world-space points along an arc between two positions.
 * Cross-module arcs: cubic Bezier through centroidA → centroidB.
 * Same-module arcs: pass centroidA = centroidB = midpoint(p0, p1) for a straight line.
 *
 * @param {{x:number,y:number}} p0 - source position (Three.js world space, Y already flipped)
 * @param {{x:number,y:number}} p1 - target position
 * @param {{x:number,y:number}} centroidA - source region centroid
 * @param {{x:number,y:number}} centroidB - target region centroid
 * @param {number} segments - number of segments (returns segments+1 points)
 * @returns {THREE.Vector3[]}
 */
export function computeArcPoints(p0, p1, centroidA, centroidB, segments) {
  const curve = new THREE.CubicBezierCurve3(
    new THREE.Vector3(p0.x, p0.y, 0),
    new THREE.Vector3(centroidA.x, centroidA.y, 0),
    new THREE.Vector3(centroidB.x, centroidB.y, 0),
    new THREE.Vector3(p1.x, p1.y, 0),
  );
  return curve.getPoints(segments);
}

/**
 * Build and add structural edges to the scene.
 * Cross-module edges use cubic Bezier bundling through region centroids.
 * Same-module edges are straight lines.
 *
 * @param {Array<[number,number]>} structEdges - [sourceId, targetId] pairs
 */
export function createEdges(structEdges) {
  // Pre-compute top-level region centroids
  const regionCentroids = {};
  for (const r of regions) {
    if (r.depth === 1) {
      regionCentroids[r.name] = {
        x: (r.x0 + r.x1) / 2,
        y: -(r.y0 + r.y1) / 2, // flip Y
      };
    }
  }

  // Build file id → topLevelDir lookup
  const fileTopDir = {};
  for (const fn of fileNodes) {
    fileTopDir[fn.id] = fn.topLevelDir;
  }

  const points = [];

  for (const [srcId, tgtId] of structEdges) {
    const srcPos = filePositions.get(srcId);
    const tgtPos = filePositions.get(tgtId);
    if (!srcPos || !tgtPos) continue;

    const sx = srcPos.x, sy = -srcPos.y; // flip Y
    const tx = tgtPos.x, ty = -tgtPos.y;

    const srcDir = fileTopDir[srcId];
    const tgtDir = fileTopDir[tgtId];

    if (srcDir && tgtDir && srcDir !== tgtDir) {
      // Cross-module: cubic Bezier through region centroids
      const c1 = regionCentroids[srcDir];
      const c2 = regionCentroids[tgtDir];
      if (c1 && c2) {
        const curve = new THREE.CubicBezierCurve3(
          new THREE.Vector3(sx, sy, -0.5),
          new THREE.Vector3(c1.x, c1.y, -0.5),
          new THREE.Vector3(c2.x, c2.y, -0.5),
          new THREE.Vector3(tx, ty, -0.5),
        );
        const samples = curve.getPoints(8);
        for (let i = 0; i < samples.length - 1; i++) {
          points.push(samples[i].x, samples[i].y, samples[i].z);
          points.push(samples[i + 1].x, samples[i + 1].y, samples[i + 1].z);
        }
      } else {
        // Fallback: straight line
        points.push(sx, sy, -0.5, tx, ty, -0.5);
      }
    } else {
      // Same-module: straight line
      points.push(sx, sy, -0.5, tx, ty, -0.5);
    }
  }

  if (points.length === 0) return;

  const geo = new THREE.BufferGeometry();
  geo.setAttribute(
    'position',
    new THREE.Float32BufferAttribute(points, 3),
  );
  const mat = new THREE.LineBasicMaterial({
    color: 0x21262d,
    transparent: true,
    opacity: 0.6,
    depthWrite: false,
  });
  edgeMesh = new THREE.LineSegments(geo, mat);
  scene.add(edgeMesh);
  markDirty();
}

/**
 * Set edge opacity for selection highlighting.
 * @param {number|null} opacity - null resets to default
 */
export function setEdgeOpacity(opacity) {
  if (!edgeMesh) return;
  edgeMesh.material.opacity = opacity ?? 0.6;
  edgeMesh.material.needsUpdate = true;
  markDirty();
}

export function getEdgeMesh() {
  return edgeMesh;
}
