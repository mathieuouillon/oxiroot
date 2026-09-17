#!/usr/bin/env python3
"""Convert ReX's render history into crates/oxiroot-rex/tests/history.txt.

Usage:
    python3 scripts/gen_rex_history.py <ReX checkout> <output path>

Reads `tests/data/regression_render.yaml` (which font each snippet uses) and
`tests/data/history_regression_render.yaml` (the recorded layout and draw
commands) from an upstream ReX checkout, and writes one record per snippet:

    EQ <Xits|Garamond> <hex TeX> <width> <height>   a successful render,
    S <x> <y> <glyph id> <scale>                     followed by its glyphs
    R <x> <y> <width> <height>                       and rules,
    END
    ERR <Xits|Garamond> <hex TeX>                   a render that failed

History entries whose snippet is no longer in regression_render.yaml are
skipped and counted. Needs PyYAML.
"""

import sys
from pathlib import Path

import yaml


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    rex = Path(sys.argv[1])
    out_path = Path(sys.argv[2])
    data = rex / "tests" / "data"
    regression = yaml.safe_load((data / "regression_render.yaml").read_text())
    history = yaml.safe_load((data / "history_regression_render.yaml").read_text())

    fonts = {}
    for collections in regression.values():
        for collection in collections:
            for snippet in collection["Snippets"]:
                key = f'{collection["Description"]} - {snippet}'
                fonts[key] = collection.get("Font", "Xits")

    ok = err = skipped = 0
    lines = []
    for key, entry in history.items():
        if key not in fonts:
            skipped += 1
            continue
        font = fonts[key]
        tex = entry["tex"].encode().hex()
        render = entry["render"]
        if "Err" in render:
            lines.append(f"ERR {font} {tex}")
            err += 1
            continue
        result = render["Ok"]
        lines.append(f"EQ {font} {tex} {result['width']!r} {result['height']!r}")
        for command in result["render"]["commands"]:
            if "Symbol" in command:
                s = command["Symbol"]
                lines.append(f"S {s['pos'][0]!r} {s['pos'][1]!r} {s['glyph_id']} {s['scale']!r}")
            else:
                r = command["Rule"]
                lines.append(f"R {r['pos'][0]!r} {r['pos'][1]!r} {r['width']!r} {r['height']!r}")
        lines.append("END")
        ok += 1

    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text("\n".join(lines) + "\n")
    print(f"{out_path}: {ok} renders, {err} errors, {skipped} history entries skipped")


if __name__ == "__main__":
    main()
