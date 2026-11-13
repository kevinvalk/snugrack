// End to end in a real browser: build a two-panel rack through the UI, with two devices side by side
// in the first panel and a hole through the second, export every part and check each STL prints well: a
// watertight surface, little overhang in the orientation it comes in, and its weight. Screenshots of
// the views we care about land in test-results/ for a human to look at. A generated stand-in plays the
// device; your own models get a rack of their own, one per panel:
//
//   npm run e2e
//   SNUGRACK_MODELS=fixtures/a.glb,fixtures/b.stl npm run e2e

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, join } from 'node:path';
import { after, test } from 'node:test';
import { unzipSync } from 'fflate';
import { chromium, type Page } from 'playwright';
import { preview } from 'vite';

const server = await preview({ build: { outDir: 'target/e2e/dist' }, preview: { port: 4174, strictPort: true }, logLevel: 'silent' });
const browser = await chromium.launch();
// What an earlier run left would pass for this one's.
await rm('test-results', { recursive: true, force: true });
await mkdir('test-results', { recursive: true });
/** PrusaSlicer, if it's here: set PRUSASLICER to point at it elsewhere. */
const SLICER = process.env.PRUSASLICER ?? '/Applications/PrusaSlicer.app/Contents/MacOS/PrusaSlicer';
const slicerData = existsSync(SLICER) ? await mkdtemp(join(tmpdir(), 'snugrack-slicer-')) : null;
after(async () => {
  await browser.close();
  await new Promise((done) => server.httpServer.close(done));
  if (slicerData) await rm(slicerData, { recursive: true });
});

