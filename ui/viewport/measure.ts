// Measuring in the viewport, as a slicer does: the point, edge or flat face under the pointer
// lights up, two clicks pick two of them, and we give the distance between them and, where both
// have a direction, the angle. Anything shown can be measured, parts and devices alike.

import * as THREE from 'three';
import { LineMaterial } from 'three/examples/jsm/lines/LineMaterial.js';
import { LineSegments2 } from 'three/examples/jsm/lines/LineSegments2.js';
import { LineSegmentsGeometry } from 'three/examples/jsm/lines/LineSegmentsGeometry.js';
import { mergeVertices } from 'three/examples/jsm/utils/BufferGeometryUtils.js';
import type { Place } from '../studio.svelte.ts';

/** Something to measure from or to, in its mesh's own frame, so it moves with the mesh. */
export type Feature = { mesh: THREE.Mesh } & (
  | { kind: 'point'; at: THREE.Vector3 }
  | { kind: 'edge'; a: THREE.Vector3; b: THREE.Vector3 }
  | { kind: 'face'; triangles: number[]; normal: THREE.Vector3; at: THREE.Vector3 }
);

/** What's picked so far; between two picks, the distance and the angle where they have one; and
 * the device they're both on, if they are, whose model the length can calibrate. */
export type Measurement = { picked: number; mm: number | null; degrees: number | null; device: Place | null };

/** Triangles whose normals stay this close to the one pointed at, and touch, are one flat face. */
const FLAT = Math.cos(THREE.MathUtils.degToRad(1));
/** Edges and faces closer than this to parallel are parallel. */
const PARALLEL = THREE.MathUtils.degToRad(0.5);
/** Faces meeting at more than this have an edge between them, as the view draws them. */
export const SHARP = 30;
/** How close, in pixels, the pointer must come to a point or an edge to take it. */
export const REACH = 8;

/**
 * What we need of a mesh to find its faces and edges, worked out the first time the pointer comes
 * by: each triangle's corners, with corners that share a position shared, and its normal; the
 * triangles around each corner; and the sharp edges.
 */
type Topology = { corners: ArrayLike<number>; normals: THREE.Vector3[]; around: number[][]; edges: [THREE.Vector3, THREE.Vector3][] };
const topologies = new WeakMap<THREE.BufferGeometry, Topology>();

function topology(geometry: THREE.BufferGeometry): Topology {
  let known = topologies.get(geometry);
  if (known) return known;
  // Positions alone, in a plain array: models can come with them interleaved or quantized, which
  // mergeVertices can't take.
  const position = geometry.getAttribute('position');
  const plain = Float32Array.from({ length: position.count * 3 }, (_, i) => position.getComponent(Math.floor(i / 3), i % 3));
  const bare = new THREE.BufferGeometry().setAttribute('position', new THREE.BufferAttribute(plain, 3));
  if (geometry.index) bare.setIndex(geometry.index);
  const merged = mergeVertices(bare);
  const corners = merged.index!.array;
  const count = corners.length / 3;
  const shapes = Array.from({ length: count }, (_, t) => triangle(merged, t));
  const normals = shapes.map((t) => t.getNormal(new THREE.Vector3()));
  const around: number[][] = [];
  for (let k = 0; k < corners.length; k++) (around[corners[k]] ??= []).push(Math.floor(k / 3));
  // Slivers of no area, as exporters leave along seams, have no direction to be sharp to.
  merged.computeBoundingBox();
  const tiny = (merged.boundingBox!.getSize(new THREE.Vector3()).length() * 1e-6) ** 2;
  merged.setIndex(Array.from(corners).filter((_, k) => shapes[Math.floor(k / 3)].getArea() > tiny));
  const lines = new THREE.EdgesGeometry(merged, SHARP).getAttribute('position');
  const segments = Array.from({ length: lines.count / 2 }, (_, s) => [0, 1].map((k) => new THREE.Vector3().fromBufferAttribute(lines, 2 * s + k)) as [THREE.Vector3, THREE.Vector3]);
  known = { corners, normals, around, edges: chain(segments) };
  topologies.set(geometry, known);
  bare.dispose();
  merged.dispose();
  return known;
}

