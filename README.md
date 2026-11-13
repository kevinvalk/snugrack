<h1><img src="public/logo.svg" alt="SnugRack" height="56"></h1>

3D-printable rack mounts for gear that was never meant for a rack, designed from the devices' own 3D models.
Everything runs in the browser; nothing is uploaded.

Lay out a rack as panels of devices, import each device's model (GLB or STL) or describe it as a box, and export
printable parts:

- **Faceplate**, printed front down, with a *nest* that follows the device's front, so its face sits flush and can't
  be pushed through. Box-shaped devices sit behind a lip instead.
- **Cage**, printed back down: a snug shell that follows the device's outline all round, open at the back but for two
  struts its back rests on, so nothing rattles or moves when you plug a cable in. All four sides are big openings, with
  45-degree chamfers that keep every bridge short: little filament, lots of air, no supports. The device rests on low
  ledges along its sides with a couple of millimeters of air under it, so vents in its bottom keep breathing.
- **Latch**: the cage clicks into the faceplate, no tools (see below).
- **Ears and joiners**: EIA-310 ear slots, and ribs that bolt pieces together with M3 screws and nuts (see below).

The nest is a smooth slope, not stair steps, so it prints the same at any layer height. A device that's taller or wider
than what's left of its panel is left out with a warning; each panel shows the width it uses, and the rack can be set
to fit its devices: as wide as the widest panel needs, with ears like a 19" rack's. A device that comes with its own
rack ears goes in beside the panels, in as many units as it's tall, with nothing printed for it; either moves up or down
past the other. Drag devices between panels. The Hole tool in the view puts a round hole for a cable straight through a
panel's faceplate where you click, whatever is there. Turn, tip and roll an imported model until its front faces you;
models in the wrong size can be calibrated: measure a known feature in the view and type its real length. The
properties on the right are about whatever you pick in the layout or the view: the rack, with its Build and Fit (the
clearances), a panel, a device or a hole.

## Device library

Add device › From library… picks a device from `public/library.json`: names and links to their makers' 3D models,
which your browser downloads as you pick one. `library/unifi.py` refreshes Ubiquiti's; see `library/README.md`.

## The latch

The faceplate has a collar on its back, a short sleeve the cage's front slides into, with two nubs on each of its inner
top and bottom faces. The cage's top and bottom walls each have two tongues cut into them, at their left and right ends,
as far out as the device's top and underside run flat: each rooted at its inner end, free along its outer end and its
back edge, and as thick as the wall, with room behind it to bend in without touching the device. Where the device leaves
too little of that room, the wall stands a little proud over the tongue, and the collar follows; under the device, the
air it rests on gives some of the room, and under the ledges, where the wall runs higher, the bottom tongues run on
level with the wall over the air. Between the tongues, the openings in the top and bottom reach right up to the collar.
As the cage goes in, the tongues ride up the nubs' ramps and bend in, until the nubs snap into holes in them. Now the
nubs hold against pulling, the faceplate itself against pushing, and the device is held between the faceplate's nest and
the struts at the cage's back. The tongues reach 9 mm past the collar: to release, squeeze the cage's top and bottom in
there, both sides, and pull the cage out backwards. The cage prints standing on its back, so the tongues bend within its
layers, not across them; the nubs' catches stand at 60 degrees to the pull, a ledge under a millimeter high at 30
degrees as the faceplate prints, which needs no support, and they give way to a hard pull before anything breaks. A
tongue is just long enough not to strain more than 1.5% at its root. Where there's no room for the latch, the cage isn't
latched, and says why: the device's top or underside doesn't run flat for long enough, the panel is too low for the
collar round the tongues, or the cage is too short. The rack's Fit tunes how far the nubs reach and how much play the
cage has once latched.

## Joints

With separate pieces on, each device's share of a panel prints on its own; each end of a panel is an ear printed with
its end piece, an ear bolted on, a joiner, or plain. Devices sit spread out, the same room in every gap, the ends
included, for the most room to work in at each joint; or packed left, centered or right, so a bolted-on ear can fill
the rest of a panel until a device takes its place.

