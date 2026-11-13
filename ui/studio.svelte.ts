// The open document and everything the UI does to it. The project is the core's input; models
// say where each device's shape comes from. Both are saved in localStorage as they change, and
// imported model files in IndexedDB, since they are too big for localStorage.

import { get, set } from 'idb-keyval';
import { tick, untrack } from 'svelte';
import { zipSync } from 'fflate';
import type { Object3D } from 'three';
import { engine } from './engine/engine.ts';
import type { Design, PartMesh } from './engine/worker.ts';
import { boxScene, download, loadModel, slug, stl } from './files.ts';
import type { Direction, Hole, Issue, Orientation, PartKind, Project, Row } from './wasm/snugrack.js';

export type Size = [number, number, number];

export interface Model extends Orientation {
  name: string;
  /** A plain box of this size, or the name of an imported file kept in IndexedDB under the model id. */
  box?: Size;
  file?: string;
}

export interface Doc {
  name: string;
  project: Project;
  models: Record<string, Model>;
}

/** A panel (a row, to the core), a device in it, or one of the device's holes; null for the rack itself. */
export type Selection = { row: number; device: number | null; hole?: number } | null;
/** Where a device sits: its row, and its place in the row. */
export type Place = { row: number; device: number };

type Status = { busy: true } | { busy: false; ms?: number; error?: string };

const KEY = 'snugrack.doc';

class Studio {
  doc = $state<Doc | null>(null);
  design = $state.raw<Design | null>(null);
  status = $state<Status>({ busy: true });
  selection = $state<Selection>(null);
  /** Designs finished so far, so a test can wait for the next one instead of guessing. */
  designs = $state(0);
  /** Saves so far, so the header can say so after each. */
  saves = $state(0);
  /** The last distance measured on a device in the viewport, in mm, and which device. */
  measured = $state<(Place & { mm: number }) | null>(null);
  /** What went wrong with the last import, until the next one works or the user dismisses it. */
  notice = $state<string | null>(null);
  /** Each model's 3D scene for the viewer, by model id. */
  scenes = $state.raw<Record<string, Object3D>>({});
  private positions = new Map<string, Float32Array>();
  private latest = 0;
  /** The project the design was made from, as JSON. */
  private made = '';

  /** Open the saved document, or start a new one. */
  async open() {
    // We were called Rackraft: a document saved under the old name opens too, and saves under the new one.
    const saved: Doc | null = JSON.parse(localStorage.getItem(KEY) ?? localStorage.getItem('rackraft.doc') ?? 'null');
    const project = await engine.defaults();
    // A setting added since the document was saved takes its default, and so does one saved empty
    // (JSON keeps a number that isn't one as null).
    const given = <T extends object>(o: T) => Object.fromEntries(Object.entries(o).filter(([, v]) => v !== null)) as T;
    const filled = (p: Project): Project => ({
      ...project,
      ...given(p),
      rows: p.rows.map((row) => ({ ...project.rows[0], ...given(row) })),
      fit: { ...project.fit, ...given(p.fit) },
      build: { ...project.build, ...given(p.build) },
    });
    const doc: Doc = saved ? { ...saved, project: filled(saved.project) } : { name: 'My rack', project, models: {} };
    // A model that fails to load shouldn't keep the rest from opening; the core reports it missing.
    await Promise.allSettled(Object.entries(doc.models).map(([id, m]) => this.register(id, m)));
    this.doc = doc;
  }

  /** Whether the design is of the project as it is: rows on screen are the rows in the project. */
  get current() {
    return JSON.stringify(this.doc?.project) === this.made;
  }

  /** Filament for every part, in grams. */
  get grams() {
    return (this.design?.meshes ?? []).reduce((sum, m) => sum + m.grams, 0);
  }

  /** Reads the whole document, so an effect that calls this re-runs on any change. */
  save() {
    if (!this.doc) return;
    localStorage.setItem(KEY, JSON.stringify(this.doc));
    // Not read here, or the effect would re-run on its own count.
    untrack(() => this.saves++);
  }

