<script lang="ts">
  import { Box, Circle, HardDrive, LibraryBig, Plus, Rows3, Server, Trash2, TriangleAlert, Upload } from '@lucide/svelte';
  import Library from './Library.svelte';
  import { MODEL_FILES } from './files.ts';
  import { describe, rowNumber, studio, type Place, type Selection } from './studio.svelte.ts';
  import type { RackWidth } from './wasm/rackraft.js';

  const RACKS: Record<RackWidth['kind'], string> = { inch19: '19″', inch10: '10″', fit: 'Fit', custom: 'Custom' };


  const issues = (row: number, device: number | null) =>
    (studio.design?.warnings ?? []).filter((w) => w.row === row && w.device === device).map((w) => describe(w.issue));
  const selected = (s: Selection) => {
    const now = studio.selection;
    return now?.row === s?.row && now?.device === s?.device && now?.hole === s?.hole;
  };
  const item = 'btn btn-quiet w-full justify-start aria-[current=true]:bg-raised aria-[current=true]:font-medium aria-[current=true]:text-accent';

  /**
   * Drag and drop: a device goes into a panel, before one of its devices or at its end, or into the rack between
   * panels, where it stands on its own rack ears; a panel, or a device on its own ears, goes between panels. While
   * dragging, a line shows where it would land, and only places it can go take it.
   */
  type Drag = { device: Place } | { row: number };
  /** Before the `before`-th device of the panel at `into`, or with `into` null, before the rack's `before`-th item. */
  type Target = { into: number | null; before: number };
  let drag = $state<Drag | null>(null);
  let target = $state<Target | null>(null);

  /** Where a drop on the top or bottom half of an element would go, if anywhere this drag can, and does something. */
  function aim(e: DragEvent, top: Target | null, bottom: Target | null = top) {
    if (!drag) return;
    const { top: y, height } = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const t = e.clientY < y + height / 2 ? top : bottom;
    e.stopPropagation();
    // A panel goes only between panels.
    if (!t || ('row' in drag && t.into !== null) || !moves(t)) return (target = null);
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    target = t;
  }

  /** Over a panel's own line: its top half takes what goes before it, its bottom half a device into it, or a panel after it. */
  const aimAt = (e: DragEvent, r: number) => aim(e, { into: null, before: r }, drag && 'row' in drag ? { into: null, before: r + 1 } : { into: r, before: 0 });

  /** Whether a drop at `t` would change anything. */
  function moves(t: Target): boolean {
    const back = (at: number, before: number) => before === at || before === at + 1;
    if ('row' in drag!) return !back(drag.row, t.before);
    const { row, device } = drag!.device;
    if (studio.doc!.project.rows[row].racked) return t.into !== null || !back(row, t.before);
    return t.into !== row || !back(device, t.before);
  }

  function drop(e: DragEvent) {
    e.preventDefault();
    const [d, t] = [drag, target];
    [drag, target] = [null, null];
    if (!d || !t) return;
    const rows = studio.doc!.project.rows;
    if ('row' in d) studio.moveRow(d.row, t.before);
    else if (t.into !== null) studio.move(d.device, { row: t.into, device: t.before });
    else if (rows[d.device.row].racked) studio.moveRow(d.device.row, t.before);
    else studio.rack(d.device, t.before);
  }

  const dragged = (d: Drag) => drag !== null && JSON.stringify(drag) === JSON.stringify(d);
  /** What makes an element draggable as `d`. */
  const handle = (d: Drag) => ({
    draggable: 'true',
    ondragstart: (e: DragEvent) => {
      drag = d;
      if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
    },
    ondragend: () => ([drag, target] = [null, null]),
  });
  const menuItem = 'btn btn-quiet text-ink';

  /** Up and down go from item to item, selecting each, Home and End to the first and the last, as in a file tree. */
  function walk(e: KeyboardEvent) {
    const nodes = [...document.querySelectorAll<HTMLElement>('nav [data-node]')];
    const at = nodes.indexOf(e.target as HTMLElement);
    const to = { ArrowDown: at + 1, ArrowUp: at - 1, Home: 0, End: nodes.length - 1 }[e.key];
    if (at < 0 || to === undefined || !nodes[to]) return;
    e.preventDefault();
    nodes[to].focus();
    nodes[to].click();
  }
  /** What a selection is called, for its remove button. */
  function name({ row, device, hole }: NonNullable<Selection>) {
    if (hole !== undefined) return `hole ${hole + 1}`;
    if (device !== null) return studio.model(row, device)?.name ?? 'device';
    return `panel ${rowNumber(row)}`;
  }