test('a two-panel rack exports closed parts', { timeout: 180_000 }, async () => {
  const errors: string[] = [];
  const page = await open((page) => {
    page.on('pageerror', (e) => errors.push(e.message));
    page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  });
  const nav = page.getByRole('navigation');
  const view = page.getByRole('img', { name: '3D view of the rack' });
  const panel = page.getByRole('complementary', { name: 'Properties' });
  // Next to the logo, the version: when the commit it was built from was made.
  assert.match((await page.locator('header').getByTitle(/^Version/).textContent())!, /^\d{4}\.\d{2}\.\d{2}\.\d{4}$/);
  // The walls and air this test is measured for, whatever the defaults are.
  await designed(page, () => panel.getByLabel(/^Walls/).fill('1.6'));
  await designed(page, () => panel.getByLabel('Air under device').fill('2'));

  // A file that isn't a model says so until dismissed, with nothing to try again: the design is fine.
  await page.getByRole('button', { name: 'Add device' }).click();
  await page.locator('#add-0 input[type=file]').setInputFiles({ name: 'broken.glb', mimeType: 'model/gltf-binary', buffer: Buffer.from('not a model') });
  const status = page.locator('[data-designs]');
  await status.getByText('Couldn’t read broken.glb').waitFor({ timeout: 5_000 });
  assert.equal(await status.getByRole('button', { name: 'Try again' }).count(), 0);
  await status.getByRole('button', { name: 'Dismiss' }).click();

  // Panel 1: an imported model.
  await importModel(page, 0, { name: 'device.glb', mimeType: 'model/gltf-binary', buffer: standInDevice() });

  // Next to it, and alone in panel 2: boxes, which are box-shaped at the front and so sit behind a lip.
  await addBox(page, 0);
  await designed(page, () => page.getByRole('button', { name: 'Add panel' }).click());
  await addBox(page, 1);
  await page.getByText('Behind a lip').waitFor({ timeout: 5_000 });

  // With the Hole tool, a click on a faceplate puts a hole there for the nearest device, straight through it: here in
  // panel 2's left ear, for its box.
  await nav.getByRole('button', { name: /^Rack/ }).click();
  await page.getByRole('button', { name: 'Front' }).click();
  const { width, height } = (await view.boundingBox())!;
  const ear = async () => inspect(await download(page, /^Panel 2 · Left ear/));
  const before = await ear();
  await page.getByRole('button', { name: 'Hole', exact: true }).click();
  // A right click pans, and puts no hole.
  await view.click({ position: { x: width * 0.3, y: height * 0.53 }, button: 'right' });
  assert.equal(await nav.getByRole('button', { name: /^Hole/ }).count(), 0);
  await designed(page, () => view.click({ position: { x: width * 0.25, y: height * 0.53 } }));
  await panel.getByRole('heading', { name: 'Hole 1' }).waitFor({ timeout: 5_000 });
  await page.getByRole('button', { name: 'Hole', exact: true }).click();
  const holed = await ear();
  // Lighter by a 10 mm hole through the 4 mm plate, give or take its facets, and closed still.
  const hole = (Math.PI * 5 ** 2 * 4 * 1.25) / 1000; // grams of PLA
  assert.ok(before.grams - holed.grams > 0.95 * hole && holed.leaks === 0, `${before.grams} g, then ${holed.grams} g`);
  // Dragged, it goes where it's let go, and only then designs again.
  const x = Number(await panel.getByLabel('X').inputValue());
  const [from, to] = [{ x: width * 0.25, y: height * 0.53 }, { x: width * 0.25 + 12, y: height * 0.53 }];
  const box = (await view.boundingBox())!;
  await page.mouse.move(box.x + from.x, box.y + from.y);
  await page.mouse.down();
  await page.mouse.move(box.x + to.x, box.y + to.y, { steps: 4 });
  assert.equal(Number(await panel.getByLabel('X').inputValue()), x);
  await designed(page, () => page.mouse.up());
  const moved = Number(await panel.getByLabel('X').inputValue());
  assert.ok(moved > x + 2, `${x} to ${moved}`);
  await panel.getByRole('heading', { name: 'Hole 1' }).waitFor({ timeout: 5_000 });
  await page.screenshot({ path: 'test-results/hole.png' });

  // A click on nothing selects the rack.
  await view.click({ position: { x: width * 0.98, y: height * 0.5 } });
  assert.equal(oneLine(await nav.locator('[aria-current=true]').textContent()), 'Rack 19″');

  // New panels spread their devices out, the same room in every gap; packed to the left, the gaps
  // between the devices close and the spare width goes to the right end.
  await nav.getByRole('button', { name: /^Panel 1/ }).click();
  const gaps = async () => (await page.getByText(/^Gaps/).textContent())!.match(/[\d.]+/g)!.map(Number);
  const spread = await gaps();
  assert.ok(spread.length === 3 && spread.every((g) => g === spread[0] && g > 0), spread.join(' · '));
  await designed(page, () => page.getByText('Left', { exact: true }).click());
  const packed = await gaps();
  assert.ok(packed[0] === 0 && packed[1] === 0 && packed[2] > spread[2], packed.join(' · '));
  await panel.screenshot({ path: 'test-results/panel-left.png' });
  await designed(page, () => page.getByText('Spread', { exact: true }).click());
  assert.deepEqual(await gaps(), spread);
  await panel.screenshot({ path: 'test-results/panel-spread.png' });

  // An emptied number field keeps its last value while the user retypes, so the design still works.
  await nav.getByRole('button', { name: /^Rack/ }).click();
  await panel.screenshot({ path: 'test-results/rack-panel.png' });
  await page.getByLabel('Nest').fill('');
  await designed(page, () => page.getByRole('button', { name: 'Design again' }).click());

  // The rack's name is the page's title.
  await page.getByLabel('Rack name').fill('Lab rack');
  assert.equal(await page.title(), 'Lab rack · SnugRack');

  await page.getByRole('button', { name: 'Iso' }).click();
  await page.screenshot({ path: 'test-results/rack.png' });
  await headers(page, 'header');
  await measure(page, 'measure');

  const files = await exportParts(page);
  // Per device a faceplate piece and a cage, per panel two ears.
  assert.equal(Object.keys(files).length, 10, Object.keys(files).join(', '));

  // Views a human should look at: the latch from behind, the parts slid off their devices and then
  // the joints between the pieces of a panel apart, and sections through the nest and cage.
  await page.getByRole('button', { name: 'Back' }).click();
  for (const [explode, name] of [['0.66', 'exploded-out'], ['1', 'exploded']]) {
    await page.getByLabel('Explode').fill(explode);
    await page.screenshot({ path: `test-results/${name}.png` });
  }
  await page.getByRole('button', { name: 'Top' }).click();
  await page.screenshot({ path: 'test-results/exploded-top.png' });
  await page.getByLabel('Explode').fill('0');
  for (const [cut, angle, at, name] of [
    ['Plan cut', 'Top', '0.75', 'plan-section'],
    ['Side cut', 'Side', '0.4', 'side-section'],
  ]) {
    await page.getByLabel('Section', { exact: true }).selectOption({ label: cut });
    await page.getByLabel('Section position').fill(at);
    await page.getByRole('button', { name: angle, exact: true }).click();
    await page.screenshot({ path: `test-results/${name}.png` });
  }
  // A click picks what the section leaves in view, not the box it cut away in front of it.
  await view.click({ position: { x: width * 0.45, y: height * 0.46 } });
  assert.equal(oneLine(await nav.locator('[aria-current=true]').textContent()), 'device');

  // A drag that ends outside the layout is over: a drop after it moves nothing, as the layout below shows.
  const device = nav.getByRole('button', { name: 'device', exact: true });
  await device.dispatchEvent('dragstart');
  await device.dispatchEvent('dragend');
  await nav.getByRole('listitem').last().dispatchEvent('drop');

  // The rack survives a reload: the project from localStorage, the model from IndexedDB. A setting
  // saved empty, as an emptied field once could, takes its default again.
  await page.evaluate(() => {
    const doc = JSON.parse(localStorage.getItem('snugrack.doc')!);
    doc.project.fit.hook = null;
    localStorage.setItem('snugrack.doc', JSON.stringify(doc));
  });
  await page.emulateMedia({ colorScheme: 'dark' });
  await page.reload();
  await designed(page);
  assert.equal(await page.title(), 'Lab rack · SnugRack');
  await nav.getByRole('button', { name: /^Rack/ }).click();
  assert.equal(await page.getByLabel('Hook').inputValue(), '1.2');
  const tree = await nav.getByRole('button').allTextContents();
  assert.deepEqual(tree.map(oneLine).filter(Boolean), [
    'Rack 19″',
    'Panel 1 1U',
    'device',
    'Box',
    'Add device',
    'Panel 2 1U',
    'Box',
    'Hole 1 ⌀ 10 mm',
    'Add device',
    'Add panel',
    'Add rack device',
  ]);
  await page.screenshot({ path: 'test-results/rack-dark.png' });
  await headers(page, 'header-dark');

  // Light picked over the system's dark stays picked after a reload, from the first paint; Shift-click follows the
  // system again.
  const background = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  const dark = await background();
  await page.getByRole('button', { name: 'Switch to light mode' }).click();
  const light = await background();
  assert.notEqual(light, dark);
  await page.reload();
  assert.equal(await background(), light);
  await designed(page);
  await page.locator('header').screenshot({ path: 'test-results/header-picked-light.png' });
  await page.getByRole('button', { name: 'Switch to dark mode' }).click({ modifiers: ['Shift'] });
  assert.equal(await background(), dark);
  assert.equal(await page.evaluate(() => localStorage.getItem('snugrack.scheme')), null);

  // Any shape will do as a device: one of the cages we just made, as an STL.
  const cage = Object.entries(files).find(([name]) => name.includes('panel-2-cage'))!;
  await designed(page, () => page.getByRole('button', { name: 'Add panel' }).click());
  await importModel(page, 2, { name: 'cage.stl', mimeType: 'model/stl', buffer: Buffer.from(cage[1]) });
  // It comes in as STLs usually do, Z up, lying on what was its back as printed: tipped forward,
  // its front faces us.
  await designed(page, () => page.getByRole('button', { name: 'Tip forward' }).click());

  // A cage is as tall as a 1U panel allows, so the collar has no room round the latch's tongues.
  const problems = await page.getByRole('list', { name: 'Problems' }).getByRole('listitem').allTextContents();
  assert.deepEqual(problems.map((p) => /^Panel 3.*Not latched: with the latch/.test(p.trim())), [true], problems.join('\n'));
  // Its text can be selected and copied, and a button of its own selects what it's about, whose parts show it.
  const problem = page.getByRole('list', { name: 'Problems' }).getByRole('listitem').first();
  await problem.getByText(/Not latched/).click({ clickCount: 3 });
  assert.match(await page.evaluate(() => getSelection()!.toString()), /Not latched/);
  await problem.getByRole('button', { name: 'Select it' }).click();
  await panel.getByText('Device in panel 3').waitFor({ timeout: 5_000 });
  await page.screenshot({ path: 'test-results/warned.png' });

  // A length measured on one device can calibrate that device, not whichever is selected next.
  await device.click();
  for (const layer of ['Faceplates', 'Cages']) await page.getByRole('button', { name: layer }).click();
  await page.getByRole('button', { name: 'Front' }).click();
  await page.getByRole('button', { name: 'Measure' }).click();
  for (const x of [0.45, 0.55]) await view.click({ position: { x: width * x, y: height / 2 }, modifiers: ['Shift'] });
  await panel.getByText('Measured').waitFor({ timeout: 5_000 });
  await nav.getByRole('button', { name: /^cage/ }).click();
  await panel.getByText('Use Measure').waitFor({ timeout: 5_000 });
  assert.equal(await panel.getByText('Measured').count(), 0);

  // A device on its own rack ears goes in beside the panels, in as many units as it's tall, with nothing printed.
  const parts = (await page.locator('#export button').allTextContents()).length;
  await page.getByRole('button', { name: 'Add rack device' }).click();
  await designed(page, () => page.locator('#add-rack').getByRole('button', { name: 'New box' }).click());
  await panel.getByLabel('Name').fill('Switch');
  for (const [label, size] of [['Width', '440'], ['Height', '43.7'], ['Depth', '250']]) {
    await panel.getByLabel(label).fill(size);
    await designed(page, () => panel.getByLabel(label).press('Tab'));
  }
  await nav.getByRole('button', { name: 'Switch 1U' }).waitFor({ timeout: 5_000 });
  assert.equal((await page.locator('#export button').allTextContents()).length, parts);
  await panel.getByRole('heading', { name: 'Rack device' }).waitFor({ timeout: 5_000 });
  await panel.screenshot({ path: 'test-results/rack-device-panel.png' });
  await page.getByRole('button', { name: 'Iso' }).click();
  await page.screenshot({ path: 'test-results/rack-device.png' });
  // It moves between the panels, which keep their numbers and their parts' names.
  await designed(page, () => panel.getByRole('button', { name: 'Move up' }).click());
  const order = async () => (await nav.getByRole('button', { name: /^(Panel|Switch)/ }).allTextContents()).map(oneLine);
  assert.deepEqual(await order(), ['Panel 1 1U', 'Panel 2 1U', 'Switch 1U', 'Panel 3 1U']);
  assert.ok((await page.locator('#export button').allTextContents()).some((t) => t.startsWith('Panel 3 · Cage')));
  // Dragged into a panel, it's a device in it; dragged out between panels, on its own rack ears again.
  const rackDevice = nav.getByRole('button', { name: /^Switch/ });
  await designed(page, () => rackDevice.dragTo(nav.getByRole('button', { name: 'Add device' }).nth(2)));
  assert.deepEqual(await order(), ['Panel 1 1U', 'Panel 2 1U', 'Panel 3 1U', 'Switch']);
  await designed(page, () => rackDevice.dragTo(nav.getByRole('button', { name: 'Add panel' }), { targetPosition: { x: 20, y: 2 } }));
  assert.deepEqual(await order(), ['Panel 1 1U', 'Panel 2 1U', 'Panel 3 1U', 'Switch 1U']);
  // The keyboard does what dragging does: Place moves it into a panel or out of one, Move left along it, and the arrow
  // keys walk the layout, selecting as they go.
  await rackDevice.click();
  await designed(page, () => panel.getByLabel('Place').selectOption({ label: 'In panel 3' }));
  await designed(page, () => panel.getByRole('button', { name: 'Move left' }).click());
  const devices = async () => (await nav.getByRole('button', { name: /^(cage|Switch)/ }).allTextContents()).map(oneLine);
  assert.deepEqual(await devices(), ['Switch', 'cage']);
  await panel.screenshot({ path: 'test-results/device-panel.png' });
  await rackDevice.focus();
  await page.keyboard.press('ArrowDown');
  assert.equal(oneLine(await nav.locator('[aria-current=true]').textContent()), 'cage');
  await page.keyboard.press('ArrowUp');
  await designed(page, () => panel.getByLabel('Place').selectOption({ label: 'On its own rack ears' }));
  assert.deepEqual(await order(), ['Panel 1 1U', 'Panel 2 1U', 'Panel 3 1U', 'Switch 1U']);
  // The layout removes things too, with the Delete key or the button that shows on hover, and the focus stays in it.
  await nav.getByRole('button', { name: /^Hole 1/ }).focus();
  await designed(page, () => page.keyboard.press('Delete'));
  assert.equal(await nav.getByRole('button', { name: /^Hole/ }).count(), 0);
  await rackDevice.hover();
  await nav.screenshot({ path: 'test-results/layout-remove.png' });
  await designed(page, () => nav.getByRole('button', { name: 'Remove Switch' }).click());
  assert.deepEqual(await order(), ['Panel 1 1U', 'Panel 2 1U', 'Panel 3 1U']);
  assert.ok(await nav.evaluate((el) => el.contains(document.activeElement)));
  assert.deepEqual(errors, []);
  await page.close();
});