  /** Design the current project. A result that arrives after a newer request is dropped. */
  async generate() {
    if (!this.doc) return;
    const token = ++this.latest;
    const started = performance.now();
    this.status = { busy: true };
    try {
      const project = $state.snapshot(this.doc.project);
      const design = await engine.generate(project);
      if (token !== this.latest) return;
      this.design = design;
      this.made = JSON.stringify(project);
      this.status = { busy: false, ms: performance.now() - started };
      this.designs++;
    } catch (e) {
      if (token === this.latest) this.status = { busy: false, error: e instanceof Error ? e.message : String(e) };
    }
  }

  /** Add a model file as `place` does, facing the way `known` says, or else the way its format usually does. */
  async importModel(file: File, into: number | null, known: { front?: Direction; up?: Direction } = {}) {
    const id = crypto.randomUUID();
    // glTF is Y-up; STLs come from CAD tools, which mostly put Z up and the front towards -Y.
    const stl = file.name.toLowerCase().endsWith('.stl');
    const [front, up] = [known.front ?? (stl ? 'negY' : 'posZ'), known.up ?? (stl ? 'posZ' : 'posY')];
    const model: Model = { name: file.name.replace(/\.[^.]+$/, ''), front, up, scale: 1, file: file.name };
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      await this.register(id, model, bytes);
      await set(id, bytes);
    } catch (e) {
      this.notice = `Couldn’t read ${file.name}: ${e instanceof Error ? e.message : e}`;
      return;
    }
    this.notice = null;
    this.doc!.models[id] = model;
    this.place(id, into);
  }

  /** A plain box, for a device without a 3D model, put as `place` does: its size is set in the Inspector. */
  addBox(into: number | null) {
    const id = crypto.randomUUID();
    const model: Model = { name: 'Box', front: 'posZ', up: 'posY', scale: 1, box: [200, 30, 120] };
    this.register(id, model);
    this.doc!.models[id] = model;
    this.place(id, into);
  }

  /**
   * Put a model at the end of a row, or with `row` null, as a device with its own rack ears in a row of its own: the
   * last if it's still empty, as a new rack's is. A row picked before a download may have gone since, or taken a rack
   * device: then a row of its own too.
   */
  place(id: string, row: number | null) {
    const rows = this.doc!.project.rows;
    const racked = row === null;
    if (row === null || !rows[row] || rows[row].racked) {
      const last = rows[rows.length - 1];
      if (last.racked || last.devices.length) this.addRow();
      row = rows.length - 1;
      rows[row].racked = racked;
    }
    const devices = rows[row].devices;
    devices.push(id);
    this.selection = { row, device: devices.length - 1 };
  }

  /**
   * Turn a model a quarter turn about one of the rack's axes, as it shows in the view: about x it
   * tips, about y it turns, about z it rolls; `by` is +1 or -1, right-handed. Its new front is
   * whatever in the model now faces the rack's front (z), its new top whatever now faces up (y).
   */
  turn(id: string, axis: 0 | 1 | 2, by: 1 | -1) {
    const model = this.doc!.models[id];
    const [front, up] = [AXES[model.front], AXES[model.up]];
    const across = cross(up, front);
    const back = (v: Size): Direction => {
      const r = quarter(v, axis, -by);
      const d = [0, 1, 2].map((k) => r[0] * across[k] + r[1] * up[k] + r[2] * front[k]);
      return (Object.keys(AXES) as Direction[]).find((k) => AXES[k].every((c, i) => c === d[i]))!;
    };
    [model.front, model.up] = [back([0, 0, 1]), back([0, 1, 0])];
    this.reshape(id);
  }

  /** The holes of the device at a place: those of its model, which go wherever it does. */
  holes({ row, device }: Place): Hole[] {
    const { rows, holes } = this.doc!.project;
    return holes.filter((h) => h.model === rows[row]?.devices[device]);
  }

  /** A hole for a cable through the faceplate, in the device's frame: `x` from its middle, `y` up from its bottom. */
  addHole(at: Place, x: number, y: number) {
    const model = this.doc!.project.rows[at.row].devices[at.device];
    this.doc!.project.holes.push({ model, x: round(x), y: round(y), diameter: 10 });
    this.selection = { ...at, hole: this.holes(at).length - 1 };
  }

  moveHole(at: Place, hole: number, x: number, y: number) {
    Object.assign(this.holes(at)[hole], { x: round(x), y: round(y) });
  }

  /** Move a device into a panel, before the device at `to.device`. */
  move(from: Place, to: Place) {
    const rows = this.doc!.project.rows;
    const source = rows[from.row];
    const [id] = source.devices.splice(from.device, 1);
    const at = from.row === to.row && from.device < to.device ? to.device - 1 : to.device;
    rows[to.row].devices.splice(at, 0, id);
    this.selection = { row: this.tidy(source, to.row), device: at };
  }

  /** Take a device out of its panel, to stand in the rack on its own rack ears, before the panel at `row`. */
  rack(from: Place, row: number) {
    const rows = this.doc!.project.rows;
    const source = rows[from.row];
    const [id] = source.devices.splice(from.device, 1);
    rows.splice(row, 0, { ...$state.snapshot(source), racked: true, devices: [id] });
    this.selection = { row: this.tidy(source, row), device: 0 };
  }

  /** Move a panel, or a device on its own rack ears, before the one at `to`. */
  moveRow(from: number, to: number) {
    const rows = this.doc!.project.rows;
    const [moved] = rows.splice(from, 1);
    const at = from < to ? to - 1 : to;
    rows.splice(at, 0, moved);
    this.selection = { row: at, device: moved.racked ? 0 : null };
  }

  /** A device on its own rack ears that moved away leaves no empty place behind: where the row at `row` is then. */
  private tidy(source: Row, row: number): number {
    const rows = this.doc!.project.rows;
    const at = rows.indexOf(source);
    if (!source.racked || source.devices.length) return row;
    rows.splice(at, 1);
    return at < row ? row - 1 : row;
  }

  /** Tell the worker a model's shape changed, after editing its orientation or size. */
  async reshape(id: string) {
    await this.register(id, this.doc!.models[id]);
    await this.generate();
  }

  addRow() {
    const rows = this.doc!.project.rows;
    rows.push({ ...rows[rows.length - 1], racked: false, devices: [] });
    this.selection = { row: rows.length - 1, device: null };
  }

  /**
   * Remove what's selected: a hole, a device from its panel, a device on its own rack ears, or a panel, asking first
   * if it still holds devices. The focus moves to what's selected then, in the layout, rather than dropping on the page.
   */
  async remove({ row, device, hole }: NonNullable<Selection>) {
    const { rows, holes } = this.doc!.project;
    const n = rows[row].racked ? 0 : rows[row].devices.length;
    if (device !== null && hole !== undefined) {
      holes.splice(holes.indexOf(this.holes({ row, device })[hole]), 1);
      this.selection = { row, device };
    } else if (device !== null && !rows[row].racked) {
      rows[row].devices.splice(device, 1);
      this.selection = { row, device: null };
    } else if (n === 0 || confirm(`Remove panel ${rowNumber(row)} and its ${n === 1 ? 'device' : `${n} devices`}?`)) {
      // A rack always has a row: removing the last leaves a new one like it.
      if (rows.length === 1) this.addRow();
      rows.splice(row, 1);
      this.selection = null;
    }
    await tick();
    document.querySelector<HTMLElement>('nav [aria-current="true"]')?.focus();
  }

  /** Every part as an STL, zipped, named by row and device. */
  exportAll() {
    const files = Object.fromEntries((this.design?.meshes ?? []).map((m) => [fileName(m), stl(m)]));
    download(`${slug(this.doc!.name)}.zip`, zipSync(files));
  }

  exportPart(m: PartMesh) {
    download(fileName(m), stl(m));
  }

  /** The model at a place in the rack, if it still exists. */
  model(row: number, device: number | null): Model | undefined {
    const id = device === null ? undefined : this.doc?.project.rows[row]?.devices[device];
    return id ? this.doc?.models[id] : undefined;
  }

  /** Load a model into the worker and the viewer. */
  private async register(id: string, model: Model, bytes?: Uint8Array) {
    if (model.box) {
      const size = $state.snapshot(model.box) as Size;
      this.scenes = { ...this.scenes, [id]: boxScene(size) };
      return engine.box(id, size);
    }
    if (!this.positions.has(id)) {
      const data = bytes ?? (await get<Uint8Array>(id));
      if (!data) return; // The file is gone; the core reports the device as missing.
      const { scene, positions } = await loadModel(model.file ?? 'model.glb', data.slice().buffer);
      this.positions.set(id, positions);
      this.scenes = { ...this.scenes, [id]: scene };
    }
    const orientation = { front: model.front, up: model.up, scale: model.scale };
    await engine.mesh(id, this.positions.get(id)!.slice(), orientation);
  }
}