/**
 * The flat face triangle `seed` is on: the triangles that touch it, corner to corner, and face its
 * way. Corners rather than edges, so a face split unevenly still holds together; and against the
 * seed, not each neighbor, so a gentle curve doesn't pass for flat.
 */
function face({ corners, normals, around }: Topology, seed: number): number[] {
  const found = new Set([seed]);
  for (const t of found) {
    for (let k = 0; k < 3; k++) {
      for (const u of around[corners[3 * t + k]]) if (!found.has(u) && normals[u].dot(normals[seed]) > FLAT) found.add(u);
    }
  }
  return [...found];
}

/** Triangle `t` of a geometry, indexed or not, in its own frame. */
function triangle(g: THREE.BufferGeometry, t: number): THREE.Triangle {
  const position = g.getAttribute('position');
  const [a, b, c] = [0, 1, 2].map((k) => new THREE.Vector3().fromBufferAttribute(position, g.index ? g.index.getX(3 * t + k) : 3 * t + k));
  return new THREE.Triangle(a, b, c);
}

/** Straight edges come split where other faces meet them: we join the pieces back up. */
function chain(segments: [THREE.Vector3, THREE.Vector3][]): [THREE.Vector3, THREE.Vector3][] {
  const key = (v: THREE.Vector3) => `${v.x.toFixed(4)},${v.y.toFixed(4)},${v.z.toFixed(4)}`;
  const ends = new Map<string, number[]>();
  segments.forEach((s, i) => s.forEach((v) => ends.set(key(v), [...(ends.get(key(v)) ?? []), i])));
  const used = new Set<number>();
  const out: [THREE.Vector3, THREE.Vector3][] = [];
  segments.forEach((s, i) => {
    if (used.has(i)) return;
    used.add(i);
    const line = [...s] as [THREE.Vector3, THREE.Vector3];
    const direction = s[1].clone().sub(s[0]).normalize();
    // Walk on from either end while exactly one other piece goes on in the same direction.
    for (const end of [0, 1] as const) {
      for (;;) {
        const next = (ends.get(key(line[end])) ?? []).filter((j) => !used.has(j));
        const j = next[0];
        if (next.length !== 1 || Math.abs(segments[j][1].clone().sub(segments[j][0]).normalize().dot(direction)) < Math.cos(PARALLEL)) break;
        used.add(j);
        const [a, b] = segments[j];
        line[end] = key(a) === key(line[end]) ? b : a;
      }
    }
    out.push(line);
  });
  return out;
}

/**
 * The feature a pointer's `ray` hits: the corner or else the edge nearest the ray within `reach`
 * (world units, about the pointer's size where it hits), else the flat face it's on if that's
 * bigger than the reach, else the spot itself. With `spot`, the spot.
 */
export function featureAt(hit: THREE.Intersection, ray: THREE.Ray, reach: number, spot: boolean): Feature {
  const mesh = hit.object as THREE.Mesh;
  const at = mesh.worldToLocal(hit.point.clone());
  if (spot || hit.faceIndex == null) return { mesh, kind: 'point', at };
  const known = topology(mesh.geometry);
  const { edges } = known;
  // How close to the ray tells how close on screen. Corners and edges far in front of or behind
  // what the ray hit don't count: they're on other faces.
  let closest = reach;
  function take(v: THREE.Vector3, off: number): boolean {
    if (off >= closest || Math.abs(v.clone().sub(ray.origin).dot(ray.direction) - hit.distance) >= 4 * reach) return false;
    closest = off;
    return true;
  }
  const world = edges.map((ends) => ends.map((v) => v.clone().applyMatrix4(mesh.matrixWorld)));
  let corner: THREE.Vector3 | null = null;
  for (const [i, ends] of world.entries()) {
    for (const k of [0, 1]) if (take(ends[k], ray.distanceToPoint(ends[k]))) corner = edges[i][k];
  }
  if (corner) return { mesh, kind: 'point', at: corner.clone() };
  let edge: [THREE.Vector3, THREE.Vector3] | null = null;
  const on = new THREE.Vector3();
  for (const [i, [a, b]] of world.entries()) {
    const off = Math.sqrt(ray.distanceSqToSegment(a, b, undefined, on));
    if (take(on, off)) edge = edges[i];
  }
  if (edge) return { mesh, kind: 'edge', a: edge[0].clone(), b: edge[1].clone() };
  const triangles = face(known, hit.faceIndex);
  const parts = triangles.map((t) => triangle(mesh.geometry, t));
  const area = parts.reduce((sum, t) => sum + t.getArea(), 0);
  if (area * mesh.matrixWorld.getMaxScaleOnAxis() ** 2 < (2 * reach) ** 2) return { mesh, kind: 'point', at };
  const center = parts.reduce((sum, t) => sum.addScaledVector(t.getMidpoint(new THREE.Vector3()), t.getArea() / area), new THREE.Vector3());
  return { mesh, kind: 'face', triangles, normal: known.normals[hit.faceIndex].clone(), at: center };
}