test('a device from the library', { timeout: 120_000 }, async () => {
  // We never reach a maker's site: the first download fails, the one tried again gets the stand-in.
  let tries = 0;
  const page = await open((page) => page.route('**/*.glb', (route) => (tries++ ? route.fulfill({ body: standInDevice() }) : route.abort())));
  await page.getByRole('button', { name: 'Add device' }).click();
  await page.getByRole('button', { name: 'From library…' }).click();
  const library = page.getByRole('dialog', { name: 'Device library' });
  await library.getByRole('searchbox').fill('ucg fiber');
  await library.getByRole('button', { name: /^Cloud Gateway Fiber/ }).click();
  await library.getByText('Couldn’t download UCG-Fiber').waitFor({ timeout: 5_000 });
  await library.screenshot({ path: 'test-results/library.png' });
  await designed(page, () => library.getByRole('button', { name: 'Try again' }).click());
  assert.ok(await library.isHidden());
  await page.getByRole('navigation').getByRole('button', { name: 'UCG-Fiber', exact: true }).waitFor({ timeout: 5_000 });
  assert.equal(tries, 2);
  // It faces the way the library says, as the UCG-Fiber's model does: the stand-in's 100 mm side is its front now.
  await page.getByText(/^Size/).getByText('100.0 × 30.0 × 150.0').waitFor({ timeout: 5_000 });
  await page.close();
});

