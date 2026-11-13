<script lang="ts">
  // Feedback for the agent, in development: pin a note to a spot on a part or to an element of the
  // interface, or write a free one. Each goes to .feedback/ with what it points at, the code that
  // made it and a screenshot; the agent's answers come back here.
  import { MessageSquarePlus, Box, MousePointerClick, X } from '@lucide/svelte';
  import { domToPng } from 'modern-screenshot';
  import { partName, studio } from '../ui/studio.svelte.ts';
  import { current as viewer } from '../ui/viewport/viewer.ts';
  import type { Change, Note } from './store.ts';

  type Draft = Omit<Note, 'id' | 'status' | 'thread' | 'page'>;

  let open = $state(false);
  let notes = $state<Note[]>([]);
  let mode = $state<'' | 'model' | 'ui'>('');
  let draft = $state<{ note: Draft; png: string; text: string } | null>(null);
  let hovered = $state<DOMRect | null>(null);
  let capturing = $state(false);
  /** Notes being edited, with what they say so far, and replies being written. */
  let editing = $state<Record<string, string>>({});
  let replies = $state<Record<string, string>>({});
  /** Resolved notes stay out of sight, list and pins, unless the user asks for the archive. */
  let archive = $state(false);
  let root: HTMLElement;

  const shown = $derived(
    notes.filter((n) => archive || n.status === 'open').sort((a, b) => Number(a.status === 'resolved') - Number(b.status === 'resolved') || b.id.localeCompare(a.id)),
  );
  const openCount = $derived(notes.filter((n) => n.status === 'open').length);

  async function load() {
    notes = await (await fetch('/__feedback/notes')).json();
  }
  load();
  import.meta.hot?.on('feedback:changed', load);

  // Pins on the model, orange while open, green once resolved, red for the one being written.
  $effect(() =>
    viewer?.setMarkers(
      [...shown, ...(draft ? [{ ...draft.note, status: 'new' as const }] : [])].flatMap((n) =>
        n.model ? [{ point: n.model.point, part: n.model.part, color: { open: 0xf5a524, resolved: 0x22c55e, new: 0xef4444 }[n.status] }] : [],
      ),
    ),
  );

  /** The whole page as the user sees it, with this overlay's panel out of the way. */
  async function screenshot(): Promise<string> {
    capturing = true;
    await new Promise(requestAnimationFrame);
    try {
      return (await domToPng(document.documentElement, { scale: 1 })).split(',')[1];
    } finally {
      capturing = false;
    }
  }

  function start(kind: 'model' | 'ui') {
    mode = kind;
    draft = null;
  }

  /** Start a note, with a screenshot of the page as it is now, pin and all. */
  async function begin(note: Draft) {
    draft = { note, png: '', text: '' };
    // The user may have cancelled or started another note meanwhile, which takes a screenshot of its own.
    const mine = draft;
    const png = await screenshot();
    if (draft === mine) draft.png = png;
  }

  // While pinning, the next click goes to us, not to the app.
  function onClick(e: MouseEvent) {
    if (!mode || root.contains(e.target as Node)) return;
    e.preventDefault();
    e.stopPropagation();
    if (mode === 'model') pinModel(e);
    else pinUi(e.target as HTMLElement);
    mode = '';
  }

  function pinModel(e: MouseEvent) {
    const hit = viewer?.probe(e);
    if (!hit || !viewer) return;
    const mesh = studio.design?.meshes.find((m) => m.row === hit.part.row && m.kind === hit.part.kind && m.device === hit.part.device);
    const name = mesh ? partName(mesh) : `Panel ${hit.part.row + 1} ${hit.part.kind}`;
    const model = { ...hit, part: { ...hit.part, name } };
    begin({ model });
  }

  async function pinUi(el: HTMLElement) {
    // Svelte marks each element in development with the component and line that rendered it.
    let at: HTMLElement | null = el;
    while (at && !(at as any).__svelte_meta) at = at.parentElement;
    const loc = (at as any)?.__svelte_meta?.loc as { file: string; line: number; column: number } | undefined;
    const label = el.getAttribute('aria-label') ?? el.textContent?.replace(/\s+/g, ' ').trim().slice(0, 120) ?? '';
    hovered = el.getBoundingClientRect();
    await begin({ ui: { file: loc?.file, line: loc?.line, column: loc?.column, selector: path(el), text: label } });
    hovered = null;
  }

  function onMove(e: PointerEvent) {
    if (mode !== 'ui' || root.contains(e.target as Node)) return;
    hovered = (e.target as HTMLElement).getBoundingClientRect();
  }

  /** A selector that finds the element again: tags and positions from the page down. */
  function path(el: HTMLElement): string {
    const steps: string[] = [];
    for (let at: HTMLElement | null = el; at && at !== document.body; at = at.parentElement) {
      const same = at.parentElement ? [...at.parentElement.children].filter((c) => c.tagName === at!.tagName) : [];
      steps.unshift(`${at.tagName.toLowerCase()}${same.length > 1 ? `:nth-of-type(${same.indexOf(at) + 1})` : ''}`);
    }
    return steps.join(' > ');
  }

  async function save() {
    if (!draft?.text.trim() || !draft.png) return;
    const page = { selection: studio.selection, doc: $state.snapshot(studio.doc) };
    const note = { ...draft.note, thread: [{ who: 'user', text: draft.text.trim(), at: new Date().toISOString() }], page };
    await fetch('/__feedback/notes', { method: 'POST', body: JSON.stringify({ note, png: draft.png }) });
    draft = null;
    await load();
  }

  async function update(n: Note, change: Change) {
    await fetch(`/__feedback/notes/${n.id}`, { method: 'POST', body: JSON.stringify(change) });
    delete editing[n.id];
    replies[n.id] = '';
    await load();
  }

  async function remove(n: Note) {
    if (!confirm(`Delete note #${n.id} and its screenshot?`)) return;
    await fetch(`/__feedback/notes/${n.id}`, { method: 'DELETE' });
    await load();
  }

  function show(n: Note) {
    if (n.model && viewer) {
      viewer.look(n.model.camera, n.model.part);
      studio.selection = { row: n.model.part.row, device: n.model.part.device };
    } else if (n.ui) {
      const el = document.querySelector(n.ui.selector);
      el?.scrollIntoView({ block: 'nearest' });
      hovered = el?.getBoundingClientRect() ?? null;
      setTimeout(() => (hovered = null), 1500);
    }
  }

  // The agent, through the MCP server, asks the open tab for a screenshot or its state.
  type Command = { call: number; command: string; args: { model?: Note['model'] } };
  import.meta.hot?.on('feedback:command', async ({ call, command, args }: Command) => {
    try {
      let value: unknown;
      if (command === 'screenshot') {
        const before = viewer?.pose;
        if (args.model && viewer) viewer.look(args.model.camera, args.model.part);
        await new Promise((done) => setTimeout(done, 300));
        value = await screenshot();
        if (args.model && viewer && before) viewer.pose = before;
      } else if (command === 'state') {
        const meshes = studio.design?.meshes ?? [];
        value = {
          doc: $state.snapshot(studio.doc),
          selection: studio.selection,
          status: studio.status,
          warnings: studio.design?.warnings ?? [],
          parts: meshes.map((m) => ({ name: partName(m), size: m.size, grams: m.grams, sources: [...new Set(m.sources.map((s) => `${s.name} (${s.at})`))] })),
        };
      } else throw new Error(`Unknown command ${command}`);
      import.meta.hot?.send('feedback:answer', { call, ok: true, value });
    } catch (e) {
      import.meta.hot?.send('feedback:answer', { call, ok: false, error: String(e) });
    }
  });

  const where = (n: Pick<Note, 'model' | 'ui'>) =>
    n.model ? `${n.model.part.name}${n.model.source ? ` · ${n.model.source.name}` : ''}` : n.ui ? `${n.ui.text || n.ui.selector}${n.ui.file ? ` · ${n.ui.file}:${n.ui.line}` : ''}` : 'General';
