# AGENTS.md

How the user wants SnugRack worked on: what the code and the README don't say. Read the README for what it is and how to
run and check it.

## Rule one: elegance

Above every other rule. Elegant means pleasingly ingenious and simple: the same result with less. Run at least three
rounds of the pass below on every change before calling it done, and whenever the user asks for elegance.

- **Less is more:** fewer lines, files, types, dependencies and moving parts, in code, docs and developer experience.
  If something can go without losing quality or behavior, it wasn't elegant enough. Weigh what each piece buys, though:
  a dozen lines that stop a real bug are cheap.
- **Every abstraction earns its place:** no type, trait, helper, wrapper, knob or config until a second real use needs
  it. One that doesn't pay its way goes, one that does gets leaned into.
- **Plain over clever:** a string is enough until a type actually buys something. No cute tricks a reader has to
  decode.
- **Together over sprawl:** what changes together lives together, a type next to its only user, one file rather than
  three small ones. Split by meaning when a file holds unrelated things, never by size or kind.
- **Each thing where its language says it best:** geometry and rules in the core, wording and layout in the UI. A wash
  in size goes where the architecture wants it.
- **Proper tools over strings:** structured data and well-known libraries over templates and hand-rolling, the platform
  (HTML, CSS, the browser, the standard library) before code of our own.
- **Consistency is elegance:** one way to do a thing, the same names, shapes and layout everywhere.

The pass:

1. **Scope** the whole codebase unless the user names a part, never just what you wrote last. Count lines, files and
   dependencies.
2. **Question every piece** against the list above: file, type, helper, dependency, command, config, doc line. Can
   something we already have do it? Is it a string where structure belongs? Does it sit where its readers would look?
3. **Research before deciding:** read the source and docs of what we use, look at what exists (downloads, last release,
   maintainer), prototype outside the repo when unsure. Weigh at least two ways and know why the others lose.
4. **Cut**, biggest first: files and dependencies, then types and abstractions, then lines. Then question again, until
   a whole round finds nothing.
5. **Prove** it: tests, lints, typecheck, the running app, and count again.
6. **Report** the counts before and after, what went, what stayed and why, what you weighed and why it lost.

Never trade away behavior, safety or a test of what we must not get wrong.

## Working with the user

- **Ask, don't guess.** Not sure how a printer, slicer or library behaves, or which of two reasonable designs the user
  wants? Stop and ask, with your recommendation first. Batch questions, don't ask what the code or a sensible default
  answers. Where sources disagree, trust a printed part, then the slicer, then well-known printing guides, then the
  spec (EIA-310). When the user describes a shape and you're unsure you see it, ask for a sketch or a model.
- **The user reviews all code and does the commits.** Never stage, commit, stash or reset. Unexpected changes in the
  working tree are theirs: build on them, never revert them, say so if one looks wrong.
- **Untracked files you didn't make aren't yours:** don't touch, move or clean them (no `git clean`, no `git stash -u`).
- **Don't change code while the user is testing**, and don't build over their running app (`npm run dev` on port
  5173): `npm run wasm`, `build` and `check` rewrite the `ui/wasm` it serves, so check first (`pgrep`, `lsof`) and wait.
  `npm run e2e` builds into `target/e2e` and is safe beside it. Core changes only reach their app when they restart it.
- **Only the user prints parts and fits them to real devices.** Device models in `fixtures/` are theirs and never get
  committed: we don't know whether their makers allow it.
- **Feedback notes:** the user pins notes in the running app; read them with the `snugrack-feedback` MCP tools
  (`feedback_list`, `feedback_get`), follow `source` to the code, and reply when done (`feedback_reply`), with what
  changed and what to check. Resolve only what you verified; the user can reopen.
- **Keep them posted:** a short line of what you're on every few steps, findings and decisions in plain words.
- **Report faithfully:** what you verified and how, what you didn't and why, what's left for the user to try.

## Safety and privacy

- Never put the user's name, email or other personal details in code, comments, fixtures or requests. Refer to "the
  user".
- Give every command a time limit (`perl -e 'alarm 120; exec @ARGV' <command>`): a hang costs far more than a retry.
- Downloads go in a fresh folder of their own under `/tmp`, scripts you write in another. Never search the whole disk.
- Clean up after yourself: stop what you started, free the ports, delete scratch files.

## Code

- Comments only where a reader would otherwise get it wrong: why, not what. Terse, "we" voice, sentences, lines up to
  ~120 characters. Keep them true when the code changes.
- American English, sentence case, no jargon a newcomer wouldn't know.
- Problems are data: the core reports typed issues (`Issue`) and the UI words them. Keep distinctions that change
  recovery, presentation or diagnosis, flatten nothing to strings early.
- Work that outlives its caller gets an identity (a counter is enough): a late completion must never change what
  replaced it.
- `ui/wasm` is build output: nobody edits or commits it.
- Keep side effects behind one module: storage in `studio.svelte.ts`, files in `files.ts`, the core and the mesh kernel
  in `engine/worker.ts`.

## Printing

- Every part prints without supports in the orientation we export it: faceplates front down, cages back down. Keep
  overhangs to short straight bridges, and walls to multiples of a line width.
- Anything that flexes bends within the layers of its part as printed, never across them.
- Surfaces follow the device, not the layer height: no stair steps.
- Every part is watertight; coplanar faces in the CSG tree are bugs waiting for float32, so cuts reach past the faces
  they open through.
- Judge printability the way the printer sees it: slice the exported part and read its toolpaths layer by layer (what
  each layer rests on, where it bridges), not just the angles of its faces.
- Build parts sharp. The corners each layer has are rounded after the fact (`src/round.rs`, Round edges under Build),
  so never round a convex corner by hand; design in only what adds material: fillets, and corners another part fits
  round. The e2e prints how many mm of sharp edge each part keeps: what's left marks a spot with no room for a round.

## Tests

- Spec and integration tests (the running app, real slicing) over unit tests.
- A test of a race sets the overlap up deterministically, never sleeps.
- A regression test must fail without the fix: break the code on purpose and watch it fail before you trust it.
- Before calling anything done: `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`,
  `npm run check` and `npm run e2e`.
- See your work in `npm run e2e`, not the user's app: it serves its own build on port 4174, away from their dev server
  and data. Look at every screenshot in `test-results/` of what you touched, add their models with `SNUGRACK_MODELS`, leave
  nothing listening, and keep it passing without a slicer or fixtures, as CI runs it.

## UI

- Calm, native-feeling, one look everywhere: the framework's own scale and tokens, no one-off sizes or colors.
- The core hands over data, the UI says it in words where it shows it: units, Yes or No.
- The platform first: native HTML controls, `<dialog>`, `title` tooltips, `popover`, CSS over JavaScript.
- Utilities in the markup. A component that repeats a style repeats it through a snippet or a class string; a class in
  `app.css` is only for the controls the whole app shares. Prefer `w-1` over `w-[4px]`.
- Every state shows: designing in place, a dash for what's missing, errors with a way to try again, what an action did
  in a status line for as long as it takes to read.
- Anything that can lose work asks first.
- Accessible: a name for every icon-only control and canvas, proper roles, a visible focus ring, nothing that steals
  focus while hidden.
- Components sit in the folder of what uses them, named for what they are, not where they show.
