# yamma-intro slides

Deck: "Pilot 7 Synthetic Data Update" — building benchmark and ground truth from EDC extraction to SDTM and ADaM.

A Quarto revealjs deck introducing the yamaa project and the pilot7
synthetic-data effort to the open source community.

## Files

- `index.qmd` — the deck (skeleton: section headers, short bullets,
  figure placeholders). Structure follows the
  [rinpharma2025 template](https://github.com/nanxstats/pycsr/tree/main/slides/rinpharma2025).

## Render

```bash
cd docs/slides/yamma-intro
quarto render index.qmd
```

(`embed-resources: true`, so the output is a single self-contained HTML file.)

## Conventions

- Diagram-first: every concept slide should carry a figure, not paragraphs.
- Figure placeholders are HTML comments (`<!-- TODO(figure): ... -->`)
  until the real visual lands.
- Existing figures live in `docs/diagrams/`:
  - `design.svg` — inheritance: org → compound → study templates → validated builds
  - `derive-simple.svg` — key table, one row per key combination
    (technical; may need a simpler flow figure for this audience)
  - `derive-full.svg` — row-template sections concatenated vertically (technical)
- Benchmark counts cited in the deck: 265 runnable benchmarks
  (143 positive, 122 negative). Re-verify before presenting:
  `find benchmarks -maxdepth 1 -mindepth 1 -type d | wc -l`.

## Open items (collaborate in chat)

1. Event/date for the title slide (`date: "TBD"`).
2. Simple engine-flow figure: ODM XML → yamaa spec → engine → SDTM/ADaM + define.xml.
3. Adoption cards: 510(k), CDISC/RConsortium pilots, Phase 3 oncology — logos or one-line proof each.
4. Pilot7 ground-truth flow figure.
5. Keiji.ai agentic-framework visual (from their side).
6. People slide: confirm name spellings/affiliations, headshots or not.
7. Real 20-line spec example for the "A spec in 20 lines" slide.
