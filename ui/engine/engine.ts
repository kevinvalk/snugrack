// Promise wrapper around the geometry worker. Each request carries an id, so a reply always
// resolves the call that asked for it.

import type { Orientation, Project } from '../wasm/rackraft.js';
import type { Vec3 } from 'manifold-3d';
import type { Design, Request } from './worker.ts';

const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
const pending = new Map<number, { resolve: (v: any) => void; reject: (e: Error) => void }>();
let next = 1;

worker.onmessage = ({ data }) => {
  const reply = pending.get(data.call);
  pending.delete(data.call);
  if (data.ok) reply?.resolve(data.value);
  else reply?.reject(new Error(data.error));
};

// If the worker itself fails (out of memory, say), nothing would ever answer: fail every call.
worker.onerror = (e) => {
  for (const { reject } of pending.values()) reject(new Error(e.message || 'The geometry worker stopped.'));
  pending.clear();
};

function call<T>(req: Request, transfer: Transferable[] = []): Promise<T> {
  const id = next++;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve, reject });
    worker.postMessage({ ...req, call: id }, transfer);
  });
}

export const engine = {
  defaults: () => call<Project>({ type: 'defaults' }),
  /** Takes ownership of `positions`. */
  mesh: (id: string, positions: Float32Array, orientation: Orientation) =>
    call<void>({ type: 'mesh', id, positions, orientation }, [positions.buffer]),
  box: (id: string, size: Vec3) => call<void>({ type: 'box', id, size }),
  generate: (project: Project) => call<Design>({ type: 'generate', project }),
};