test('a power supply behind a blank faceplate', { timeout: 120_000 }, async () => {
  const page = await open();
  await addBox(page, 0);
  await designed(page, () => page.getByLabel('Blank faceplate').check());
  await page.getByText(/^Front/).getByText('Behind a blank faceplate').waitFor({ timeout: 5_000 });
  assert.equal(await page.getByLabel('Cable room').inputValue(), '30');
  assert.ok(await page.getByText(/^Held by/).isHidden());
  await exportParts(page);
  // Seen from the side, cut through the middle: the pillars from the plate to the device, and the openings in front of it.
  await page.getByLabel('Section', { exact: true }).selectOption({ label: 'Side cut' });
  await page.getByLabel('Section position').fill('0.3');
  await page.getByRole('button', { name: 'Side', exact: true }).click();
  await page.screenshot({ path: 'test-results/blank-section.png' });
  await page.getByLabel('Section', { exact: true }).selectOption({ label: 'No section' });
  await page.getByRole('button', { name: 'Iso' }).click();
  await page.screenshot({ path: 'test-results/blank.png' });
  await page.close();
});

test('edges round after the fact, and can stay sharp', { timeout: 120_000 }, async () => {
  const page = await open();
  await addBox(page, 0);
  const cage = async () => inspect(await download(page, /· Cage/));
  const round = await cage();
  await page.getByRole('navigation').getByRole('button', { name: /^Rack/ }).click();
  await designed(page, () => page.getByLabel('Round edges').uncheck());
  const sharp = await cage();
  // Rounding only takes material off the corners that stand out, and takes off most of what's sharp.
  assert.ok(round.leaks === 0 && sharp.leaks === 0);
  assert.ok(round.grams < sharp.grams, `${round.grams} g rounded, ${sharp.grams} g sharp`);
  assert.ok(round.sharp < sharp.sharp / 2, `${round.sharp.toFixed(0)} mm sharp rounded, ${sharp.sharp.toFixed(0)} mm without`);
  await page.close();
});

