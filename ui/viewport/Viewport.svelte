<script lang="ts">
  import { Circle, LocateFixed, Ruler, TriangleAlert, X } from '@lucide/svelte';
  import { Viewer, type Layer, type Section, type Tool, type ViewName } from './viewer.ts';
  import type { Measurement } from './measure.ts';
  import { describe, rowNumber, studio } from '../studio.svelte.ts';

  const VIEWS: Record<ViewName, string> = { iso: 'Iso', front: 'Front', top: 'Top', side: 'Side', back: 'Back' };
  const LAYERS: Record<Layer, string> = { device: 'Devices', faceplate: 'Faceplates', cage: 'Cages' };
  const CUTS: Record<Section['axis'], string> = { x: 'Side cut', y: 'Plan cut', z: 'Depth cut' };
  const TOOLS = [
    { tool: 'measure', Icon: Ruler, label: 'Measure', hint: 'Measure between points, edges and faces' },
    { tool: 'hole', Icon: Circle, label: 'Hole', hint: 'Put a hole for a cable straight through a faceplate, where you click' },
  ] as const;

  /** For the view buttons; the viewer itself follows the app from inside `attach`. */
  let viewer: Viewer | undefined;
  let hidden = $state.raw<Layer[]>([]);
  let explode = $state(0);
  let cut = $state<'' | Section['axis']>('');
  let at = $state(0.5);
  let tool = $state<Tool>(null);
  let measured = $state.raw<Measurement>({ picked: 0, mm: null, degrees: null, device: null });
  // Warnings the user has put away, by what they say; one that comes back changed shows again.
  let dismissed = $state.raw<string[]>([]);
  const warnings = $derived((studio.design?.warnings ?? []).map((w) => ({ ...w, key: JSON.stringify(w) })).filter((w) => !dismissed.includes(w.key)));
  // We frame the rack when its parts and devices change in number, not when a setting reshapes them.
  let framed = 0;

  const instances = $derived.by(() => {
    const { design, scenes } = studio;
    if (!design) return [];
    return design.rows.flatMap((report, r) =>
      report.devices.flatMap((placed, d) => {
        const object = scenes[studio.designed(r, d) ?? ''];
        return placed && object ? [{ model: object, matrix: placed.matrix, place: { row: r, device: d } }] : [];
      }),
    );
  });

  // In the rack frame, from where each device's own frame is. Where they are is the project's as it is, so a hole
  // moved stays put while the next design runs.
  const holes = $derived(
    (studio.design?.rows ?? []).flatMap((report, row) =>
      report.devices.flatMap((placed, device) =>
        placed ? (studio.doc?.project.holes ?? []).filter((h) => h.model === studio.designed(row, device)).map((h, hole) => ({ ...h, row, device, hole, origin: placed.origin, x: placed.origin[0] + h.x, y: placed.origin[1] + h.y })) : [],
      ),
    ),
  );

  /**
   * A hole at a spot in the rack, for the device at `at`, or with none given, the nearest one in its row; none for a
   * device on its own rack ears, which has nothing printed to go through.
   */
  function addHole(at: { row: number; device: number | null }, x: number, y: number) {
    // Only on the design of the project as it is, or the place it gives could hold another device by now.
    const placed = studio.current && !studio.doc!.project.rows[at.row].racked ? studio.design!.rows[at.row].devices : [];
    const near = (d: number) => (placed[d] ? Math.abs(placed[d]!.origin[0] - x) : Infinity);
    const device = at.device ?? placed.reduce((best, _, d) => (near(d) < near(best) ? d : best), 0);
    const o = placed[device]?.origin;
    if (o) studio.addHole({ row: at.row, device }, x - o[0], y - o[1]);
  }

  // The viewer lives as long as its element; these effects keep it in step with the app.
  function attach(el: HTMLElement) {
    const v = (viewer = new Viewer(el, {
      pick: (pick) => (studio.selection = pick),
      measure: (m) => {
        measured = m;
        // A length on one device can calibrate its model, unless it's a box: we show that device.
        const model = m.device && m.mm !== null ? studio.model(m.device.row, m.device.device) : undefined;
        studio.measured = model && !model.box ? { ...m.device!, mm: m.mm! } : null;
        if (studio.measured) studio.selection = { ...m.device! };
      },
      hole: addHole,
      move: (hole, x, y) => {
        if (studio.current) studio.moveHole(hole, hole.hole, x - hole.origin[0], y - hole.origin[1]);
      },
    }));
    $effect(() => v.setDevices(instances));
    $effect(() => {
      const meshes = studio.design?.meshes;
      if (meshes) v.setParts(meshes);
    });
    // Last, so the view takes in both: a device with its own rack ears adds a model and no parts.
    $effect(() => {
      const count = (studio.design?.meshes.length ?? 0) + instances.length;
      if (count !== framed) {
        v.setView('iso');
        framed = count;
      }
    });
    $effect(() => v.set({ hidden: new Set(hidden), explode, tool, holes, section: cut ? { axis: cut, at } : null, selected: studio.selection, warned: studio.design?.warnings ?? [] }));
    return () => v.dispose();
  }

  // Not while the rack is still opening.
  const empty = $derived(studio.doc?.project.rows.every((r) => !r.devices.length) ?? false);
  // Floating bars over the 3D view.
  const overlay = 'flex flex-wrap items-center gap-1 rounded-lg border border-line bg-surface/90 p-1 backdrop-blur';
  const toggle = (layer: Layer) => (hidden = hidden.includes(layer) ? hidden.filter((l) => l !== layer) : [...hidden, layer]);
