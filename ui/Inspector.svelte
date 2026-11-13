<script lang="ts">
  import { ArrowDown, ArrowLeft, ArrowRight, ArrowUp, Circle, RotateCcw, RotateCw, Ruler, Trash2 } from '@lucide/svelte';
  import { rowNumber, studio } from './studio.svelte.ts';
  import type { Align, RackWidth, Side } from './wasm/snugrack.js';

  /** Quarter turns of a device as it shows in the view, about the rack's x, y and z. */
  const TURNS = [
    { label: 'Turn', axis: 1, ways: [['Turn left', -1], ['Turn right', 1]] },
    { label: 'Tip', axis: 0, ways: [['Tip back', -1], ['Tip forward', 1]] },
    { label: 'Roll', axis: 2, ways: [['Roll left', 1], ['Roll right', -1]] },
  ] as const;
  const SIDES: Record<Side, string> = { ear: 'Rack ear', boltedEar: 'Rack ear, bolted on', joiner: 'Joiner', plain: 'Plain' };
  // Each pictured by the CSS that places its devices the same way.
  const ALIGNS: Record<Align, [string, string, string]> = {
    left: ['Left', 'Packed against the left end', 'justify-start'],
    center: ['Center', 'Packed in the middle', 'justify-center'],
    right: ['Right', 'Packed against the right end', 'justify-end'],
    spread: ['Spread', 'The same room in every gap, the ends included: the most room to work in at each joint', 'justify-evenly'],
  };
  const RACKS: Record<RackWidth['kind'], string> = { inch19: '19″ (EIA-310)', inch10: '10″', fit: 'Fit the devices', custom: 'Custom' };

  let trueLength = $state(0);

  function setRack(kind: RackWidth['kind']) {
    // A custom rack starts from the one we had.
    const panel = studio.design!.panel;
    studio.doc!.project.rack =
      kind === 'custom' ? { kind, panelWidth: panel.width, holeSpacing: panel.holeSpacing, railOpening: panel.railOpening } : { kind };
  }

  // One part of what's selected, under its heading.
  const section = 'border-b border-line py-3 first:pt-0 last:border-0';
  const heading = 'mb-1 font-semibold';
  // A labeled line: label left, value or control right, units lined up after it.
  const field = 'flex min-h-9 items-center justify-between gap-2';
  const unit = 'inline-block w-6 text-muted';
  const number = 'input w-24 text-right';
  // A checkbox that looks like UniFi's switches.
  const knob =
    'relative h-4.5 w-8 cursor-pointer appearance-none rounded-full bg-line transition-colors after:absolute after:top-0.5 after:left-0.5 after:size-3.5 after:rounded-full after:bg-white after:transition-transform checked:bg-accent checked:after:translate-x-3.5';
</script>

<!-- Not bound: an emptied field would hand the core a null, so it keeps its last number until it has a new one. -->
{#snippet num(
  label: string,
  obj: Record<string, any>,
  key: string,
  step: number,
  { hint, suffix = 'mm', onchange, min = 0 }: { hint?: string; suffix?: string; onchange?: () => void; min?: number | null } = {},
)}
  <label class={field} title={hint}>
    {label}
    <span>
      <input
        class={number}
        type="number"
        {step}
        {min}
        value={obj[key]}
        oninput={(e) => e.currentTarget.valueAsNumber >= (min ?? -Infinity) && (obj[key] = e.currentTarget.valueAsNumber)}
        {onchange}
      />
      <span class={unit}>{suffix}</span>
    </span>
  </label>
{/snippet}

