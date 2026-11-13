<script lang="ts" module>
  /** Where the library lives; a remote one works the same, if it allows other sites to read it. */
  const LIBRARY = `${import.meta.env.BASE_URL}library.json`;

  import type { Direction } from './wasm/rackraft.js';

  /** A device in the library: `glb` is a link to its model, absolute or relative to the library. */
  interface Entry {
    name: string;
    manufacturer: string;
    model?: string;
    category: string;
    glb: string;
    /** Which way the model's front and top face, where known. */
    front?: Direction;
    up?: Direction;
  }
</script>

<script lang="ts">
  // Picks a device from the library and downloads its model in the browser, to import it as if dropped
  // in. Any Add device menu opens it with `commandfor="library"`, and says which row with `data-row`, or by
  // leaving it out, that it's for a device with its own rack ears.
  import { studio } from './studio.svelte.ts';

  let dialog: HTMLDialogElement;
  let row: number | null = null;
  let entries = $state.raw<Entry[] | null>(null);
  let query = $state('');
  let job = $state<{ doing: string } | { failed: string; retry: () => void } | null>(null);
  let download: AbortController | null = null;

  const found = $derived.by(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    const hits = (entries ?? []).filter((e) => {
      const text = [e.name, e.model, e.manufacturer, e.category].join(' ').toLowerCase();
      return words.every((w) => text.includes(w));
    });
    return [...Map.groupBy(hits, (e) => `${e.manufacturer} · ${e.category}`)];
  });

  const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

  async function load() {
    job = { doing: 'Loading the library…' };
    try {
      const response = await fetch(LIBRARY);
      if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
      entries = await response.json();
      job = null;
    } catch (e) {
      job = { failed: `Couldn’t load the library: ${message(e)}`, retry: load };
    }
  }

  async function pick(entry: Entry) {
    const name = entry.model ?? entry.name;
    const { signal } = (download = new AbortController());
    const mb = (bytes: number) => (bytes / 1e6).toFixed(1);
    job = { doing: `Downloading ${name}…` };
    try {
      const response = await fetch(new URL(entry.glb, new URL(LIBRARY, location.href)), { signal });
      if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
      const total = Number(response.headers.get('content-length'));
      const chunks: Uint8Array[] = [];
      let got = 0;
      for await (const chunk of response.body!) {
        chunks.push(chunk);
        got += chunk.length;
        job = { doing: `Downloading ${name}… ${mb(got)}${total ? ` of ${mb(total)}` : ''} MB` };
      }
      job = { doing: `Adding ${name}…` };
      await studio.importModel(new File(chunks as BlobPart[], `${name}.glb`), row, entry);
      dialog.close();
    } catch (e) {
      // Closing the dialog cancels the download, and says nothing.
      if (!signal.aborted) job = { failed: `Couldn’t download ${name}: ${message(e)}`, retry: () => pick(entry) };
    }
  }
</script>

<dialog
  id="library"
  class="w-lg"
  aria-labelledby="library-title"
  bind:this={dialog}
  {@attach (d) =>
    d.addEventListener('command', (e) => {
      const at = ((e as CommandEvent).source as HTMLElement).dataset.row;
      row = at === undefined ? null : Number(at);
      if (!entries) load();
    })}
  onclose={() => {
    download?.abort();
    job = null;
  }}
>
  <h2 id="library-title" class="font-semibold">Device library</h2>
  <p class="mb-2 text-muted">Models from their makers: your browser downloads the one you pick.</p>
  <input class="input w-full" type="search" placeholder="Search by name, model or maker" aria-label="Search the library" bind:value={query} />
  <div class="mt-2 h-80 overflow-y-auto">
    {#each found as [group, items] (group)}
      <section aria-label={group}>
        <h3 class="px-2 pt-2 pb-1 text-xs font-medium text-muted">{group}</h3>
        {#each items as entry (entry.glb)}
          <button class="btn btn-quiet w-full justify-between text-ink" disabled={!!job && 'doing' in job} onclick={() => pick(entry)}>
            <span class="truncate">{entry.name}</span>
            <span class="text-muted">{entry.model}</span>
          </button>
        {/each}
      </section>
    {:else}
      {#if entries}<p class="p-2 text-muted">Nothing in the library matches “{query}”.</p>{/if}
    {/each}
  </div>
  <footer class="mt-3 flex items-center justify-end gap-2">
    <span class="mr-auto" role="status">
      {#if job && 'failed' in job}
        <span class="text-danger">{job.failed}</span>
      {:else if job}
        <span class="text-muted">{job.doing}</span>
      {/if}
    </span>
    {#if job && 'failed' in job}<button class="btn" onclick={job.retry}>Try again</button>{/if}
    <button class="btn" commandfor="library" command="close">Cancel</button>
  </footer>
</dialog>