/** A feature where it is now, in the rack frame. */
function placed(f: Feature) {
  const m = f.mesh.matrixWorld;
  if (f.kind === 'point') return { kind: f.kind, a: f.at.clone().applyMatrix4(m), b: f.at.clone().applyMatrix4(m) };
  if (f.kind === 'edge') return { kind: f.kind, a: f.a.clone().applyMatrix4(m), b: f.b.clone().applyMatrix4(m) };
  const at = f.at.clone().applyMatrix4(m);
  const normal = f.normal.clone().applyNormalMatrix(new THREE.Matrix3().getNormalMatrix(m));
  return { kind: f.kind, a: at, b: at, plane: new THREE.Plane().setFromNormalAndCoplanarPoint(normal, at) };
}

/**
 * The shortest way between two features, and the angle between them where both have a direction.
 * Faces count as the planes they lie in, and are only a distance apart when parallel; so is an edge
 * from a face. Between points and edges, it's the closest two points.
 */
export function between(x: Feature, y: Feature): { from: THREE.Vector3 | null; to: THREE.Vector3 | null; mm: number | null; degrees: number | null } {
  let [p, q] = [placed(x), placed(y)];
  if (p.plane && !q.plane) [p, q] = [q, p];
  const direction = (f: typeof p) => (f.plane ? f.plane.normal : f.b.clone().sub(f.a).normalize());
  // The angle between two lines or two planes runs to 90 degrees; a line to a plane leaves 90 less.
  let angle = p.kind === 'point' || q.kind === 'point' ? null : Math.acos(Math.min(1, Math.abs(direction(p).dot(direction(q)))));
  if (angle !== null && !p.plane && q.plane) angle = Math.PI / 2 - angle;
  const degrees = angle !== null && angle > PARALLEL ? THREE.MathUtils.radToDeg(angle) : null;
  let from: THREE.Vector3 | null = null;
  let to: THREE.Vector3 | null = null;
  // Between two points, three hands back the first less the second, so we take those as they are.
  if (p.kind === 'point' && q.kind === 'point') [from, to] = [p.a, q.a];
  else if (!q.plane) {
    [from, to] = [new THREE.Vector3(), new THREE.Vector3()];
    new THREE.Line3(p.a, p.b).distanceSqToLine3(new THREE.Line3(q.a, q.b), from, to);
  } else if (degrees === null) {
    from = p.a.clone().lerp(p.b, 0.5);
    to = q.plane.projectPoint(from, new THREE.Vector3());
  }
  return { from, to, mm: from && to ? from.distanceTo(to) : null, degrees };
}

/** The features picked and under the pointer, drawn over the scene. */
export class Ruler {
  readonly group = new THREE.Group();
  private picked: Feature[] = [];
  private hovered: Feature | null = null;

  private last = '';
  private clip: THREE.Plane;
  /** Where the device models are, each placed with its `Place` in its user data. */
  private devices: THREE.Object3D;
  private color: THREE.Color;
  private changed: (m: Measurement) => void;

  constructor(clip: THREE.Plane, devices: THREE.Object3D, color: THREE.Color, changed: (m: Measurement) => void) {
    [this.clip, this.devices, this.color, this.changed] = [clip, devices, color, changed];
  }

  hover(f: Feature | null) {
    this.hovered = f;
    this.draw();
  }