test('a device with its own settings', { timeout: 120_000 }, async () => {
  const page = await open();
  await addBox(page, 0);
  const air = page.getByLabel('Air under device').first();
  const project = async () => (await page.evaluate(() => JSON.parse(localStorage.getItem('snugrack.doc')!))).project;
  // Empty, showing the project's, until it has its own.
  assert.equal(await air.inputValue(), '');
  assert.equal(await air.getAttribute('placeholder'), String((await project()).build.air));
  await designed(page, () => air.fill('0'));
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('snugrack.doc')!).project.overrides.length === 1);
  assert.deepEqual((await project()).overrides.map((o: { air: number }) => o.air), [0]);
  // Its walls closed too; with no air of its own any more, it still has that.
  await designed(page, () => page.getByLabel('Open cage walls').selectOption({ label: 'No' }));
  await designed(page, () => air.fill(''));
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('snugrack.doc')!).project.overrides[0]?.air === null);
  assert.deepEqual((await project()).overrides.map((o: { vents: boolean }) => o.vents), [false]);
  await page.screenshot({ path: 'test-results/own-settings.png' });
  await designed(page, () => page.getByLabel('Open cage walls').selectOption({ index: 0 }));
  await page.waitForFunction(() => JSON.parse(localStorage.getItem('snugrack.doc')!).project.overrides.length === 0);
  await page.close();
});

