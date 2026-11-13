// MCP server for feedback during development: an agent reads the user's notes, pinned to spots on
// the model or the interface with the code they trace back to, replies and resolves them, and looks
// at the open app itself. Notes come from .feedback/; the app is reached through the dev server.
//
//   claude mcp add snugrack-feedback -- node feedback/mcp.ts

import { readFile } from 'node:fs/promises';
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';
import { z } from 'zod';
import * as store from './store.ts';

/** Where `npm run dev` serves the app. */
const DEV = process.env.SNUGRACK_DEV ?? 'http://localhost:5173';

const server = new McpServer({ name: 'snugrack-feedback', version: '0.1.0' });
const text = (value: unknown) => ({ type: 'text' as const, text: typeof value === 'string' ? value : JSON.stringify(value, null, 2) });
const image = (data: string) => ({ type: 'image' as const, data, mimeType: 'image/png' });

server.registerTool(
  'feedback_list',
  {
    description: 'The user\'s feedback notes: one line each with id, status, what it\'s about and where in the code it traces to.',
    inputSchema: { status: z.enum(['open', 'resolved', 'all']).default('open') },
  },
  async ({ status }) => {
    const notes = (await store.list()).filter((n) => status === 'all' || n.status === status);
    const line = (n: store.Note) => {
      const where = n.model ? `${n.model.part.name}${n.model.source ? `, ${n.model.source.name} (${n.model.source.at})` : ''}` : n.ui ? `${n.ui.file ?? '?'}:${n.ui.line ?? '?'}` : 'general';
      return `#${n.id} [${n.status}] ${where}: ${n.thread[0]?.text.split('\n')[0]}${n.thread.length > 1 ? ` (${n.thread.length - 1} replies)` : ''}`;
    };
    return { content: [text(notes.length ? notes.map(line).join('\n') : `No ${status} notes.`)] };
  },
);

server.registerTool(
  'feedback_get',
  {
    description: 'One note in full: the thread, where it\'s pinned (point, part, source code, camera, or UI element), the open document at the time, and the screenshot the user saw.',
    inputSchema: { id: z.string() },
  },
  async ({ id }) => {
    const note = await store.get(id);
    const png = await readFile(store.shot(id)).catch(() => null);
    return { content: [text(note), ...(png ? [image(png.toString('base64'))] : [])] };
  },
);

server.registerTool(
  'feedback_reply',
  {
    description: 'Answer a note: say what you changed or ask back, and resolve it when it\'s done. The user sees it in the app at once and can reopen it.',
    inputSchema: { id: z.string(), text: z.string(), resolve: z.boolean().optional() },
  },
  async ({ id, text: say, resolve }) => {
    const note = await store.update(id, { say: { who: 'agent', text: say }, status: resolve === undefined ? undefined : resolve ? 'resolved' : 'open' });
    return { content: [text(`#${note.id} is ${note.status}.`)] };
  },
);

server.registerTool(
  'app_screenshot',
  {
    description: 'A screenshot of the app open in the user\'s browser, now. With a note\'s id, from that note\'s camera, to compare with the screenshot it came with.',
    inputSchema: { note: z.string().optional() },
  },
  async ({ note }) => {
    const model = note ? (await store.get(note)).model : undefined;
    return { content: [image((await tab('screenshot', { model })) as string)] };
  },
);

server.registerTool(
  'app_state',
  { description: 'What the open app shows: the project, its models, the selection, the warnings and the parts with their sizes and weights.' },
  async () => ({ content: [text(await tab('state'))] }),
);

/** Run a command in the open tab, through the dev server. */
async function tab(command: string, args: object = {}): Promise<unknown> {
  const response = await fetch(`${DEV}/__feedback/tab`, { method: 'POST', body: JSON.stringify({ command, args }) }).catch(() => {
    throw new Error(`The dev server isn't running at ${DEV}: start it with npm run dev.`);
  });
  const answer = (await response.json()) as { ok: boolean; value?: unknown; error?: string };
  if (!answer.ok) throw new Error(answer.error);
  return answer.value;
}

await server.connect(new StdioServerTransport());
