<script lang="ts">
  import { Download, LoaderCircle, Moon, Pencil, RefreshCw, Sun, SunMoon, X } from '@lucide/svelte';
  import Inspector from './Inspector.svelte';
  import RackPanel from './RackPanel.svelte';
  import { partName, studio } from './studio.svelte.ts';
  import Viewport from './viewport/Viewport.svelte';

  studio.open();

  $effect(() => studio.save());

  // Typing into a field shouldn't queue a design per keystroke.
  $effect(() => {
    if (!$state.snapshot(studio.doc?.project)) return;
    const t = setTimeout(() => studio.generate(), 200);
    return () => clearTimeout(t);
  });

  const meshes = $derived(studio.design?.meshes ?? []);
</script>

<svelte:head><title>{studio.doc ? `${studio.doc.name} · SnugRack` : 'SnugRack'}</title></svelte:head>

<!-- Narrow, the view takes the full width, with the layout and the properties side by side below it. -->
<div class="grid h-screen grid-cols-2 grid-rows-[3.25rem_minmax(0,1fr)_minmax(0,40%)] xl:grid-cols-[16rem_minmax(0,1fr)_19rem] xl:grid-rows-[3.25rem_minmax(0,1fr)]">
  <header class="col-span-full grid grid-cols-[minmax(max-content,1fr)_minmax(0,max-content)_minmax(max-content,1fr)] items-center gap-2 border-b border-line bg-surface px-3">
    <!-- Where there's no room for the name, the mark alone. -->
    <div class="flex items-center gap-2">
      <picture>
        <source media="(width < 40rem)" srcset="favicon.svg" />
        <img src="logo.svg" alt="SnugRack" class="h-8" />
      </picture>
      <span class="text-xs text-muted" title="Version: when this was made">{import.meta.env.VERSION}</span>
    </div>
    <!-- The rack's name is the page's title, centered, and edits in place, as a document's name does in Google Docs
         or Figma: a field that reads as a title, with a pencil that says it edits. Its frame shows on hover and holds
         the focus ring, so the ring goes round the pencil rather than through it. -->
    {#if studio.doc}
      <label
        class="flex h-8 min-w-0 cursor-text items-center gap-1.5 rounded-lg border border-transparent px-2 text-muted focus-within:border-line focus-within:bg-raised focus-within:text-ink has-focus-visible:outline-2 has-focus-visible:outline-offset-1 has-focus-visible:outline-accent hover:border-line hover:bg-raised hover:text-ink"
        title="Rename this rack"
      >
        <input class="min-w-0 bg-transparent text-lg font-semibold text-ink outline-none [field-sizing:content]" aria-label="Rack name" bind:value={studio.doc.name} />
        <Pencil size={14} class="shrink-0" />
      </label>
    {/if}

    <div class="col-start-3 flex min-w-0 items-center justify-end gap-2">
      {#if studio.doc}
        {#key studio.saves}
          <span class="animate-settle whitespace-nowrap text-muted max-lg:hidden" title="Your rack and its models stay in this browser, saved as you go. Nothing is uploaded.">Saved in this browser</span>
        {/key}
      {/if}

      <span class="inline-flex items-center gap-1.5 whitespace-nowrap text-muted" role="status" data-designs={studio.designs}>
        {#if studio.notice}
          <span class="text-danger">{studio.notice}</span>
          <button class="btn btn-quiet" aria-label="Dismiss" title="Dismiss" onclick={() => (studio.notice = null)}><X size={16} /></button>
        {/if}
        {#if studio.status.busy}
          <LoaderCircle size={14} class="animate-spin" /> Designing…
        {:else if studio.status.error}
          <span class="text-danger">{studio.status.error}</span>
          <button class="btn btn-quiet" onclick={() => studio.generate()}>Try again</button>
        {:else if studio.status.ms !== undefined}
          {#key studio.designs}<span class="animate-settle max-sm:hidden">Designed in {(studio.status.ms / 1000).toFixed(1)} s</span>{/key}
          <button class="btn btn-quiet" aria-label="Design again" title="Design again" onclick={() => studio.generate()}><RefreshCw size={16} /></button>
        {/if}
      </span>

      <button
        class="btn btn-quiet"
        aria-label={studio.dark ? 'Switch to light mode' : 'Switch to dark mode'}
        title={studio.scheme
          ? `${studio.dark ? 'Switch to light mode' : 'Switch to dark mode'}; Shift-click to follow the system again`
          : `Following the system: switch to ${studio.dark ? 'light' : 'dark'} mode`}
        onclick={(e) => studio.setScheme(e.shiftKey ? null : studio.dark ? 'light' : 'dark')}
      >
        {#if !studio.scheme}<SunMoon size={16} />{:else if studio.dark}<Moon size={16} />{:else}<Sun size={16} />{/if}
      </button>
      <button
        class="btn border-accent bg-accent text-white"
        popovertarget="export"
        disabled={!meshes.length}
        title="Download the parts to print, as STL files. The rack itself saves in this browser as you go."
      >
        <Download size={16} /> Export STLs
      </button>
      <div id="export" popover class="mt-1 min-w-56 open:grid [position-area:bottom_span-left]">
        <button class="btn btn-quiet justify-between" popovertarget="export" popovertargetaction="hide" onclick={() => studio.exportAll()}>
          <span class="inline-flex items-center gap-1.5"><Download size={16} /> All parts (.zip)</span>
          <span class="text-muted">{studio.grams.toFixed(0)} g</span>
        </button>
        <hr class="my-1 border-line" />
        {#each meshes as m (`${m.row} ${m.kind} ${m.device}`)}
          <button class="btn btn-quiet justify-between gap-6" popovertarget="export" popovertargetaction="hide" onclick={() => studio.exportPart(m)} title="STL, oriented as it prints">
            {partName(m)}
            <span class="text-muted">{m.size.map((v) => v.toFixed(0)).join(' × ')} mm · {m.grams.toFixed(0)} g</span>
          </button>
        {/each}
      </div>
    </div>
  </header>

  <RackPanel />
  <Viewport />
  <Inspector />
</div>