test('real models', { skip: !process.env.SNUGRACK_MODELS, timeout: 600_000 }, async () => {
  const page = await open();
  const models = process.env.SNUGRACK_MODELS!.split(',');
  for (const [panel, path] of models.entries()) {
    if (panel > 0) await designed(page, () => page.getByRole('button', { name: 'Add panel' }).click());
    const started = performance.now();
    await importModel(page, panel, path);
    console.log(`${path}: designed in ${((performance.now() - started) / 1000).toFixed(1)} s`);
  }
  for (const issue of await page.getByRole('list', { name: 'Problems' }).getByRole('listitem').allTextContents()) console.log(`warning: ${issue.trim()}`);
  await page.getByRole('button', { name: 'Iso' }).click();
  await page.screenshot({ path: 'test-results/real-models.png' });
  await measure(page, 'real-measure');
  await exportParts(page);
  // A box next to the first model should look made of the same plastic.
  await addBox(page, 0);
  await page.getByRole('button', { name: 'Faceplates' }).click();
  await page.getByRole('button', { name: 'Cages' }).click();
  await page.getByRole('button', { name: 'Iso' }).click();
  await page.screenshot({ path: 'test-results/real-models-box.png' });
  await page.close();
});

/** A new page on the app once it shows its first design, so nothing we do races that. `setup` runs before it loads. */
async function open(setup?: (page: Page) => unknown) {
  const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
  await setup?.(page);
  await page.goto(server.resolvedUrls!.local[0]);
  await designed(page);
  return page;
}

/** Text as it reads, on one line. */
const oneLine = (text: string | null) => (text ?? '').replace(/\s+/g, ' ').trim();

/** The header at rest, with the rack's name hovered, and with it focused. */
async function headers(page: Page, name: string) {
  const header = page.locator('header');
  await page.mouse.move(800, 600);
  await header.screenshot({ path: `test-results/${name}.png` });
  await page.getByLabel('Rack name').hover();
  await header.screenshot({ path: `test-results/${name}-hover.png` });
  await page.getByLabel('Rack name').focus();
  await page.mouse.move(800, 600);
  await header.screenshot({ path: `test-results/${name}-focus.png` });
  await page.getByLabel('Rack name').blur();
}

/** Export every part into test-results/parts and check it prints: watertight, little overhang. */
async function exportParts(page: Page) {
  const files = unzipSync(await download(page, /^All parts/));
  await mkdir('test-results/parts', { recursive: true });
  for (const [name, bytes] of Object.entries(files)) {
    const stl = Buffer.from(bytes);
    await writeFile(`test-results/parts/${name}`, stl);
    const { grams, overhang, leaks, sharp } = inspect(stl);
    console.log(`${name}: ${grams.toFixed(1)} g, ${overhang.toFixed(0)} mm² overhang, ${sharp.toFixed(0)} mm sharp${slice(`test-results/parts/${name}`)}`);
    assert.equal(leaks, 0, `${name} is not watertight`);
    // What's left are bridges: the tops of the cage's openings (30 mm at most; at the open sides
    // they span the ledges the device rests on too), the latch holes and the nut pockets.
    assert.ok(overhang < 1200, `${name} has ${overhang.toFixed(0)} mm² of overhang`);
  }
  return files;
}

/** What the Export STLs menu item `name` downloads. */
async function download(page: Page, name: RegExp) {
  await page.getByRole('button', { name: 'Export STLs' }).click();
  const [file] = await Promise.all([page.waitForEvent('download'), page.getByRole('button', { name }).click()]);
  return readFile((await file.path())!);
}

/** Measure between whatever two things are at either side of the view's middle, and leave a
 * screenshot with a third under the pointer, lit up. */
