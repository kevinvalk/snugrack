// Geometry worker: the Rust core designs the parts, manifold-3d builds them.

import Module, { type Manifold, type ManifoldToplevel, type Mat4, type Vec3 } from 'manifold-3d';
import init, { Models, defaultProject, type Assembly, type Orientation, type Part, type Project, type Solid } from '../wasm/rackraft.js';
import wasmUrl from '../wasm/rackraft_bg.wasm?url';

export type Request =
  | { type: 'defaults' }
  | { type: 'mesh'; id: string; positions: Float32Array; orientation: Orientation }
  | { type: 'box'; id: string; size: Vec3 }
  | { type: 'generate'; project: Project };

/** A built part, oriented for printing and centered on the bed. */
export type PartMesh = Omit<Part, 'solid'> & {
  positions: Float32Array;
  indices: Uint32Array;
  /** Print frame to rack frame, column-major. */
  placement: number[];
  /** Print size: x, y, z. */
  size: Vec3;
  /** Filament, for PLA or PETG. */
  grams: number;
  /** Where its triangles come from: runs of them, by index, with the label and code that made them. */
  sources: Source[];
};

export type Source = { from: number; to: number; name: string; at: string };

/** About right for PLA and PETG. */
const DENSITY = 1.25;

export type Design = Omit<Assembly, 'parts'> & { meshes: PartMesh[] };

const ready = Promise.all([init({ module_or_path: wasmUrl }), Module()]).then(([, manifold]) => {
  manifold.setup();
  return { manifold, models: new Models() };
});

type Built = ReturnType<typeof build>;
let previous = new Map<string, Built>();
type Label = { name: string; at: string };
type Base = { solid: Manifold; labels: Map<number, Label> };
/**
 * What parts cut twice are before the second cut, by its tree, as built for the last design: a piece with holes is
 * cut for them last (see `generate` in src/rack.rs), so moving one only cuts that again, and moving its last one
 * away leaves what we have.
 */
let bases = new Map<string, Base>();

self.onmessage = async ({ data: { call, ...req } }: MessageEvent<Request & { call: number }>) => {
  try {
    const { manifold, models } = await ready;
    let value: unknown;
    let transfer: Transferable[] = [];
    if (req.type === 'defaults') value = defaultProject();
    else if (req.type === 'mesh') models.mesh(req.id, req.positions, req.orientation);
    else if (req.type === 'box') models.box(req.id, ...req.size);
    else {
      const { parts, ...rest } = models.generate(req.project);
      // Most edits leave most parts alone, so we reuse the previous design's builds where the tree
      // is the same. Copies go out, since their buffers are transferred and our builds must stay.
      const keys = parts.map((p) => JSON.stringify([p.bed, p.solid]));
      previous = new Map(keys.map((key, i) => [key, previous.get(key) ?? build(manifold, parts[i])]));
      // We keep what this design's parts are cut from, and let the rest go.
      const used = new Set(parts.map(({ solid }) => JSON.stringify(twice(solid) ? solid.base : solid)));
      for (const [key, base] of bases) if (!used.has(key)) (base.solid.delete(), bases.delete(key));
      const meshes = parts.map(({ solid: _, ...part }, i) => {
        const built = previous.get(keys[i])!;
        return { ...part, ...built, positions: built.positions.slice(), indices: built.indices.slice() };
      });
      transfer = meshes.flatMap((m) => [m.positions.buffer, m.indices.buffer]);
      value = { ...rest, meshes } satisfies Design;
    }
    self.postMessage({ call, ok: true, value }, { transfer });
  } catch (err) {
    self.postMessage({ call, ok: false, error: err instanceof Error ? err.message : String(err) });
  }
};

