// The dev server's side of feedback: it stores the notes the overlay sends, tells the overlay when
// notes change, and relays the MCP server's commands to the open tab. Only `npm run dev` uses it.

import type { ServerResponse } from 'node:http';
import { json } from 'node:stream/consumers';
import type { Plugin } from 'vite';
import * as store from './store.ts';

/** How long a command waits for the tab before giving up. */
const PATIENCE = 20_000;

export default function feedback(): Plugin {
  return {
    name: 'rackraft-feedback',
    apply: 'serve',
    configureServer(server) {
      // Commands for the tab, each answered by the call it carries.
      const waiting = new Map<number, (answer: unknown) => void>();
      let calls = 0;
      server.ws.on('feedback:answer', (answer: { call: number }) => {
        waiting.get(answer.call)?.(answer);
        waiting.delete(answer.call);
      });
      server.watcher.on('all', (_, path) => path.startsWith(store.DIR) && server.ws.send('feedback:changed', {}));

      server.middlewares.use('/__feedback', async (req, res) => {
        try {
          const path = req.url ?? '/';
          if (req.method === 'GET' && path === '/notes') return send(res, await store.list());
          if (req.method === 'POST' && path === '/notes') {
            const { note, png } = (await json(req)) as { note: Omit<store.Note, 'id' | 'status'>; png: string };
            return send(res, await store.create(note, Buffer.from(png, 'base64')));
          }
          const id = path.match(/^\/notes\/(\d+)$/)?.[1];
          if (id && req.method === 'POST') return send(res, await store.update(id, (await json(req)) as store.Change));
          if (id && req.method === 'DELETE') return send(res, await store.remove(id));
          if (req.method === 'POST' && path === '/tab') {
            const call = ++calls;
            const answer = new Promise((resolve) => {
              waiting.set(call, resolve);
              setTimeout(() => waiting.delete(call) && resolve({ ok: false, error: 'No Rackraft tab answered: open the app in a browser.' }), PATIENCE);
            });
            server.ws.send('feedback:command', { call, ...((await json(req)) as object) });
            return send(res, await answer);
          }
          res.statusCode = 404;
          res.end();
        } catch (e) {
          res.statusCode = 500;
          res.end(String(e));
        }
      });
    },
  };
}

function send(res: ServerResponse, value: unknown) {
  res.setHeader('Content-Type', 'application/json');
  res.end(JSON.stringify(value));
}
