// three.js viewport. Rack frame: X right, Y up, Z towards the viewer, millimeters.

import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { RoomEnvironment } from 'three/examples/jsm/environments/RoomEnvironment.js';
import { toCreasedNormals } from 'three/examples/jsm/utils/BufferGeometryUtils.js';
import type { PartMesh, Source } from '../engine/worker.ts';
import { clear, featureAt, pixel, REACH, Ruler, SHARP, type Feature, type Measurement } from './measure.ts';
import type { Place, Selection } from '../studio.svelte.ts';
import type { PartKind } from '../wasm/snugrack.js';

export type Layer = 'device' | 'faceplate' | 'cage';
export type ViewName = 'iso' | 'front' | 'top' | 'side' | 'back';
export type Section = { axis: 'x' | 'y' | 'z'; at: number };
export type Tool = 'measure' | 'hole' | null;
/**
 * A device's hole through the faceplate, by its device's place and its own, and where it is in the rack frame; `origin`
 * is where its device's own frame is, to take a move back to.
 */
type Hole = Place & { hole: number; x: number; y: number; diameter: number; origin: number[] };

const LAYER: Record<PartKind, Layer> = { faceplate: 'faceplate', leftEar: 'faceplate', rightEar: 'faceplate', cage: 'cage' };
/** Where each view looks from, towards the rack. */
const VIEWS: Record<ViewName, [number, number, number]> = { iso: [0.55, 0.4, 0.75], front: [0, 0, 1], top: [0, 1, 0.0001], side: [1, 0, 0], back: [-0.45, 0.35, -0.85] };
/** Every printed part in one color, as a slicer shows one filament. */
const PART = 0xc5ceda;
const GRID = 0x94a3b8;
/** A color of the app's theme (`app.css`), so the view and the interface agree. */
const theme = (name: string) => new THREE.Color(getComputedStyle(document.documentElement).getPropertyValue(name).trim());
/** The closest the camera gets to the point it orbits, in mm. */
const NEAREST = 5;
/** How far the pointer may move between press and release, in pixels, for a click rather than a drag. */
const CLICK = 4;
/** Faces meeting at less than this are smooth-shaded together; sharper ones keep a crisp edge. */
const CREASE = THREE.MathUtils.degToRad(SHARP);
/**
 * The explode runs from 0 to 1 in two steps, devices staying put. Up to OUT, faceplates slide forward and cages back
 * until they clear every device, the reverse of putting them on. That says how the rack goes together, so it gets
 * most of the slider; past OUT, each row's pieces part at their seams, cages with theirs, so the joints show.
 */
const OUT = 2 / 3;
/** The air between what slid apart, in mm, once it has; and how far each seam opens. */
const CLEAR = 10;
const SPREAD = 25;

type Camera = { position: number[]; target: number[] };
type Part = { row: number; kind: PartKind; device: number | null };
/** What's under the pointer: for pinning feedback to a spot on a part, in development. */
export type Probe = { point: number[]; normal: number[]; part: Part; source?: Source; camera: Camera };

/** The viewer on screen, for the feedback overlay in development. */
export let current: Viewer | undefined;