function build(m: ManifoldToplevel, { bed, solid: tree }: Part) {
  // Collapse anything finer than the core's resolution (`RESOLUTION` in src/poly.rs), as the clusters of
  // near-equal corners that offsetting leaves are; curves, already chords that close to them, stay as round.
  // Then turn the bed face down. Both happen in the kernel's doubles: the mesh we hand out is float32, and moving it
  // after that would round vertices onto each other. The back faces -z already; the front turns over about X.
  const flip = bed === 'front';
  const labels = new Map<number, Label>();
  const simple = consume(cut(m, tree, labels), (s) => s.simplify(0.01));
  const solid = flip ? consume(simple, (s) => s.rotate([180, 0, 0])) : simple;
  const { min, max } = solid.boundingBox();
  const t: Vec3 = [-(min[0] + max[0]) / 2, -(min[1] + max[1]) / 2, -min[2]];
  const printed = consume(solid, (s) => s.translate(t));
  const mesh = printed.getMesh();
  const grams = (printed.volume() / 1000) * DENSITY;
  printed.delete();
  // The kernel keeps track of which original shape each run of triangles came from.
  const sources: Source[] = [];
  mesh.runOriginalID.forEach((id, i) => {
    const label = labels.get(id);
    if (label) sources.push({ from: mesh.runIndex[i] / 3, to: mesh.runIndex[i + 1] / 3, ...label });
  });
  // Undo the translation, then the half turn about X if there was one (it is its own inverse).
  const s = flip ? -1 : 1;
  return {
    // Every solid we build has just a position per vertex, so the properties are the positions.
    positions: mesh.vertProperties.slice(),
    indices: mesh.triVerts.slice(),
    placement: [1, 0, 0, 0, 0, s, 0, 0, 0, 0, s, 0, -t[0], -s * t[1], -s * t[2], 1],
    size: [max[0] - min[0], max[1] - min[1], max[2] - min[2]] as Vec3,
    grams,
    sources,
  };
}

const twice = (s: Solid): s is Extract<Solid, { op: 'difference' }> => s.op === 'difference' && s.base.op === 'difference';

/** The part, from what we kept of it if we did; one cut twice, from what it was before the second cut, kept now. */
function cut(m: ManifoldToplevel, tree: Solid, labels: Map<number, Label>): Manifold {
  const key = JSON.stringify(twice(tree) ? tree.base : tree);
  let base = bases.get(key);
  if (!base && !twice(tree)) return evaluate(m, tree, labels);
  if (!base) {
    const own = new Map<number, Label>();
    base = { solid: evaluate(m, (tree as Extract<Solid, { op: 'difference' }>).base, own), labels: own };
    bases.set(key, base);
  }
  base.labels.forEach((label, id) => labels.set(id, label));
  // A copy, which the caller frees: a move costs nothing.
  if (!twice(tree)) return base.solid.translate([0, 0, 0]);
  const tool = combine(m, tree.cut.map((c) => evaluate(m, c, labels)));
  const out = base.solid.subtract(tool);
  tool.delete();
  return out;
}

// Prism profiles use cyclic axes: X -> (y, z), Y -> (z, x), Z -> (x, y). These column-major
// matrices map the extrusion frame back to world axes.
const AXIS: Record<'x' | 'y', Mat4> = {
  x: [0, 1, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 1],
  y: [0, 0, 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1],
};

/** `labels` collects, by the kernel's id for each labeled shape, its label. */
function evaluate(m: ManifoldToplevel, s: Solid, labels: Map<number, Label>): Manifold {
  switch (s.op) {
    case 'extrude': {
      const cs = new m.CrossSection(s.contours, 'NonZero');
      const prism = consume(m.Manifold.extrude(cs, s.hi - s.lo), (p) => p.translate([0, 0, s.lo]));
      cs.delete();
      return s.axis === 'z' ? prism : consume(prism, (p) => p.transform(AXIS[s.axis as 'x' | 'y']));
    }
    case 'mesh': {
      const mesh = new m.Mesh({ numProp: 3, vertProperties: Float32Array.from(s.vertices.flat()), triVerts: Uint32Array.from(s.triangles.flat()) });
      mesh.merge();
      return new m.Manifold(mesh);
    }
    case 'union':
      return combine(m, s.children.map((c) => evaluate(m, c, labels)));
    case 'difference': {
      const tool = combine(m, s.cut.map((c) => evaluate(m, c, labels)));
      const out = consume(evaluate(m, s.base, labels), (b) => b.subtract(tool));
      tool.delete();
      return out;
    }
    case 'label': {
      const original = consume(evaluate(m, s.solid, labels), (o) => o.asOriginal());
      labels.set(original.originalID(), { name: s.name, at: s.at });
      return original;
    }
  }
}

function combine(m: ManifoldToplevel, parts: Manifold[]): Manifold {
  const out = m.Manifold.union(parts);
  parts.forEach((p) => p.delete());
  return out;
}

/** Apply `f`, then free its input (manifold objects live on the wasm heap). */
function consume(input: Manifold, f: (m: Manifold) => Manifold): Manifold {
  const out = f(input);
  input.delete();
  return out;
}