  /** Pick a feature; a third starts over, and none clears, the one under the pointer too. */
  pick(f: Feature | null) {
    this.picked = f ? [...(this.picked.length === 2 ? [] : this.picked), f] : [];
    if (!f) this.hovered = null;
    this.draw();
  }

  /** Draw again where the features are now, and say what that measures if it changed. */
  draw() {
    clear(this.group);
    for (const f of this.picked) this.add(f, 1);
    if (this.hovered) this.add(this.hovered, 0.5);
    const [x, y] = this.picked;
    const { from, to, mm, degrees } = y ? between(x, y) : { from: null, to: null, mm: null, degrees: null };
    if (from && to) this.group.add(line([from, to], this.color, 1), dot(from, this.color, 1), dot(to, this.color, 1));
    const [p, q] = this.picked.map((f) => this.place(f.mesh));
    const measurement = { picked: this.picked.length, mm, degrees, device: p && p === q ? (p.userData as Place) : null };
    const said = JSON.stringify(measurement);
    if (said !== this.last) this.changed(measurement);
    this.last = said;
  }

  /** The device placed in the rack that a mesh belongs to, if it's a device's. */
  private place(mesh: THREE.Object3D): THREE.Object3D | undefined {
    let at: THREE.Object3D | null = mesh;
    while (at && at.parent !== this.devices) at = at.parent;
    return at ?? undefined;
  }

  private add(f: Feature, opacity: number) {
    const m = f.mesh.matrixWorld;
    if (f.kind === 'point') this.group.add(dot(f.at.clone().applyMatrix4(m), this.color, opacity));
    else if (f.kind === 'edge') this.group.add(line([f.a, f.b].map((v) => v.clone().applyMatrix4(m)), this.color, opacity));
    else {
      const points = f.triangles.flatMap((t) => {
        const { a, b, c } = triangle(f.mesh.geometry, t);
        return [a, b, c].map((v) => v.applyMatrix4(m));
      });
      const material = new THREE.MeshBasicMaterial({ color: this.color, transparent: true, opacity: 0.45 * opacity, side: THREE.DoubleSide, depthWrite: false, clippingPlanes: [this.clip] });
      // Drawn just in front of the face, so the two don't fight over which shows.
      Object.assign(material, { polygonOffset: true, polygonOffsetFactor: -2, polygonOffsetUnits: -2 });
      this.group.add(new THREE.Mesh(new THREE.BufferGeometry().setFromPoints(points), material));
    }
  }
}

/** How big a pixel is, in world units, this far from the camera, on a view this many pixels tall. */
export const pixel = (camera: THREE.PerspectiveCamera, distance: number, height: number) => (2 * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)) * distance) / height;

/** A dot a few pixels across, however far away. */
function dot(at: THREE.Vector3, color: THREE.Color, opacity: number): THREE.Mesh {
  const mesh = new THREE.Mesh(new THREE.SphereGeometry(1, 12, 8), new THREE.MeshBasicMaterial({ color, transparent: true, opacity, depthTest: false }));
  mesh.position.copy(at);
  mesh.renderOrder = 1;
  mesh.onBeforeRender = (renderer, _, camera) => {
    mesh.scale.setScalar(4 * pixel(camera as THREE.PerspectiveCamera, camera.position.distanceTo(at), renderer.domElement.clientHeight));
    mesh.updateMatrixWorld();
  };
  return mesh;
}

/** A line a few pixels wide, drawn over everything. */
function line(points: THREE.Vector3[], color: THREE.Color, opacity: number): LineSegments2 {
  const material = new LineMaterial({ color, linewidth: 3, transparent: true, opacity, depthTest: false });
  const mesh = new LineSegments2(new LineSegmentsGeometry().setPositions(points.flatMap((p) => p.toArray())), material);
  mesh.renderOrder = 1;
  return mesh;
}

/** Empty a group, freeing what it drew with. */
export function clear(group: THREE.Group) {
  group.traverse((o) => {
    const mesh = o as THREE.Mesh;
    if (!mesh.geometry) return;
    mesh.geometry.dispose();
    [mesh.material].flat().forEach((m) => m.dispose());
  });
  group.clear();
}
