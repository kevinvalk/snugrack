# Device library

`public/library.json` lists devices whose 3D models their makers publish, one entry a line: its `name`, `manufacturer`,
`model` (the SKU, optional), `category`, a link to its `glb` (absolute, or relative to the library) and, if known, which
way the model's `front` and `up` face, as the app's Turn buttons would set them (`posX`, `negX`, `posY`, `negY`, `posZ`
or `negZ`). Add device › From library… in the app searches it; picking one downloads its model in the user's browser and
imports it as if dropped in, facing that way, or else the way a GLB does by default.

## Refresh

```sh
python3 library/unifi.py
```

It takes about 11 minutes, a second between requests, and rewrites the Ubiquiti entries; others stay as they are. It
walks the categories in `CATEGORIES` on techspecs.ui.com, reads each product's store page for its model's link, and
keeps those that have one: in November 2026, 14 cloud gateways, 58 switches, 6 storage and 7 internet products, 13
NVRs and Cloud Keys, 6 door access hubs, and 9 UPSes, power distribution and surge protection units. Ubiquiti's power
supplies and PoE injectors have no models.

To find a model's front, it reads only the start of its GLB, the JSON that names its parts, and lets each part vote
for the side it's on: drive bays and screens for the front, fans and power sockets against, ports and lights a little
for. A side with at least 3 votes and twice the next one's is the front; about three in five entries get one. Their
top is +Y, glTF's up, in all of Ubiquiti's models, so `up` stays out. Wrong guesses are the user's to turn.

## Ubiquiti's content

Ubiquiti's terms don't allow republishing their content, so we don't: the library holds product names and links, and
the user's browser downloads a model from Ubiquiti's own servers when it's picked, as their store does. Nothing of
theirs is in this repository or served by the app.

## Remove

Delete `library/`, `public/library.json` and `ui/Library.svelte`; the `Library` and `LibraryBig` imports, `<Library />`
and From library… lines in `ui/RackPanel.svelte`; the library test in `e2e.test.ts`; and the Device library section and
`library/` line of the README.