</script>

{#snippet alert(messages: string[])}
  {#if messages.length}
    <span class="ml-auto text-warn" role="img" title={messages.join('\n')} aria-label={messages.join(' ')}><TriangleAlert size={16} /></span>
  {/if}
{/snippet}

<!-- Where a drop would land: a line, and between panels, what a device becomes there. -->
{#snippet line(into: number | null, before: number)}
  {#if target?.into === into && target.before === before}
    <li class="pointer-events-none relative h-0" aria-hidden="true">
      <span class="absolute inset-x-1 -top-px h-0.5 rounded-full bg-accent"></span>
      {#if into === null && drag && 'device' in drag && !studio.doc!.project.rows[drag.device.row].racked}
        <span class="absolute -top-2.5 right-1 rounded-full bg-accent px-1.5 text-xs text-white">On its own rack ears</span>
      {/if}
    </li>
  {/if}
{/snippet}

<!-- A button that selects what it names, marked while it is; and but for the rack, one that removes it, shown on hover
     and focus, as the Delete key does. -->
{#snippet node(select: Selection, body: import('svelte').Snippet, attributes: Record<string, unknown> = {})}
  <div class="group flex">
    <button
      class={[item, 'min-w-0']}
      data-node
      aria-current={selected(select)}
      onclick={() => (studio.selection = select)}
      onkeydown={(e) => (select && (e.key === 'Delete' || e.key === 'Backspace') ? studio.remove(select) : walk(e))}
      {...attributes}>{@render body()}</button
    >
    {#if select}
      <button
        class="btn btn-quiet hidden shrink-0 px-2 group-focus-within:inline-flex group-hover:inline-flex"
        aria-label="Remove {name(select)}"
        title="Remove (Delete)"
        onclick={() => studio.remove(select)}><Trash2 size={16} /></button
      >
    {/if}
  </div>
{/snippet}

<!-- A device by its place, with its problems, and its holes under it. One on its own rack ears says how many units it
     takes. -->
{#snippet device(r: number, d: number)}
  {@const { project, models } = studio.doc!}
  {@const { racked, devices } = project.rows[r]}
  {@const id = devices[d]}
  {#snippet label()}
    {#if models[id]?.box}<Box size={16} class="shrink-0" />{:else}<HardDrive size={16} class="shrink-0" />{/if}
    <span class="truncate">{models[id]?.name ?? 'Missing model'}</span>
    {#if racked}<span class="text-muted">{studio.design?.rows[r]?.units ?? '—'}U</span>{/if}
    {@render alert(issues(r, d))}
  {/snippet}
  {@const me = { device: { row: r, device: d } }}
  <div class={['cursor-grab', dragged(me) && 'opacity-40']}>
    {@render node({ row: r, device: d }, label, handle(me))}
  </div>
  {@const holes = studio.holes({ row: r, device: d })}
  {#if holes.length}
    <ul class="pl-4">
      {#each holes as hole, h}
        {#snippet holeName()}<Circle size={16} /> Hole {h + 1} <span class="text-muted">⌀ {hole.diameter} mm</span>{/snippet}
        <li>{@render node({ row: r, device: d, hole: h }, holeName)}</li>
      {/each}
    </ul>
  {/if}
{/snippet}
<!-- Add a device to a panel, or with `r` null, one on its own rack ears to the rack. -->
{#snippet add(r: number | null, label: string)}
  {@const { models } = studio.doc!}
  {@const menu = `add-${r ?? 'rack'}`}
  <button class={item} popovertarget={menu}><Plus size={16} /> {label}</button>
  <div id={menu} popover class="mt-0.5 min-w-56 open:grid [position-area:bottom_span-right]">
    {#each Object.entries(models) as [id, m] (id)}
      <button class={menuItem} popovertarget={menu} popovertargetaction="hide" onclick={() => studio.place(id, r)}>
        {#if m.box}<Box size={16} />{:else}<HardDrive size={16} />{/if}
        {m.name}
      </button>
    {/each}
    {#if Object.keys(models).length}<hr class="my-1 border-line" />{/if}
    <label class={menuItem}>
      <Upload size={16} /> Import model…
      <input
        type="file"
        accept={MODEL_FILES}
        hidden
        onchange={(e) => {
          const file = e.currentTarget.files?.[0];
          if (file) studio.importModel(file, r);
          e.currentTarget.value = '';
          document.getElementById(menu)?.hidePopover();
        }}
      />
    </label>
    <button class={menuItem} commandfor="library" command="show-modal" data-row={r}><LibraryBig size={16} /> From library…</button>
    <button class={menuItem} popovertarget={menu} popovertargetaction="hide" title="A plain box, for a device without a 3D model: set its size on the right" onclick={() => studio.addBox(r)}><Box size={16} /> New box</button>
  </div>
{/snippet}

<nav class="overflow-y-auto border-r border-line bg-surface p-3 max-xl:row-start-3 max-xl:border-t" aria-label="Rack layout">
  <h2 class="mb-2 px-2 font-semibold">Layout</h2>
  {#if studio.doc}
    {@const { project } = studio.doc}
    {#snippet rack()}<Server size={16} /> Rack <span class="text-muted">{RACKS[project.rack.kind]}</span>{/snippet}
    <ul>
      <li>
        {@render node(null, rack)}
        <ul class="pl-4" ondragover={() => (target = null)} ondrop={drop}>
          {#each project.rows as row, r}
            {@render line(null, r)}
            {#if row.racked}
              <li ondragover={(e) => aim(e, { into: null, before: r }, { into: null, before: r + 1 })}>{@render device(r, 0)}</li>
            {:else}
              <li>
                {#snippet panelName()}<Rows3 size={16} /> Panel {rowNumber(r)} <span class="text-muted">{row.units}U</span> {@render alert(issues(r, null))}{/snippet}
                <div class={['cursor-grab', dragged({ row: r }) && 'opacity-40']}>
                  {@render node({ row: r, device: null }, panelName, { ...handle({ row: r }), ondragover: (e: DragEvent) => aimAt(e, r) })}
                </div>
                <ul class={['mb-1.5 rounded-lg pl-4 transition-colors', target?.into === r && 'bg-accent/8']}>
                  {#each row.devices as _, d}
                    {@render line(r, d)}
                    <li ondragover={(e) => aim(e, { into: r, before: d }, { into: r, before: d + 1 })}>{@render device(r, d)}</li>
                  {/each}
                  {@render line(r, row.devices.length)}
                  <li ondragover={(e) => aim(e, { into: r, before: row.devices.length })}>{@render add(r, 'Add device')}</li>
                </ul>
              </li>
            {/if}
          {/each}
          {@render line(null, project.rows.length)}
          <li ondragover={(e) => aim(e, { into: null, before: project.rows.length })}>
            <button class={item} onclick={() => studio.addRow()}><Plus size={16} /> Add panel</button>
          </li>
          <li>{@render add(null, 'Add rack device')}</li>
        </ul>
      </li>
    </ul>
  {/if}
</nav>

<Library />