export const studio = new Studio();

const AXES: Record<Direction, Size> = { posX: [1, 0, 0], negX: [-1, 0, 0], posY: [0, 1, 0], negY: [0, -1, 0], posZ: [0, 0, 1], negZ: [0, 0, -1] };
const cross = (a: Size, b: Size): Size => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
/** A quarter turn of `v` about axis 0, 1 or 2, by +1 or -1. */
function quarter([x, y, z]: Size, axis: 0 | 1 | 2, by: number): Size {
  if (axis === 0) return [x, -by * z, by * y];
  if (axis === 1) return [by * z, y, -by * x];
  return [-by * y, by * x, z];
}

const KINDS: Record<PartKind, string> = { faceplate: 'Faceplate', leftEar: 'Left ear', rightEar: 'Right ear', cage: 'Cage' };

/** Unique in the rack: where two devices in a row share a name, their place in it tells them apart. */
export function partName(m: PartMesh): string {
  const name = studio.model(m.row, m.device)?.name;
  const named = (studio.doc?.project.rows[m.row]?.devices ?? []).filter((id) => studio.doc?.models[id]?.name === name);
  const which = name && named.length > 1 ? `${name} (${m.device! + 1})` : name;
  return [`Panel ${rowNumber(m.row)}`, KINDS[m.kind], which].filter(Boolean).join(' · ');
}