Any of M3x8, M3x10 or M3x12 screws will do: socket head (ISO 4762, a 2.5 mm hex key), button head (ISO 7380, a 2 mm
key), flanged button head (ISO 7380-2) or pan head with an L-shaped key, with M3 nuts, square (DIN 562) or hex
(DIN 934). The nuts always sit in the device pieces, in a solid block that runs from the seam on into the device's
collar: drop each nut into its slot from the back and push it down with a 1.5 or 2 mm hex key until it lines up with the
screw hole, where it can't turn. To join two device pieces, put the screws through the right-hand one's rib, with 20 mm
of room behind it for any of those screws and a standard 2.5 mm L-key, and push the pieces together; at an ear, the
screws go in from the ear's side. Of the three screws, the one in the middle near the faceplate pulls the seam shut, and
the two high and low at the back of the 40 mm deep joints keep the panel straight. A 60-degree gusset where the screws'
rib meets the faceplate keeps it from cracking there, with an 8 mm hole through it for the middle screw's head, up to a
6.9 mm flange, and the key. The seams are square, and a tongue along each slides into a groove in the next piece, so the
faces stay flush and you can't see through the crack.

## Layout

```
src/                     the core, in Rust: silhouettes, nest, cavity, latch, rack -> a CSG tree per part
ui/                      the app, in Svelte, Tailwind and three.js
  studio.svelte.ts       its state: the open document, the latest design, the selection, and every action
  files.ts               device models in, STL out
  engine/                a web worker where the core designs parts and manifold-3d builds them
  viewport/              the 3D view
  wasm/                  the core built for the browser, with its TypeScript types (built, not committed)
library/                 the device library's scraper (see Device library)
feedback/                notes for an agent while developing: overlay, dev server plugin, MCP server
e2e.test.ts              the app in a browser, down to sliced parts
fixtures/                your own device models for the tests, never committed
```

## Run

Requires [rustup](https://rustup.rs) (`rust-toolchain.toml` brings the rest of Rust) and Node 26+ (`nvm install` picks it up from `.nvmrc`).

```sh
npm install
npm run dev        # http://localhost:5173
npm run build      # a static site in dist/
```

Both build the core into `ui/wasm` first; `npm run wasm` does just that.

## Check

```sh
cargo test                                  # incl. random racks
cargo clippy --all-targets -- -D warnings
cargo fmt --check
npm run check                               # types
npx playwright install chromium             # once, and after each Playwright update
npm run e2e
```

CI runs all of these on every push (`.github/workflows/check.yml`), without slicing.

`npm run e2e` builds a two-panel rack through the UI, with two devices side by side, exports every part and checks it
is watertight with no overhangs beyond short bridges. It imports a part back as an STL device, reloads to check the
rack persists, and leaves screenshots and STLs in `test-results/`. It also runs the measuring tests in
`ui/viewport/measure.test.ts`. With PrusaSlicer installed (or `PRUSASLICER` pointing at it) it slices every part and
reports print time, filament, bridges, overhangs and gap fill. `SNUGRACK_MODELS=fixtures/a.glb,...` adds a rack of
your own models, one per panel, with the same checks.

## Feedback

While developing with an agent, `npm run dev` adds a Feedback button. Pin a note to a spot on a part (Model), to an
element of the interface (UI), or write a free one (Note). Each goes to `.feedback/` (not committed) with a screenshot,
the open document, and where it points: the point, the part and the Rust code that made that face (every solid in the
CSG tree is `labeled`, with the file and line), or the Svelte component and line that rendered the element. The
agent's answers show up in the panel, where you reply, resolve or reopen.

The agent reads the notes through an MCP server, which can also take a screenshot of the open app and read its state:

```sh
claude mcp add snugrack-feedback -- node feedback/mcp.ts
```

None of it reaches `npm run build`.
