"""Refresh public/library.json with Ubiquiti's rack gear that has a 3D model.

    python3 library/unifi.py

Standard library only. It walks a few categories on techspecs.ui.com for products, reads each one's store page for
the link to its GLB, and keeps the products that have one, with the way the model faces where its part names tell.
We write names and links only: the user's browser downloads a model from Ubiquiti when it's picked. Entries other
than Ubiquiti's in the file are kept.
"""

import json
import re
import struct
import time
import urllib.error
import urllib.request
from collections import Counter
from pathlib import Path

LIBRARY = Path(__file__).parent.parent / "public" / "library.json"
TECHSPECS = "https://techspecs.ui.com"
# Categories of gear that goes in a rack, and of each the subcategories we want; none means all of them.
CATEGORIES = {
    "cloud-gateways": (),
    "switching": (),
    "integrations": ("network-storage", "internet-solutions"),
    "physical-security": ("physical-security-nvr",),
    "door-access": ("door-access-hub", "door-access-nvrs"),
    "accessories": ("accessories-poe-power",),  # UPSes, power distribution and surge protection
}
# What a part's names say about the side it's on, the first that matches: drive bays and screens are at the front,
# fans and power sockets at the back, and ports and lights mostly at the front (NASes and NVRs have them at the back).
VOTES = {
    re.compile(r"front|hdd|slot|bay|screen|lcm|display", re.I): 5,
    re.compile(r"fan|socket|plug", re.I): -5,
    re.compile(r"rj-?45|port|sfp|pin|led|light", re.I): 1,
}


def get(url: str, glb=False):
    """A page as text or, from a GLB, just its JSON: the mesh data that follows it is most of the download."""
    time.sleep(1)  # politeness
    request = urllib.request.Request(url, headers={"User-Agent": "Mozilla/5.0 (compatible; library-scraper)"})
    with urllib.request.urlopen(request, timeout=60) as response:
        if not glb:
            return response.read().decode()
        length = struct.unpack("<12xI4x", response.read(20))[0]
        return json.loads(response.read(length))


def front(gltf) -> str | None:
    """Which side of the model is its front, if its parts' names clearly say. Its top we leave at +Y, glTF's up,
    where Ubiquiti's models all have it.

    A mesh's accessor gives its bounding box, so we needn't decode its (compressed) data: each part votes, as its
    names and those of the nodes above it say, for the side of the whole model it's nearest to, relative to the
    model's size."""
    parts = []  # each part's corners in the model, and the names that describe it

    def walk(index, parent, names):
        node = gltf["nodes"][index]
        if "matrix" in node:  # column major
            m = node["matrix"]
            local = lambda p: [sum(m[c * 4 + r] * v for c, v in enumerate(p)) + m[12 + r] for r in range(3)]
        else:
            x, y, z, w = node.get("rotation", (0, 0, 0, 1))
            t, s = node.get("translation", (0, 0, 0)), node.get("scale", (1, 1, 1))

            def local(p):  # p turned by the quaternion: p + 2w(q × p) + 2q × (q × p), with q = (x, y, z)
                p = [a * b for a, b in zip(p, s)]
                u = (y * p[2] - z * p[1], z * p[0] - x * p[2], x * p[1] - y * p[0])
                uu = (y * u[2] - z * u[1], z * u[0] - x * u[2], x * u[1] - y * u[0])
                return [p[k] + 2 * (w * u[k] + uu[k]) + t[k] for k in range(3)]

        place = lambda p: parent(local(p))
        names += " " + node.get("name", "")
        mesh = gltf["meshes"][node["mesh"]] if "mesh" in node else {"primitives": ()}
        for primitive in mesh["primitives"]:
            box = gltf["accessors"][primitive["attributes"]["POSITION"]]
            material = gltf["materials"][primitive["material"]].get("name", "") if "material" in primitive else ""
            corners = [place([(box["min"], box["max"])[c >> k & 1][k] for k in range(3)]) for c in range(8)]
            parts.append((corners, f"{names} {mesh.get('name', '')} {material}"))
        for child in node.get("children", ()):
            walk(child, place, names)

    for index in gltf["scenes"][gltf.get("scene", 0)]["nodes"]:
        walk(index, lambda p: p, "")
    every = [p for corners, _ in parts for p in corners]
    low, high = [min(v) for v in zip(*every)], [max(v) for v in zip(*every)]
    votes = Counter()
    for corners, names in parts:
        if vote := next((v for words, v in VOTES.items() if words.search(names)), 0):
            lo, hi = [min(v) for v in zip(*corners)], [max(v) for v in zip(*corners)]
            gaps = {}
            for k, axis in enumerate("XYZ"):
                gaps[f"pos{axis}"] = (high[k] - hi[k]) / (high[k] - low[k] or 1)
                gaps[f"neg{axis}"] = (lo[k] - low[k]) / (high[k] - low[k] or 1)
            votes[min(gaps, key=gaps.get)] += vote
    # With the top up, the front is one of the sides. We're sure with three votes and twice the runner-up's.
    side, runner_up = sorted(("posX", "negX", "posZ", "negZ"), key=votes.__getitem__, reverse=True)[:2]
    return side if votes[side] >= max(3, 2 * votes[runner_up]) else None


def products():
    build = re.search(r'"buildId":"([^"]+)"', get(TECHSPECS)).group(1)
    data = f"{TECHSPECS}/_next/data/{build}/unifi"
    seen = set()  # a product can be in two subcategories
    for category, wanted in CATEGORIES.items():
        listing = json.loads(get(f"{data}/{category}.json"))["pageProps"]["subCategoriesWithProducts"]
        for sub in listing:
            if wanted and sub["id"] not in wanted:
                continue
            for item in sub["products"]:
                if item["slug"] in seen:
                    continue
                seen.add(item["slug"])
                yield category.replace("-", " ").capitalize(), f"{data}/{category}/{item['slug']}.json"


def main():
    started = time.monotonic()
    entries = []
    for category, specs in products():
        try:
            p = json.loads(get(specs))["pageProps"]["product"]
            page = get(f"https://store.ui.com/{p['storeId']}/en/products/{p['slug']}")
        except urllib.error.HTTPError as e:  # not in the store (any more)
            print(f"{specs}: {e}")
            continue
        # The page links other products' models too: only this one's id will do.
        glb = re.search(rf"https://cdn\.ecomm\.ui\.com/products/{p['id']}/[\w-]+\.glb", page)
        if not glb:
            continue
        entry = {"name": p["title"], "manufacturer": "Ubiquiti", "model": p["name"], "category": category,
                 "glb": glb[0]}
        if side := front(get(glb[0], glb=True)):
            entry["front"] = side
        entries.append(entry)
        print(f"{p['name']}: {glb[0]} {side or ''}")
    others = [e for e in json.loads(LIBRARY.read_text()) if e["manufacturer"] != "Ubiquiti"] if LIBRARY.exists() else []
    entries = others + sorted(entries, key=lambda e: (e["category"], e["name"]))
    # A line each, so a refresh's diff shows what changed.
    LIBRARY.write_text("[\n" + ",\n".join(json.dumps(e, ensure_ascii=False) for e in entries) + "\n]\n")
    print(dict(Counter(e["category"] for e in entries[len(others):])), f"in {time.monotonic() - started:.0f} s")


if __name__ == "__main__":
    main()