async function measure(page: Page, name: string) {
  const button = page.getByRole('button', { name: 'Measure' });
  await button.click();
  const view = page.getByRole('img', { name: '3D view of the rack' });
  const { width, height } = (await view.boundingBox())!;
  for (const x of [0.4, 0.6]) await view.click({ position: { x: width * x, y: height / 2 } });
  await page.getByRole('status').filter({ hasText: /\d( mm|°)/ }).waitFor({ timeout: 5_000 });
  await view.hover({ position: { x: width / 2, y: height * 0.55 } });
  // The pointer is looked at on the next frame, and what it found drawn on the one after.
  await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
  await page.screenshot({ path: `test-results/${name}.png` });
  await button.click();
}

/** Import a model into a panel through its Add device menu, and wait for the design. */
async function importModel(page: Page, panel: number, file: Parameters<Page['setInputFiles']>[1]) {
  await page.getByRole('button', { name: 'Add device' }).nth(panel).click();
  await designed(page, () => page.locator(`#add-${panel} input[type=file]`).setInputFiles(file));
}

/** Add a box of the default size to a panel through its Add device menu, and wait for the design. */
async function addBox(page: Page, panel: number) {
  await page.getByRole('button', { name: 'Add device' }).nth(panel).click();
  await designed(page, () => page.locator(`#add-${panel}`).getByRole('button', { name: 'New box' }).click());
}

/** Do something, then wait for the design it starts to finish, not for the one already shown.
 * Without an action, wait for the page's first design. */
async function designed(page: Page, action?: () => Promise<unknown>) {
  const before = action ? Number(await page.locator('[data-designs]').getAttribute('data-designs')) : 0;
  await action?.();
  await page.waitForFunction(
    (n) => {
      const el = document.querySelector('[data-designs]');
      return Number(el?.getAttribute('data-designs')) > n && el?.textContent?.includes('Designed in');
    },
    before,
    { timeout: 120_000 },
  );
}

/** Slice a part with stock settings on a 250 x 210 bed and sum what it extrudes by feature, in
 * mm of filament: what the slicer had to bridge, print as overhang, or fill in gaps. */
function slice(stl: string): string {
  if (!slicerData) return '';
  const gcode = join(slicerData, `${basename(stl)}.gcode`);
  // Slicing reports, it doesn't judge: a slicer that hangs now and then (one open in its own
  // window can) costs a line, not the run.
  try {
    execFileSync(SLICER, ['--datadir', slicerData, '--export-gcode', '--bed-shape', '0x0,250x0,250x210,0x210', '--center', '125,105',
      '--layer-height', '0.2', '--use-relative-e-distances', '--output', gcode, stl], { timeout: 60_000, stdio: 'ignore' });
  } catch {
    return ' | not sliced: PrusaSlicer failed or timed out';
  }
  // A part outside the printer's volume makes no G-code, and PrusaSlicer still exits fine.
  if (!existsSync(gcode)) return ' | not sliced: bigger than a 250 x 210 x 200 mm printer';
  const text = readFileSync(gcode, 'utf8');
  const by: Record<string, number> = {};
  let type = '';
  for (const line of text.split('\n')) {
    if (line.startsWith(';TYPE:')) type = line.slice(6);
    else if (line.startsWith('G1') && line.includes(' E')) by[type] = (by[type] ?? 0) + Math.max(0, Number(line.split(' E')[1].split(' ')[0]));
  }
  const time = text.match(/estimated printing time \(normal mode\) = (.*)/)?.[1];
  const mm = (k: string) => (by[k] ?? 0).toFixed(0);
  return ` | sliced: ${time}, bridges ${mm('Bridge infill')} mm, overhangs ${mm('Overhang perimeter')} mm, gap fill ${mm('Gap fill')} mm`;
}

/**
 * Weight in grams (at 1.25 g/cm³); the overhang, the area of faces off the bed that lean out more
 * than 46 degrees from upright, a degree past the 45-degree slopes we print without support; and the
 * edges where the surface doesn't close: walked one way more often than the other, once identical
 * float32 vertices merge. A pinch, where a window's corner just touches the cavity and four faces
 * share an edge, still closes and slices. And how long the edges that stand out sharp in a layer add up to, the
 * corners src/round.rs rounds where it can: what's left marks where the design leaves no room for a round.
 */