{#snippet toggle(label: string, obj: Record<string, any>, key: string, hint = '', onchange?: () => void)}
  <label class={field} title={hint}>
    {label}
    <input class={knob} type="checkbox" bind:checked={obj[key]} {onchange} />
  </label>
{/snippet}

{#snippet fact(label: string, value: string, suffix = '', hint = '')}
  <p class={field} title={hint}>{label} <span>{value} {#if suffix}<span class={unit}>{suffix}</span>{/if}</span></p>
{/snippet}

<!-- What to do with what's selected; a panel, or a device on its own rack ears, at `r` moves too. -->
{#snippet actions(remove: string, r?: number, d?: number)}
  <div class="mt-3 flex flex-wrap gap-2">
    {#if r !== undefined && d !== undefined}
      <button class="btn" disabled={d === 0} onclick={() => studio.move({ row: r, device: d }, { row: r, device: d - 1 })}><ArrowLeft size={16} /> Move left</button>
      <button class="btn" disabled={d === studio.doc!.project.rows[r].devices.length - 1} onclick={() => studio.move({ row: r, device: d }, { row: r, device: d + 2 })}
        ><ArrowRight size={16} /> Move right</button
      >
    {:else if r !== undefined}
      <button class="btn" disabled={r === 0} onclick={() => studio.moveRow(r, r - 1)}><ArrowUp size={16} /> Move up</button>
      <button class="btn" disabled={r === studio.doc!.project.rows.length - 1} onclick={() => studio.moveRow(r, r + 2)}><ArrowDown size={16} /> Move down</button>
    {/if}
    <button class="btn" onclick={() => studio.remove(studio.selection!)}><Trash2 size={16} /> {remove}</button>
  </div>
{/snippet}

<!-- Whatever is selected: a hole, a device, a panel, or the rack. -->
<aside class="overflow-y-auto border-line bg-surface p-4 max-xl:row-start-3 max-xl:border-t xl:border-l" aria-label="Properties">
  {#if studio.doc}
    {@const { project } = studio.doc}
    {@const s = studio.selection}
    {@const row = s && project.rows[s.row]}
    {@const d = s?.device}
    {@const hole = s?.hole === undefined || d == null ? undefined : studio.holes({ row: s.row, device: d })[s.hole]}
    {@const id = d == null ? undefined : row?.devices[d]}
    {@const model = id ? studio.doc.models[id] : undefined}
    {#if s && d != null && hole}
      <section class={section}>
        <h2 class={heading}>Hole {s.hole! + 1}</h2>
        <p class="mb-1 text-muted">For {model?.name ?? 'its device'}: it goes wherever the device does. Drag it in the view to move it.</p>
        {@render num('X', hole, 'x', 0.5, { hint: 'From the device’s middle to the hole’s center, to the right; to the left is negative', min: null })}
        {@render num('Y', hole, 'y', 0.5, { hint: 'From the device’s bottom up to the hole’s center', min: null })}
        {@render num('Diameter', hole, 'diameter', 0.5, { hint: 'It cuts straight through the faceplate, whatever is there', min: 1 })}
      </section>
      {@render actions('Remove hole')}
    {:else if s && row && d != null && id}
      {@const report = studio.design?.rows[s.row]?.devices[d]}
      {@const units = studio.design?.rows[s.row]?.units}
      <section class={section}>
        <h2 class={heading}>{row.racked ? 'Rack device' : `Device in panel ${rowNumber(s.row)}`}</h2>
        {#if !model}
          <p class="text-muted">Its model is gone. Remove it and import the model again.</p>
        {:else}
          <label class={field}>Name <input class="input" bind:value={model.name} /></label>
          {#if model.box}
            {#each ['Width', 'Height', 'Depth'] as label, k (label)}
              {@render num(label, model.box, String(k), 0.5, { onchange: () => studio.reshape(id) })}
            {/each}
          {:else}
            <p class="text-muted">Turn the device until its front faces you and its top is up.</p>
            {#each TURNS as { label, axis, ways } (label)}
              <div class={field}>
                {label}
                <span class="flex gap-1">
                  {#each ways as [name, by], k (name)}
                    {@const Icon = k ? RotateCw : RotateCcw}
                    <button class="btn btn-quiet" aria-label={name} title={name} onclick={() => studio.turn(id, axis, by)}><Icon size={16} /></button>
                  {/each}
                </span>
              </div>
            {/each}
            {@render num('Scale', model, 'scale', 0.001, { hint: 'For a model that comes in the wrong size', suffix: '×', onchange: () => studio.reshape(id) })}
          {/if}
        {/if}
      </section>

      {#if model && !model.box}
        {@const measured = studio.measured?.row === s.row && studio.measured.device === d ? studio.measured.mm : null}
        <section class={section}>
          <h2 class={heading}>Calibrate</h2>
          {#if measured === null}
            <p class="text-muted">Use <Ruler size={14} class="inline" /> Measure in the view to measure a feature of known length, like the width of a port.</p>
          {:else}
            {@render fact('Measured', measured.toFixed(1), 'mm')}
            <form
              class={field}
              onsubmit={(e) => {
                e.preventDefault();
                model.scale *= trueLength / measured;
                studio.measured = null;
                studio.reshape(id);
              }}
            >
              It is really
              <span><input class={number} type="number" min="0.1" step="0.1" required bind:value={trueLength} /> <span class={unit}>mm</span></span>
            </form>
            <p class="text-muted">Press Enter to rescale the model.</p>
          {/if}
        </section>
      {/if}

      <section class={section}>
        <h2 class={heading}>Mount</h2>
        <!-- What dragging it in the layout does, for the keyboard too. -->
        <label class={field}>
          Place
          <select
            class="input"
            value={row.racked ? 'rack' : String(s.row)}
            onchange={(e) => {
              const to = e.currentTarget.value;
              const from = { row: s.row, device: d };
              if (to === 'rack') studio.rack(from, s.row + 1);
              else studio.move(from, { row: Number(to), device: project.rows[Number(to)].devices.length });
            }}
          >
            {#each project.rows as other, r (r)}
              {#if !other.racked}<option value={String(r)}>In panel {rowNumber(r)}</option>{/if}
            {/each}
            <option value="rack">On its own rack ears</option>
          </select>
        </label>
        {#if row.racked}
          {@render fact('Height', String(units ?? '—'), units ? 'U' : '', 'It comes with its own rack ears, takes as many units as it’s tall, and has nothing printed')}
        {/if}
        {#if report}
          {@render fact('Size', report.size.map((v) => v.toFixed(1)).join(' × '), 'mm')}
          {#if !row.racked}
            {@render fact('Held by', report.retention.map((v) => v.toFixed(1)).join(' / '), 'mm', "How far the faceplate overlaps the device's outline, sideways and vertically")}
            {@render fact('Front', report.lip ? 'Behind a lip' : 'Flush')}
          {/if}
        {:else}
          <p class="text-muted">—</p>
        {/if}
      </section>
      {#if !row.racked}
        <section class={section}>
          <h2 class={heading}>Cable holes</h2>
          <p class="text-muted">Round holes for its cables, straight through the faceplate. Use <Circle size={14} class="inline" /> Hole in the view to put one where you click, and drag it to move it.</p>
          <button
            class="btn mt-2"
            title="Just right of the device, halfway up, to move from there"
            disabled={!report}
            onclick={() => studio.addHole({ row: s.row, device: d }, report!.size[0] / 2 + 10, report!.size[1] / 2)}><Circle size={16} /> Add hole</button
          >
        </section>
      {/if}
      {#if row.racked}
        {@render actions('Remove from rack', s.row)}
      {:else}
        {@render actions('Remove from panel', s.row, d)}
      {/if}
    {:else if s && row}
      {@const used = studio.design?.rows[s.row]}
      <section class={section}>
        <h2 class={heading}>Panel {rowNumber(s.row)}</h2>
        {@render num('Height', row, 'units', 1, { hint: 'Rack units this panel takes', suffix: 'U' })}
        {#each [['Left end', 'left'], ['Right end', 'right']] as const as [label, key] (key)}
          <label class={field}>
            {label}
            <select class="input" bind:value={row[key]}>
              {#each Object.entries(SIDES) as [value, text] (value)}<option {value}>{text}</option>{/each}
            </select>
          </label>
        {/each}
        <fieldset class="py-1">
          <legend class="mb-1">Devices</legend>
          <div class="grid grid-cols-4 gap-1">
            {#each Object.entries(ALIGNS) as [value, [label, hint, justify]] (value)}
              <label
                class="flex cursor-pointer flex-col items-center gap-1 rounded-lg border border-line bg-raised p-1.5 text-muted has-checked:border-accent has-checked:text-ink has-focus-visible:outline-2 has-focus-visible:outline-offset-1 has-focus-visible:outline-accent"
                title={hint}
              >
                <input class="sr-only" type="radio" name="align" {value} bind:group={row.align} />
                <span class={['flex h-4 w-full gap-0.5 border-x-2 border-current px-0.5', justify]} aria-hidden="true">
                  <span class="w-3 rounded-xs bg-current opacity-60"></span><span class="w-3 rounded-xs bg-current opacity-60"></span>
                </span>
                {label}
              </label>
            {/each}
          </div>
        </fieldset>
        {#if used}
          {@render fact('Width', `${used.width.toFixed(0)} of ${used.room.toFixed(0)}`, 'mm', 'What its devices take, collars and joints included, of what the panel has between the ears or the rails')}
          {#if used.gaps.length}
            {@render fact('Gaps', used.gaps.map((g) => g.toFixed(0)).join(' · '), 'mm', 'What’s left of the panel, from left to right: before the first device, between each two, after the last')}
          {/if}
        {/if}
      </section>
      {@render actions('Remove panel', s.row)}
    {:else}
      <section class={section}>
        <h2 class={heading}>Rack</h2>
        <label class={field}>
          Width
          <select class="input" value={project.rack.kind} disabled={!studio.design} onchange={(e) => setRack(e.currentTarget.value as RackWidth['kind'])}>
            {#each Object.entries(RACKS) as [value, label] (value)}<option {value}>{label}</option>{/each}
          </select>
        </label>
        {#if project.rack.kind === 'fit' && studio.design}
          {@render fact('Panel', studio.design.panel.width.toFixed(1), 'mm', 'As wide as the widest panel needs, with ears like a 19″ rack’s; a standard rack for devices on their own rack ears')}
          {@render fact('Rail opening', studio.design.panel.railOpening.toFixed(1), 'mm')}
        {/if}
        {#if project.rack.kind === 'custom'}
          {@render num('Panel width', project.rack, 'panelWidth', 0.1)}
          {@render num('Hole spacing', project.rack, 'holeSpacing', 0.1, { hint: 'Between the left and right hole centers' })}
          {@render num('Rail opening', project.rack, 'railOpening', 0.1, { hint: 'Clear width between the rails' })}
        {/if}
        {@render toggle('Separate pieces', project.build, 'split', 'Print each device’s share of a panel on its own, bolted to its neighbors with M3 screws, for smaller printer beds. Ears join the end pieces or bolt on, per panel.')}
        {@render fact('Parts', String(studio.design?.meshes.length ?? 0))}
        {@render fact('Filament', studio.grams.toFixed(0), 'g')}
      </section>
      <section class={section}>
        <h2 class={heading}>Build</h2>
        {@render num('Faceplate', project.build, 'plate', 0.2, { hint: 'Faceplate thickness' })}
        {@render num('Walls', project.build, 'wall', 0.2, { hint: 'Cage and collar walls; a multiple of your line width prints best' })}
        {@render num('Collar depth', project.build, 'collarDepth', 1, { hint: 'How deep the cage plugs into the faceplate' })}
        {@render num('Cage back', project.build, 'back', 0.5, { hint: 'How deep the cage’s back is: the struts the device’s back rests on, and the walls round them' })}
        {@render toggle('Open cage walls', project.build, 'vents', 'Big openings: less filament, more air')}
        {@render num('Air under device', project.build, 'air', 0.5, { hint: 'For vents in its bottom: the device rests on ledges along the cage sides; 0 lays it on the floor' })}
      </section>
      <section class={section}>
        <h2 class={heading}>Fit</h2>
        <p class="mb-1 text-muted">Gaps between parts. Tune them after a test print.</p>
        {@render num('Nest', project.fit, 'nest', 0.05, { hint: 'Between the device and the faceplate' })}
        {@render num('Cage', project.fit, 'cage', 0.05, { hint: 'Between the device and the cage' })}
        {@render num('Collar', project.fit, 'collar', 0.05, { hint: 'Between the cage and the collar' })}
        {@render num('Hook', project.fit, 'hook', 0.1, { hint: 'How far the latch’s nubs reach into the cage’s 1.2 mm tongues; more makes the tongues longer and the cage stand taller where they are, and past 1.2 holds no better' })}
        {@render num('Latch play', project.fit, 'play', 0.05, { hint: 'Front-to-back play once latched' })}
      </section>
    {/if}
  {/if}
</aside>