</script>

<svelte:window onclickcapture={onClick} onpointermovecapture={onMove} onkeydown={(e) => e.key === 'Escape' && ((mode = ''), (hovered = null))} />

{#if hovered}
  <div class="pointer-events-none fixed z-50 rounded-lg border-2 border-accent" style:left="{hovered.left - 4}px" style:top="{hovered.top - 4}px" style:width="{hovered.width + 8}px" style:height="{hovered.height + 8}px"></div>
{/if}

<div bind:this={root} class={['fixed right-3 bottom-3 z-40 flex flex-col items-end gap-2', capturing && 'invisible']}>
  {#if open}
    <section class="flex max-h-[70vh] w-96 flex-col gap-2 overflow-y-auto rounded-lg border border-line bg-surface p-3 shadow-xl shadow-black/25" aria-label="Feedback">
      <header class="flex items-center gap-1">
        <h2 class="mr-auto font-semibold">Feedback</h2>
        <button class="btn btn-quiet" aria-pressed={mode === 'model'} title="Click a spot on a part" onclick={() => start('model')}><Box size={16} /> Model</button>
        <button class="btn btn-quiet" aria-pressed={mode === 'ui'} title="Click an element of the interface" onclick={() => start('ui')}><MousePointerClick size={16} /> UI</button>
        <button class="btn btn-quiet" title="A note about anything else" onclick={() => begin({})}><MessageSquarePlus size={16} /> Note</button>
      </header>
      {#if mode}
        <p class="text-muted">Click {mode === 'model' ? 'a spot on a part' : 'an element'} to pin a note to it. Esc cancels.</p>
      {/if}
      {#if draft}
        <form class="flex flex-col gap-2 rounded-lg bg-raised p-2" onsubmit={(e) => (e.preventDefault(), save())}>
          <p class="text-muted">{where(draft.note)}</p>
          <!-- svelte-ignore a11y_autofocus -->
          <textarea class="input h-24 py-1" placeholder="What should change, and why?" bind:value={draft.text} autofocus></textarea>
          <footer class="flex justify-end gap-2">
            <button type="button" class="btn" onclick={() => (draft = null)}>Cancel</button>
            <button class="btn border-accent bg-accent text-white" disabled={!draft.png}>Save</button>
          </footer>
        </form>
      {/if}
      {#each shown as n (n.id)}
        <article class={['flex flex-col gap-1 border-t border-line pt-2', n.status === 'resolved' && 'opacity-60']}>
          <button class="btn btn-quiet h-auto justify-start p-0 text-left whitespace-normal text-ink" onclick={() => show(n)}>
            <span class="text-muted">#{n.id}</span> {where(n)}
          </button>
          {#if editing[n.id] !== undefined}
            <form class="flex flex-col gap-2" onsubmit={(e) => (e.preventDefault(), editing[n.id].trim() && update(n, { text: editing[n.id].trim(), status: 'open' }))}>
              <!-- svelte-ignore a11y_autofocus -->
              <textarea class="input h-24 py-1" aria-label="Note" bind:value={editing[n.id]} autofocus></textarea>
              <footer class="flex justify-end gap-2">
                <button type="button" class="btn" onclick={() => delete editing[n.id]}>Cancel</button>
                <button class="btn border-accent bg-accent text-white">Save</button>
              </footer>
            </form>
          {:else}
            {#each n.thread as turn}
              <p class={['whitespace-pre-wrap', turn.who === 'agent' && 'border-l-2 border-accent pl-2']}>{turn.text}</p>
            {/each}
            <form class="flex gap-1" onsubmit={(e) => (e.preventDefault(), replies[n.id]?.trim() && update(n, { say: { who: 'user', text: replies[n.id].trim() }, status: 'open' }))}>
              <input class="input min-w-0 grow" placeholder="Reply" aria-label="Reply" bind:value={replies[n.id]} />
            </form>
            <footer class="flex justify-end gap-1">
              <button class="btn btn-quiet" title="Change what you first wrote" onclick={() => (editing[n.id] = n.thread[0].text)}>Edit</button>
              <button class="btn btn-quiet" onclick={() => remove(n)}>Delete</button>
              <button class="btn btn-quiet" onclick={() => update(n, { status: n.status === 'open' ? 'resolved' : 'open' })}>{n.status === 'open' ? 'Resolve' : 'Reopen'}</button>
            </footer>
          {/if}
        </article>
      {/each}
      {#if notes.length > openCount}
        <button class="btn btn-quiet self-start" aria-pressed={archive} onclick={() => (archive = !archive)}>
          {archive ? 'Hide resolved' : `Show resolved · ${notes.length - openCount}`}
        </button>
      {/if}
    </section>
  {/if}
  <button class="btn border-accent bg-accent text-white shadow-lg" aria-expanded={open} onclick={() => (open = !open)}>
    {#if open}<X size={16} />{:else}<MessageSquarePlus size={16} />{/if}
    Feedback{openCount ? ` · ${openCount}` : ''}
  </button>
</div>