function inspect(stl: Buffer) {
  let [volume, overhang, sharp] = [0, 0, 0];
  const edges = new Map<string, number>();
  // Each directed edge: its face's normal, and the face's third corner.
  const faces = new Map<string, [number[], number[]]>();
  for (let t = 0; t < stl.readUInt32LE(80); t++) {
    const [a, b, c] = [0, 1, 2].map((k) => [0, 1, 2].map((i) => stl.readFloatLE(84 + t * 50 + 12 + k * 12 + i * 4)));
    const u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    const v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    const n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    volume += (a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0])) / 6;
    const twice = Math.hypot(...n);
    if (twice > 0 && n[2] / twice < -Math.sin((46 * Math.PI) / 180) && Math.max(a[2], b[2], c[2]) > 0.01) overhang += twice / 2;
    const keys = [a, b, c].map(String);
    const corners = [a, b, c];
    for (let k = 0; k < 3; k++) {
      const key = `${keys[k]}>${keys[(k + 1) % 3]}`;
      edges.set(key, (edges.get(key) ?? 0) + 1);
      if (twice > 0) faces.set(key, [n.map((x) => x / twice), corners[(k + 2) % 3]]);
    }
  }
  const leaks = [...edges].filter(([key, n]) => n !== edges.get(key.split('>').reverse().join('>'))).length;
  const dot = (p: number[], q: number[]) => p[0] * q[0] + p[1] * q[1] + p[2] * q[2];
  for (const [key, [n1]] of faces) {
    const [from, to] = key.split('>');
    const twin = faces.get(`${to}>${from}`);
    if (!twin || from > to) continue;
    const [n2, w] = twin;
    const [p, q] = [from, to].map((s) => s.split(',').map(Number));
    const along = [q[0] - p[0], q[1] - p[1], q[2] - p[2]];
    const length = Math.hypot(...along);
    const sharpish = dot(n1, n2) < 0.5 && dot(n1, [w[0] - p[0], w[1] - p[1], w[2] - p[2]]) <= 1e-6;
    if (sharpish && Math.abs(along[2]) > 0.2 * length && Math.hypot(n1[0], n1[1]) > 0.3 && Math.hypot(n2[0], n2[1]) > 0.3) sharp += length;
  }
  return { grams: (volume / 1000) * 1.25, overhang, leaks, sharp };
}

/** A 150 x 30 x 100 mm device with its four vertical edges chamfered 5 mm, as a GLB in meters. */
function standInDevice(): Buffer {
  const ring = (w: number, z: number) => [[-w, 0, z], [w, 0, z], [w, 30, z], [-w, 30, z]];
  const rings = [ring(70, 0), ring(75, -5), ring(75, -95), ring(70, -100)];
  const tris: number[][][] = [];
  for (let r = 0; r < 3; r++) {
    for (let i = 0; i < 4; i++) {
      const [a, b, c, d] = [rings[r][i], rings[r + 1][i], rings[r + 1][(i + 1) % 4], rings[r][(i + 1) % 4]];
      tris.push([a, b, c], [a, c, d]);
    }
  }
  const [f, k] = [rings[0], rings[3]];
  tris.push([f[0], f[1], f[2]], [f[0], f[2], f[3]], [k[0], k[2], k[1]], [k[0], k[3], k[2]]);
  const positions = new Float32Array(tris.flat(2).map((v) => v / 1000));
  const bin = Buffer.from(positions.buffer);
  const text = JSON.stringify({
    asset: { version: '2.0' },
    scenes: [{ nodes: [0] }],
    nodes: [{ mesh: 0 }],
    meshes: [{ primitives: [{ attributes: { POSITION: 0 } }] }],
    accessors: [{ bufferView: 0, componentType: 5126, count: positions.length / 3, type: 'VEC3', min: [-0.075, 0, -0.1], max: [0.075, 0.03, 0] }],
    bufferViews: [{ buffer: 0, byteLength: bin.length }],
    buffers: [{ byteLength: bin.length }],
  });
  // GLB chunks are 4-byte aligned; JSON pads with spaces.
  const json = Buffer.from(text.padEnd(Math.ceil(text.length / 4) * 4));
  const chunk = (type: number, data: Buffer) => {
    const head = Buffer.alloc(8);
    head.writeUInt32LE(data.length, 0);
    head.writeUInt32LE(type, 4);
    return Buffer.concat([head, data]);
  };
  const body = Buffer.concat([chunk(0x4e4f534a, json), chunk(0x004e4942, bin)]);
  const header = Buffer.alloc(12);
  header.write('glTF', 0);
  header.writeUInt32LE(2, 4);
  header.writeUInt32LE(12 + body.length, 8);
  return Buffer.concat([header, body]);
}
