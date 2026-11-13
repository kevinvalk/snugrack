// Feedback notes in .feedback/: one JSON file and one screenshot each. The dev server writes them as
// the user pins them; the MCP server reads them and writes the agent's replies.

import { mkdir, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { Probe } from '../ui/viewport/viewer.ts';

export const DIR = join(import.meta.dirname, '..', '.feedback');

type Turn ={ who: 'user' | 'agent'; text: string; at: string };

export type Note = {
  id: string;
  status: 'open' | 'resolved';
  /** The user's note first, then replies either way. */
  thread: Turn[];
  /** A spot on a part: where; which part, named; the code its geometry came from; the camera. The spot
   * and the camera are as in the assembled rack, so they follow the part when the view explodes. */
  model?: Probe & { part: Probe['part'] & { name: string } };
  /** An element of the interface: the component and line that rendered it, and what it says. */
  ui?: { file?: string; line?: number; column?: number; selector: string; text: string };
  /** The app as the user saw it: selection, and the open document. */
  page: { selection: unknown; doc: unknown };
};

export const shot = (id: string) => join(DIR, `${id}.png`);

export async function list(): Promise<Note[]> {
  const files = await readdir(DIR).catch(() => [] as string[]);
  const notes = await Promise.all(files.filter((f) => f.endsWith('.json')).map((f) => get(f.slice(0, -5))));
  return notes.sort((a, b) => a.id.localeCompare(b.id));
}

export async function get(id: string): Promise<Note> {
  return JSON.parse(await readFile(join(DIR, `${id}.json`), 'utf8'));
}

/** Store a new note with its screenshot, numbered after the last one. */
export async function create(note: Omit<Note, 'id' | 'status'>, png: Buffer): Promise<Note> {
  await mkdir(DIR, { recursive: true });
  const last = (await list()).at(-1);
  const id = String((last ? Number(last.id) : 0) + 1).padStart(3, '0');
  const stored: Note = { id, status: 'open', ...note };
  await writeFile(shot(id), png);
  await save(stored);
  return stored;
}

export type Change = { say?: Omit<Turn, 'at'>; text?: string; status?: Note['status'] };

/** Add to a note's thread, rewrite what the user said, or open or resolve it. */
export async function update(id: string, change: Change): Promise<Note> {
  const note = await get(id);
  if (change.say) note.thread.push({ ...change.say, at: new Date().toISOString() });
  if (change.text) note.thread[0].text = change.text;
  if (change.status) note.status = change.status;
  await save(note);
  return note;
}

/** Remove a note and its screenshot. */
export async function remove(id: string): Promise<void> {
  await rm(join(DIR, `${id}.json`));
  await rm(shot(id), { force: true });
}

const save = (note: Note) => writeFile(join(DIR, `${note.id}.json`), JSON.stringify(note, null, 2));