export class Viewer {
  // In development the last frame stays readable, so feedback can take a screenshot of it.
  private renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: import.meta.env.DEV });
  private scene = new THREE.Scene();
  private camera = new THREE.PerspectiveCamera(30, 1, 1, 20000);
  private controls: OrbitControls;
  private sun = new THREE.DirectionalLight(0xffffff, 1.2);
  private grid = new THREE.Group();
  private parts = new THREE.Group();
  private devices = new THREE.Group();
  private markers = new THREE.Group();
  private clip = new THREE.Plane(new THREE.Vector3(-1, 0, 0), Infinity);
  private observer: ResizeObserver;
  private bounds = new THREE.Box3(new THREE.Vector3(-240, -45, -150), new THREE.Vector3(240, 0, 0));
  private state = { explode: 0, hidden: new Set<Layer>(), selected: null as Selection, tool: null as Tool, holes: [] as Hole[], section: null as Section | null };
  private accent = theme('--color-accent');
  private cut = theme('--color-warn');
  private ruler = new Ruler(this.clip, this.devices, this.accent, (m) => this.on.measure(m));
  /** Round the selected hole, on the front of the faceplate it goes through, wherever the explode has moved that. */
  private ring = new THREE.LineLoop(new THREE.BufferGeometry().setFromPoints(new THREE.EllipseCurve(0, 0, 1, 1).getPoints(64)), new THREE.LineBasicMaterial({ color: this.accent, depthTest: false }));
  /** A hole being dragged, and where to, in the rack frame as assembled. */
  private moving: { hole: Hole; x: number; y: number } | null = null;
  /** Round the selected device's model: one with its own rack ears has no parts to light up. */
  private outline = new THREE.Box3Helper(new THREE.Box3(), this.accent);
  /** We draw only when something changed, not every frame: an idle view costs nothing. */
  private dirty = true;

  constructor(
    private host: HTMLElement,
    private on: {
      pick: (pick: Selection) => void;
      measure: (m: Measurement) => void;
      /** Put a hole here, in the rack frame as assembled, for a device in this row, or this device. */
      hole: (at: { row: number; device: number | null }, x: number, y: number) => void;
      /** A hole dragged to here, in the rack frame as assembled. */
      move: (hole: Hole, x: number, y: number) => void;
    },
  ) {
    this.renderer.setPixelRatio(window.devicePixelRatio);
    this.renderer.localClippingEnabled = true;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    host.appendChild(this.renderer.domElement);
    // Soft, even light from a studio-like room, plus a sun from above and in front for depth.
    this.scene.environment = new THREE.PMREMGenerator(this.renderer).fromScene(new RoomEnvironment(), 0.04).texture;
    this.scene.environmentIntensity = 0.9;
    this.sun.position.set(0.3, 1, 0.6);
    this.ring.renderOrder = 2;
    this.scene.add(this.sun, this.grid, this.parts, this.devices, this.ruler.group, this.markers, this.ring, this.outline);
    if (import.meta.env.DEV) current = this;
    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = true;
    this.controls.zoomToCursor = true;
    // The controls apply a wheel zoom inside their own handler, so the frame loop's update() sees
    // nothing left to do: their change event is what says to draw.
    this.controls.addEventListener('change', () => (this.dirty = true));
    this.observer = new ResizeObserver(() => this.resize());
    this.observer.observe(host);
    this.resize();
    this.setView('iso');
    this.renderer.setAnimationLoop(() => {
      if (this.controls.update() || this.dirty) {
        // Each zoom step shrinks the distance to the orbit point by a share, so right up to it
        // zooming and panning would all but stop: there the point moves on ahead instead.
        const offset = this.camera.position.clone().sub(this.controls.target);
        const distance = offset.length();
        if (distance < NEAREST) this.controls.target.sub(offset.setLength(NEAREST - distance));
        // Near and far follow the zoom, so nothing clips away up close or far out.
        const size = this.bounds.getSize(new THREE.Vector3()).length();
        this.camera.near = Math.max(distance, NEAREST) / 1000;
        this.camera.far = Math.max(distance * 100, size * 20);
        this.camera.updateProjectionMatrix();
        this.renderer.render(this.scene, this.camera);
      }
      this.dirty = false;
    });
    // A click that didn't drag picks what's under the pointer.
    let down = { x: 0, y: 0 };
    host.addEventListener('pointerdown', (e) => (down = { x: e.clientX, y: e.clientY }));
    // Pressing on a hole selects it and drags it, instead of the view, over the face it's on; letting go puts it there.
    host.addEventListener(
      'pointerdown',
      (e) => {
        const hole = e.button === 0 && this.state.tool !== 'measure' ? this.holeAt(this.ray(e)) : undefined;
        if (!hole) return;
        e.stopPropagation();
        host.setPointerCapture(e.pointerId);
        const from = { x: e.clientX, y: e.clientY };
        this.on.pick({ row: hole.row, device: hole.device, hole: hole.hole });
        this.moving = { hole, x: hole.x, y: hole.y };
        const grab = this.onFace(this.ray(e), hole)!;
        const move = (m: PointerEvent) => {
          const at = this.onFace(this.ray(m), hole);
          if (at && this.moving) [this.moving.x, this.moving.y] = [hole.x + at.x - grab.x, hole.y + at.y - grab.y];
          this.apply();
        };
        const up = (u: PointerEvent) => {
          host.removeEventListener('pointermove', move);
          const moved = this.moving;
          this.moving = null;
          if (moved && Math.hypot(u.clientX - from.x, u.clientY - from.y) >= CLICK) this.on.move(hole, moved.x, moved.y);
          this.apply();
        };
        host.addEventListener('pointermove', move);
        host.addEventListener('pointerup', up, { once: true });
      },
      { capture: true },
    );
    // Before the controls see a zoom or a drag, which they listen for on the canvas inside.
    host.addEventListener('wheel', (e) => this.retarget(e), { capture: true, passive: true });
    host.addEventListener('pointerdown', (e) => this.retarget(e), { capture: true });
    // The other buttons pan and orbit, whether they moved or not.
    host.addEventListener('pointerup', (e) => {
      if (e.button === 0 && !this.moving && Math.hypot(e.clientX - down.x, e.clientY - down.y) < CLICK) this.click(e);
    });
    // While measuring, what's under the pointer lights up: looked for once a frame at most.
    let pointer: PointerEvent | null = null;
    host.addEventListener('pointermove', (e) => {
      if (e.buttons) return;
      if (!pointer) {
        requestAnimationFrame(() => {
          if (pointer) this.hover(pointer);
          pointer = null;
        });
      }
      pointer = e;
    });
    host.addEventListener('pointerleave', () => {
      pointer = null;
      this.ruler.hover(null);
      this.dirty = true;
    });
  }

  dispose() {
    if (current === this) current = undefined;
    this.renderer.setAnimationLoop(null);
    this.observer.disconnect();
    this.controls.dispose();
    clear(this.parts);
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }

  private resize() {
    const { clientWidth: w, clientHeight: h } = this.host;
    if (!w || !h) return;
    this.renderer.setSize(w, h);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.dirty = true;
  }

  setParts(meshes: PartMesh[]) {
    clear(this.parts);
    this.ruler.pick(null);
    const rows = new Map<number, THREE.Box3[]>();
    for (const p of meshes) {
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute('position', new THREE.BufferAttribute(p.positions, 3));
      geometry.setIndex(new THREE.BufferAttribute(p.indices, 1));
      geometry.computeBoundingBox();
      const shaded = toCreasedNormals(geometry, CREASE);
      const surface = new THREE.Mesh(shaded, new THREE.MeshStandardMaterial({ roughness: 0.6, metalness: 0, clippingPlanes: [this.clip] }));
      // Where a section cuts a part open, its back faces show: we color them as the cut face. Only
      // then, or they'd flicker through wherever two faces lie close.
      const inside = new THREE.Mesh(shaded, new THREE.MeshBasicMaterial({ color: this.cut, side: THREE.BackSide, clippingPlanes: [this.clip] }));
      surface.name = 'surface';
      inside.name = 'inside';
      const edges = new THREE.LineSegments(
        new THREE.EdgesGeometry(geometry, SHARP),
        new THREE.LineBasicMaterial({ color: 0x000000, transparent: true, opacity: 0.25, clippingPlanes: [this.clip] }),
      );
      const base = new THREE.Matrix4().fromArray(p.placement);
      const box = geometry.boundingBox!.clone().applyMatrix4(base);
      geometry.dispose();
      const part = new THREE.Group().add(surface, inside, edges);
      part.matrixAutoUpdate = false;
      part.userData = { kind: p.kind, layer: LAYER[p.kind], row: p.row, device: p.device, sources: p.sources, base, box, offset: new THREE.Vector3() };
      this.parts.add(part);
      if (part.userData.layer === 'faceplate') rows.set(p.row, [...(rows.get(p.row) ?? []), box]);
    }
    // A seam halfway between neighbors: a tongue reaches past it, the groove it goes into starts before it. Each part
    // moves apart with the seams to its left, less half of them, so a row spreads out from its middle.
    const seams = new Map(
      [...rows].map(([row, boxes]) => {
        boxes.sort((a, b) => a.min.x + a.max.x - b.min.x - b.max.x);
        return [row, boxes.slice(1).map((b, i) => (boxes[i].max.x + b.min.x) / 2)];
      }),
    );
    for (const part of this.parts.children) {
      const at = seams.get(part.userData.row) ?? [];
      const x = part.userData.box.getCenter(new THREE.Vector3()).x;
      part.userData.spread = at.filter((s) => x > s).length - at.length / 2;
    }
    // Place the parts before measuring them; they arrive in print orientation.
    this.apply();
    this.fitGrid();
  }

  /** Device models to show, each placed by its rack matrix. */
  setDevices(instances: { model: THREE.Object3D; matrix: number[]; place: Place }[]) {
    // The copies share their geometry and materials with the loaded models, which a section cuts like the parts.
    this.devices.clear();
    this.ruler.pick(null);
    for (const { model, matrix, place } of instances) {
      const copy = model.clone();
      copy.matrixAutoUpdate = false;
      copy.matrix.fromArray(matrix);
      copy.userData = place;
      copy.traverse((o) => {
        for (const m of [(o as THREE.Mesh).material ?? []].flat()) m.clippingPlanes = [this.clip];
      });
      this.devices.add(copy);
    }
    this.fitGrid();
  }

  set(changes: Partial<typeof this.state>) {
    if (changes.tool !== undefined && changes.tool !== this.state.tool) this.ruler.pick(null);
    Object.assign(this.state, changes);
    this.apply();
  }

  setView(view: ViewName) {
    // With a device selected we frame it and its parts, otherwise the whole rack.
    const { selected } = this.state;
    const focus = new THREE.Box3();
    for (const object of [...this.parts.children, ...this.devices.children]) {
      const { row, device } = object.userData;
      if (selected?.device != null && row === selected.row && device === selected.device) focus.expandByObject(object);
    }
    const box = focus.isEmpty() ? this.bounds : focus;
    const center = box.getCenter(new THREE.Vector3());
    const radius = box.getSize(new THREE.Vector3()).length() / 2;
    // Fit the bounding sphere in the narrower of the two fields of view, with some air around it.
    const half = THREE.MathUtils.degToRad(this.camera.fov / 2);
    const narrow = Math.min(half, Math.atan(Math.tan(half) * this.camera.aspect));
    const distance = (radius / Math.sin(narrow)) * 1.05;
    this.camera.position.copy(center).addScaledVector(new THREE.Vector3(...VIEWS[view]).normalize(), distance);
    this.controls.target.copy(center);
    this.dirty = true;
  }

  private apply() {
    const { hidden, selected, section } = this.state;
    // A section cuts across its axis, `at` running from 0 to 1 across the rack's extent.
    if (!section) this.clip.set(new THREE.Vector3(-1, 0, 0), Infinity);
    else {
      const k = { x: 0, y: 1, z: 2 }[section.axis];
      const [lo, hi] = [this.bounds.min.getComponent(k), this.bounds.max.getComponent(k)];
      this.clip.set(new THREE.Vector3().setComponent(k, -1), lo + (hi - lo) * section.at);
    }
    this.devices.visible = !hidden.has('device');
    const { forward, back } = this.travel();
    const [out, apart] = [Math.min(this.state.explode / OUT, 1), Math.max((this.state.explode - OUT) / (1 - OUT), 0)];
    for (const part of this.parts.children) {
      const { layer, row, device, base, spread, offset } = part.userData as Part & { layer: Layer; base: THREE.Matrix4; spread: number; offset: THREE.Vector3 };
      part.visible = !hidden.has(layer);
      offset.set(spread * SPREAD * apart, 0, (layer === 'faceplate' ? forward : -back) * out);
      part.matrix.makeTranslation(offset).multiply(base);
      const isSelected = selected?.row === row && selected.hole === undefined && (selected.device === null || selected.device === device);
      const surface = (part.getObjectByName('surface') as THREE.Mesh).material as THREE.MeshStandardMaterial;
      surface.color.set(PART).lerp(this.accent, isSelected ? 0.35 : 0);
      part.getObjectByName('inside')!.visible = this.clip.constant !== Infinity;
    }
    for (const dot of this.markers.children) dot.position.fromArray(dot.userData.point).add(this.offset(dot.userData.part));
    const found = this.state.holes.find((h) => h.row === selected?.row && h.device === selected.device && h.hole === selected.hole);
    const hole = this.moving ? { ...this.moving.hole, x: this.moving.x, y: this.moving.y } : found;
    this.ring.visible = !!hole && !hidden.has('faceplate');
    if (hole) this.ring.position.set(hole.x, hole.y, 0).add(this.plate(hole));
    this.ring.scale.setScalar(hole ? hole.diameter / 2 : 1);
    this.parts.updateMatrixWorld(true);
    this.devices.updateMatrixWorld(true);
    const device = this.devices.children.find((o) => o.userData.row === selected?.row && o.userData.device === selected?.device);
    this.outline.visible = !!device && this.devices.visible;
    if (device) this.outline.box.setFromObject(device);
    this.ruler.draw();
    this.dirty = true;
  }

  /**
   * How far faceplates slide forward to clear the front of every device and cage, and cages back to clear the back of
   * every device and faceplate, as assembled: nothing passes through anything on the way.
   */
  private travel() {
    const box = (layer: Layer) => this.parts.children.reduce((all, p) => (p.userData.layer === layer ? all.union(p.userData.box) : all), new THREE.Box3());
    const [plates, cages, devices] = [box('faceplate'), box('cage'), new THREE.Box3().setFromObject(this.devices)];
    // An empty box runs from +Infinity to -Infinity, so what's missing never holds anything back.
    const forward = Math.max(devices.max.z, cages.max.z) - plates.min.z;
    const back = cages.max.z - Math.min(devices.min.z, plates.min.z);
    return { forward: Math.max(forward, 0) + CLEAR, back: Math.max(back, 0) + CLEAR };
  }

  /** How far the explode has moved a part, by its place in the rack; nothing if it's gone. */
  private offset({ row, kind, device }: Part): THREE.Vector3 {
    const part = this.parts.children.find((p) => p.userData.row === row && p.userData.kind === kind && p.userData.device === device);
    return part?.userData.offset.clone() ?? new THREE.Vector3();
  }

  /**
   * How far the explode has moved the faceplate piece a hole is in: of its row's, the one it's deepest inside, since a
   * piece's tongue reaches into the next.
   */
  private plate({ row, x }: Hole): THREE.Vector3 {
    const inside = (p: THREE.Object3D) => Math.min(x - p.userData.box.min.x, p.userData.box.max.x - x);
    const pieces = this.parts.children.filter((p) => p.userData.row === row && p.userData.layer === 'faceplate');
    const piece = pieces.reduce<THREE.Object3D | undefined>((best, p) => (!best || inside(p) > inside(best) ? p : best), undefined);
    return piece?.userData.offset.clone() ?? new THREE.Vector3();
  }

  /** Where a ray meets the front of the faceplate a hole is in, in the rack frame as assembled. */
  private onFace(ray: THREE.Raycaster, hole: Hole): THREE.Vector3 | null {
    const by = this.plate(hole);
    const at = ray.ray.intersectPlane(new THREE.Plane(new THREE.Vector3(0, 0, 1), -by.z), new THREE.Vector3());
    return at && at.sub(by);
  }

  /** The hole under the pointer, unless something's in front of it: a hole is where its faceplate isn't. */
  private holeAt(ray: THREE.Raycaster): Hole | undefined {
    if (this.state.hidden.has('faceplate')) return undefined;
    const hit = this.hit(ray);
    return this.state.holes.find((h) => {
      const at = this.onFace(ray, h);
      const nearer = at && (!hit || hit.distance > at.clone().add(this.plate(h)).distanceTo(ray.ray.origin) - 0.01);
      return nearer && Math.hypot(at.x - h.x, at.y - h.y) <= h.diameter / 2;
    });
  }

  /**
   * The rack's extent, devices included, since those with their own ears print nothing; and a millimeter grid just
   * under it: lines every 10 mm, heavier every 50, on round numbers. Then a section cuts at its share of the new extent.
   */
  private fitGrid() {
    const extent = new THREE.Box3().setFromObject(this.parts).union(new THREE.Box3().setFromObject(this.devices));
    if (!extent.isEmpty()) this.bounds = extent;
    clear(this.grid);
    const { min, max } = this.bounds;
    const size = Math.ceil((Math.max(max.x - min.x, max.z - min.z) * 1.5) / 100) * 100 || 600;
    for (const [step, opacity] of [[10, 0.18], [50, 0.4]]) {
      const lines = new THREE.GridHelper(size, size / step, GRID, GRID);
      Object.assign(lines.material, { transparent: true, opacity, depthWrite: false });
      this.grid.add(lines);
    }
    const round = (v: number) => Math.round(v / 50) * 50;
    this.grid.position.set(round((min.x + max.x) / 2), min.y - 0.5, round((min.z + max.z) / 2));
    this.apply();
  }

  /** The spot on a part under the pointer, and the camera, both as in the assembled rack; and the code its triangle came from. */
  probe(e: MouseEvent): Probe | null {
    const hit = this.hit(this.ray(e), false);
    const data = hit?.object.parent?.userData;
    if (!hit || !data || hit.faceIndex == null || !hit.face) return null;
    const normal = hit.face.normal.clone().transformDirection(hit.object.matrixWorld);
    const source = (data.sources as Source[]).find((s) => hit.faceIndex! >= s.from && hit.faceIndex! < s.to);
    const back = data.offset.clone().negate();
    const camera = { position: this.camera.position.clone().add(back).toArray(), target: this.controls.target.clone().add(back).toArray() };
    return { point: hit.point.clone().add(back).toArray(), normal: normal.toArray(), part: { row: data.row, kind: data.kind, device: data.device }, source, camera };
  }

  /** Where the camera is and what it looks at. */
  get pose(): Camera {
    return { position: this.camera.position.toArray(), target: this.controls.target.toArray() };
  }

  set pose({ position, target }: Camera) {
    this.camera.position.fromArray(position);
    this.controls.target.fromArray(target);
    this.dirty = true;
  }

  /** Look as from a camera `probe` gave on a part, moved as far as the explode moved that part. */
  look({ position, target }: Camera, part: Part) {
    const by = this.offset(part);
    this.pose = { position: new THREE.Vector3(...position).add(by).toArray(), target: new THREE.Vector3(...target).add(by).toArray() };
  }

  /** Pins at spots on parts, as `probe` gives them, drawn over everything. They move with their part. */
  setMarkers(pins: { point: number[]; part: Part; color: number }[]) {
    clear(this.markers);
    const size = this.bounds.getSize(new THREE.Vector3()).length() / 250 || 1.5;
    for (const { point, part, color } of pins) {
      const dot = new THREE.Mesh(new THREE.SphereGeometry(size), new THREE.MeshBasicMaterial({ color, depthTest: false }));
      dot.userData = { point, part };
      dot.renderOrder = 2;
      this.markers.add(dot);
    }
    this.apply();
  }

  private ray(e: MouseEvent): THREE.Raycaster {
    const rect = this.renderer.domElement.getBoundingClientRect();
    const ray = new THREE.Raycaster();
    ray.setFromCamera(new THREE.Vector2(((e.clientX - rect.left) / rect.width) * 2 - 1, -((e.clientY - rect.top) / rect.height) * 2 + 1), this.camera);
    return ray;
  }

  /**
   * Put the orbit point at the depth of whatever is under the pointer, along the current view so
   * nothing jumps: zooming then heads for that surface, and panning moves it with the pointer.
   */
  private retarget(e: MouseEvent) {
    const hit = this.hit(this.ray(e));
    if (!hit) return;
    const view = this.controls.target.clone().sub(this.camera.position).normalize();
    const depth = hit.point.clone().sub(this.camera.position).dot(view);
    if (depth > 0) this.controls.target.copy(this.camera.position).addScaledVector(view, depth);
  }

  /** The nearest surface shown under the pointer, of a part or a device (unless left out), unless a section cuts it away. */
  private hit(ray: THREE.Raycaster, devices = true): THREE.Intersection | undefined {
    // Of a part, only its surface: the rest are marks drawn on it, and testing them costs as much.
    const surfaces = this.parts.children.filter((p) => p.visible).map((p) => p.getObjectByName('surface')!);
    const shown = [...surfaces, ...(devices && this.devices.visible ? this.devices.children : [])];
    return ray.intersectObjects(shown, true).find((h) => (h.object as THREE.Mesh).isMesh && this.clip.distanceToPoint(h.point) >= 0);
  }

  /** The feature under the pointer, at a hit: what's within REACH pixels of it counts. */
  private feature(e: MouseEvent): Feature | null {
    const ray = this.ray(e);
    const hit = this.hit(ray);
    if (!hit) return null;
    return featureAt(hit, ray.ray, REACH * pixel(this.camera, hit.distance, this.host.clientHeight), e.shiftKey);
  }

  /** Light up the feature under the pointer, if we're still measuring by the frame after it moved. */
  private hover(e: PointerEvent) {
    if (this.state.tool !== 'measure') return;
    this.ruler.hover(this.feature(e));
    this.dirty = true;
  }

  /**
   * Measure; or select what's under the pointer, the rack where it's nothing; or with the Hole tool, put a hole there,
   * on a faceplate or a device's front. Pressing on a hole has selected it already.
   */
  private click(e: PointerEvent) {
    const { tool } = this.state;
    if (tool === 'measure') {
      this.ruler.pick(this.feature(e));
      this.dirty = true;
      return;
    }
    const ray = this.ray(e);
    if (this.holeAt(ray)) return;
    const hit = this.hit(ray);
    let target = hit?.object;
    while (target && target.userData.row === undefined) target = target.parent ?? undefined;
    const data = target?.userData;
    if (tool !== 'hole') this.on.pick(data ? { row: data.row, device: data.device } : null);
    else if (data && (data.kind === undefined || data.layer === 'faceplate')) {
      const spot = hit!.point.clone().sub(data.offset ?? new THREE.Vector3());
      this.on.hole({ row: data.row, device: data.kind === undefined || data.kind === 'faceplate' ? data.device : null }, spot.x, spot.y);
    }
  }
}