/** A panel's number, counting panels only: devices on their own rack ears sit between them. */
export const rowNumber = (row: number) => studio.doc!.project.rows.slice(0, row + 1).filter((r) => !r.racked).length;

const fileName = (m: PartMesh) => `${slug(partName(m))}.stl`;

const mm = (v: number) => `${v.toFixed(1)} mm`;
const round = (v: number) => Math.round(v * 10) / 10;

export function describe(issue: Issue): string {
  switch (issue.kind) {
    case 'missingModel':
      return 'This device’s model is gone. Remove it and import the model again.';
    case 'noOutline':
      return 'Left out: seen from the front it’s too small to fit. Check its scale or size.';
    case 'tooTall':
      return `Left out: it needs ${mm(issue.height)} of height but the panel has ${mm(issue.row)}. Make the panel taller.`;
    case 'tooWide':
      return `Left out: it needs ${mm(issue.width)} across but there’s ${mm(issue.room)} left. Move it to another panel, or make the rack wider.`;
    case 'unlatched': {
      const { need, room } = issue;
      const why =
        need[0] > room[0]
          ? `the latch’s tongues need the device’s top and underside to run flat for ${mm(need[0])}, but they do for ${mm(room[0])}`
          : need[1] > room[1]
            ? `with the latch, the cage and its collar are ${mm(need[1])} tall, but the panel is ${mm(room[1])}`
            : `the latch’s tongues reach ${mm(need[2])} behind the panel, but the cage ends ${mm(room[2])} behind it`;
      const fix = need[1] > room[1] ? 'Make the panel taller, or use a smaller hook or less play.' : 'A smaller hook or less play needs less.';
      return `Not latched: ${why}, so the cage only slides into its collar. ${fix}`;
    }
  }
}