</script>

<!-- Escape puts the tool away. -->
<svelte:window onkeydown={(e) => e.key === 'Escape' && (tool = null)} />

<main class="relative overflow-hidden bg-radial-[circle_at_50%_40%] from-accent/6 to-transparent to-60% max-xl:col-span-2 max-xl:row-start-2">
  <div class={['absolute inset-0', tool && 'cursor-crosshair']} role="img" aria-label="3D view of the rack" {@attach attach}></div>

  {#if empty}
    <div class="pointer-events-none absolute inset-0 grid place-content-center text-center">
      <h2 class="text-base font-semibold">Your rack is empty</h2>
      <p class="mt-1 text-muted">Add a device to a panel, or one on its own rack ears to the rack: import its 3D model (GLB or STL),<br />or describe it as a box. Everything runs in your browser.</p>
    </div>
  {/if}

  <!-- The bars wrap, rather than overlap, when the view gets narrow. -->
  <div class="pointer-events-none absolute inset-x-3 top-3 flex flex-wrap items-start justify-between gap-2 *:pointer-events-auto">
    <div class={overlay}>
      {#each Object.entries(VIEWS) as [view, label] (view)}
        <button class="btn btn-quiet" onclick={() => viewer?.setView(view as ViewName)}>{label}</button>
      {/each}
    </div>
    <div class="flex flex-col items-end gap-2">
      <div class={overlay}>
        {#each Object.entries(LAYERS) as [layer, label] (layer)}
          <button class="btn btn-quiet" aria-pressed={!hidden.includes(layer as Layer)} onclick={() => toggle(layer as Layer)}>{label}</button>
        {/each}
        <span class="mx-1 h-5 w-px bg-line"></span>
        {#each TOOLS as { tool: t, Icon, label, hint } (t)}
          <button class="btn btn-quiet" aria-pressed={tool === t} onclick={() => (tool = tool === t ? null : t)} title={hint}><Icon size={16} /> {label}</button>
        {/each}
      </div>
      {#if tool === 'hole'}
        <div class={[overlay, 'px-3 py-2 text-muted']} role="status">
          {hidden.includes('faceplate') ? 'Show the faceplates to put a hole through one.' : 'Click a faceplate where the hole goes: it’s for the nearest device. Drag a hole to move it.'}
        </div>
      {:else if tool === 'measure'}
        <div class={[overlay, 'px-3 py-2 text-muted']} role="status">
          {#if measured.picked === 0}
            Click a point, an edge or a face. Hold Shift for any spot on a surface.
          {:else if measured.picked === 1}
            Click a second one to measure to.
          {:else}
            <span class="text-ink">{[measured.mm !== null && `${measured.mm.toFixed(2)} mm`, measured.degrees !== null && `${measured.degrees.toFixed(1)}°`].filter(Boolean).join(' · ')}</span>
            {#if measured.mm === null}· no distance unless they're parallel{/if}
            {#if studio.measured}· set its true length under Calibrate{/if}
          {/if}
        </div>
      {/if}
    </div>
  </div>

  <div class="pointer-events-none absolute inset-x-3 bottom-3 flex flex-col gap-2 *:pointer-events-auto">
    {#if warnings.length}
      <ul class={[overlay, 'max-w-2xl flex-col items-stretch self-start']} aria-label="Problems">
        {#each warnings as w (w.key)}
          <!-- Text to read and copy; selecting what it's about is a button of its own. -->
          <li class="flex items-start">
            <p class="flex grow gap-1.5 px-2 py-1.5">
              <TriangleAlert size={16} class="mt-0.5 shrink-0 text-warn" />
              <span>{[!studio.doc?.project.rows[w.row]?.racked && `Panel ${rowNumber(w.row)}`, w.device !== null && (studio.doc?.models[studio.designed(w.row, w.device) ?? '']?.name ?? 'device')].filter(Boolean).join(', ')}: {describe(w.issue)}</span>
            </p>
            <button class="btn btn-quiet shrink-0" aria-label="Select it" title="Select it" onclick={() => (studio.selection = { row: w.row, device: w.device })}><LocateFixed size={16} /></button>
            <button class="btn btn-quiet shrink-0" aria-label="Dismiss" title="Dismiss" onclick={() => (dismissed = [...dismissed, w.key])}><X size={16} /></button>
          </li>
        {/each}
      </ul>
    {/if}
    <div class={[overlay, 'flex-wrap justify-center gap-3 self-center pr-3']}>
      <select class="input" bind:value={cut} aria-label="Section">
        <option value="">No section</option>
        {#each Object.entries(CUTS) as [axis, label] (axis)}<option value={axis}>{label}</option>{/each}
      </select>
      <input class="accent-accent" type="range" min="0" max="1" step="0.001" bind:value={at} disabled={!cut} aria-label="Section position" />
      <label class="flex items-center gap-2 text-muted" title="Pull the parts apart: first faceplates forward and cages back, then each panel's pieces apart at their joints">Explode <input class="accent-accent" type="range" min="0" max="1" step="0.01" bind:value={explode} /></label>
    </div>
  </div>
</main>
