// Measuring as clicks find it: a box's faces, edges and corners and what lies between them, even
// once it's moved; in millimeters whatever a model's units and layout; and curves and seams, which
// are neither faces nor edges.

import assert from 'node:assert/strict';
import { test } from 'node:test';
import * as THREE from 'three';
import { between, featureAt } from './measure.ts';

const box = new THREE.Mesh(new THREE.BoxGeometry(200, 30, 120));
box.updateMatrixWorld();

/** What a click finds looking from `eye` straight at `target`, within 1 mm of the pointer. */
function click(eye: number[], target: number[], mesh: THREE.Mesh = box, spot = false) {
  const [from, to] = [new THREE.Vector3(...eye), new THREE.Vector3(...target)];
  const ray = new THREE.Raycaster(from, to.sub(from).normalize());
  return featureAt(ray.intersectObject(mesh)[0], ray.ray, 1, spot);
}
/** Equal to within float32's precision, as the GPU's meshes have it, or to `within`. */
const close = (actual: number | null, expected: number, within = 1e-4) => assert.ok(actual !== null && Math.abs(actual - expected) < within, `${actual}, not ${expected}`);

test('a box measures as it is', () => {
  const front = click([10, 0, 500], [10, 0, 0]);
  const back = click([-30, 5, -500], [-30, 5, 0]);
  const top = click([0, 500, 0], [0, 0, 0]);
  assert.equal(front.kind, 'face');
  // Parallel faces are their distance apart; others only an angle.
  close(between(front, back).mm, 120);
  assert.equal(between(front, top).mm, null);
  close(between(front, top).degrees, 90);

  // Near its top front edge, the edge; near a corner, the corner.
  const edge = click([20, 14.5, 500], [20, 14.5, 0]);
  const corner = click([99.5, 14.5, 500], [99.5, 14.5, 0]);
  assert.equal(edge.kind, 'edge');
  // So does a ray right through it.
  assert.equal(click([20, 500, 60], [20, 15, 60]).kind, 'edge');
  assert.deepEqual(corner.kind === 'point' && corner.at.toArray(), [100, 15, 60]);
  close(between(edge, back).mm, 120);
  close(between(corner, click([0, 0, -500], [0, 0, 0])).mm, 120);
  // Near a corner on screen is near it, however steep the face it's on: here 1.34 mm from it on
  // the top face, seen from low down, but 0.64 mm from the pointer's ray.
  const low = click([99.4, 15 + 98.06, 58.8 + 490.29], [99.4, 15, 58.8]);
  assert.deepEqual(low.kind === 'point' && low.at.toArray(), [100, 15, 60]);
  // A point and a line have no angle between them.
  assert.equal(between(corner, edge).degrees, null);
  // Corner to corner, and edge to parallel edge, are the closest two points.
  close(between(corner, click([-99.5, 14.5, 500], [-99.5, 14.5, 0])).mm, 200);
  close(between(edge, click([20, 14.5, -500], [20, 14.5, 0])).mm, 120);

  // Asked for a spot, it's the spot.
  const spot = click([10, 0, 500], [10, 0, 0], box, true);
  assert.deepEqual(spot.kind === 'point' && spot.at.toArray(), [10, 0, 60]);
  close(between(spot, top).mm, 15);

  // What's picked moves with the box, as parts do when the view explodes.
  box.position.set(0, 0, 50);
  box.updateMatrixWorld();
  const fixed = new THREE.Mesh(new THREE.BoxGeometry(10, 10, 10));
  fixed.updateMatrixWorld();
  const near = click([0, 0, 500], [0, 0, 0], fixed);
  // Its back face is now at z = -10, the other box's front at 5.
  close(between(back, near).mm, 15);
});

test('models measure in millimeters whatever their units and layout', () => {
  // A model in meters, shown a thousand times bigger, as GLB devices are; with interleaved
  // positions, as some exporters write them.
  const meters = new THREE.BoxGeometry(0.2, 0.03, 0.12);
  const interleaved = new THREE.InterleavedBuffer(new Float32Array([...meters.getAttribute('position').array].flatMap((v, i) => (i % 3 === 2 ? [v, 0] : [v]))), 4);
  meters.setAttribute('position', new THREE.InterleavedBufferAttribute(interleaved, 3, 0));
  const device = new THREE.Mesh(meters);
  device.scale.setScalar(1000);
  device.updateMatrixWorld();
  const [front, back] = [click([10, 0, 500], [10, 0, 0], device), click([10, 0, -500], [10, 0, 0], device)];
  assert.equal(front.kind, 'face');
  close(between(front, back).mm, 120);
  close(between(click([99.5, 14.5, 500], [99.5, 14.5, 0], device), back).mm, 120);
});

test('a curve is no face, and a seam is no edge', () => {
  // Facets less than a degree apart are still a curve: only those facing the way of the one
  // pointed at count, so the face is a sliver of the side, its plane touching it.
  const cylinder = new THREE.Mesh(new THREE.CylinderGeometry(20, 20, 50, 512));
  cylinder.updateMatrixWorld();
  const [side, other] = [click([0, 0, 500], [0, 0, 0], cylinder), click([0, 0, -500], [0, 0, 0], cylinder)];
  assert.ok(side.kind === 'face' && side.triangles.length < 20, `${side.kind === 'face' && side.triangles.length} triangles`);
  // Facets 40 mm apart, less a sliver for being flat.
  close(between(side, other).mm, 40, 0.01);

  // A flat square, split along x = 50 with a sliver of no area between its halves.
  const positions = [0, 0, 0, 50, 0, 0, 50, 100, 0, 0, 0, 0, 50, 100, 0, 0, 100, 0, 50, 0, 0, 100, 0, 0, 100, 100, 0, 50, 0, 0, 100, 100, 0, 50, 100, 0, 50, 0, 0, 50, 50, 0, 50, 100, 0];
  const square = new THREE.Mesh(new THREE.BufferGeometry().setAttribute('position', new THREE.Float32BufferAttribute(positions, 3)));
  square.updateMatrixWorld();
  const [left, right] = [click([25, 50, 100], [25, 50, 0], square), click([50.5, 50, 100], [50.5, 50, 0], square)];
  // Either side is the whole square, its four triangles, and next to the seam is no edge.
  assert.deepEqual([left, right].map((f) => f.kind === 'face' && f.triangles.length), [4, 4]);
});
