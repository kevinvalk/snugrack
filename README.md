<h1><img src="public/logo.svg" alt="SnugRack" height="56"></h1>

3D-printable rack mounts for gear that was never made for a rack: switches, gateways, mini PCs. SnugRack designs each
mount around the device's own 3D model, so it fits snugly, prints without supports, and clicks together without tools.
Everything runs in your browser; nothing is uploaded.

## Why

Most small network gear comes without rack ears. Printed mounts exist, but each is made for one device and one rack,
and a generic tray lets things slide around. SnugRack makes a mount for whatever you have, from its maker's model or
just its size, and lays out a whole rack of them at once.

## Using it

1. **Lay out your rack.** Add panels (one rack unit each, or taller) and put devices in them: pick one from the library,
   import its model (GLB or STL), or describe it as a box. Turn the model until its front faces you. If it comes in the
   wrong size, measure a feature you know in the view and type its real length. A power supply, or anything else whose
   cables come out of its front, can sit behind a blank faceplate, with room in front of it for its plugs and cables.
2. **Check the warnings.** A device that doesn't fit its panel, or has no room for the latch, says so and why.
3. **Export STLs** and print them without supports: faceplates front down, cages back down. Tune the clearances under
   Fit after a test print.
4. **Assemble.** Pieces of a panel bolt together with M3x8 to M3x12 screws and M3 nuts, square or hex: drop each nut
   into its slot from the back, push it down with a 1.5 or 2 mm hex key, and put the screws in from the right-hand
   piece. Slide each device into its cage, and the cage into its faceplate until it clicks. To take it out again,
   squeeze the cage's top and bottom just behind the faceplate and pull.

## Development

Requires [rustup](https://rustup.rs) (`rust-toolchain.toml` brings the rest of Rust) and Node 26 (`nvm install` reads
`.nvmrc`).

```sh
npm install
npm run dev        # http://localhost:5173
npm run build      # a static site in dist/
```

The geometry lives in a Rust core (`src/`), built to WebAssembly into `ui/wasm` by every npm script; the app is Svelte
(`ui/`), with manifold-3d building the parts in a web worker.

```sh
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
npm run check                                  # types
npx playwright install --only-shell chromium   # once, and after each Playwright update
npm run e2e                                    # the app in a browser, down to exported parts
```

CI runs these on every push, and publishes `main` to GitHub Pages. The e2e builds its own copy of the app in
`target/e2e`, so it runs beside `npm run dev`, and leaves screenshots and STLs in `test-results/`; with PrusaSlicer installed (or `PRUSASLICER` pointing at it) it also slices every part, and
`SNUGRACK_MODELS=fixtures/a.glb,...` adds your own models, kept in `fixtures/`, which is never committed.

**Feedback notes.** `npm run dev` adds a Feedback button for developing with an agent: pin a note to a spot on a part,
to an element of the interface, or write a free one. Notes go to `.feedback/` (not committed) with a screenshot and the
code that made what they point at. The agent reads and answers them through an MCP server:

```sh
claude mcp add snugrack-feedback -- node feedback/mcp.ts
```

**Device library.** `public/library.json` lists devices whose makers publish 3D models; see `library/README.md` to
refresh it.
