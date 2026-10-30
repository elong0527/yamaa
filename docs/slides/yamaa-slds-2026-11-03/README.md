# yamaa-slds-2026-11-03 slides

"yamaa: agentic workflow for CDISC data standardization" -- a presentation
for SLDS 2026 (2026-11-03, Marriott at the Brooklyn Bridge, New York).

The deck reuses the NJSUG 2026-10-09 structure (motivation, MAP prompt
tiers, YAML example, core principle, workflow) and points to the
RConsortium Pilot 7 site for the full-study work. Its YAML example is
illustrative; check the current schema before reusing it.

[View the rendered slides](https://elong0527.github.io/yamaa/slides/yamaa-slds-2026-11-03/index.html)

## Files

- `index.qmd` -- the deck
- `custom.css` -- the deck's single style file (copied from yamaa-njsug-2026-10-09)
- `title-logo.html` -- title-slide logo, included via `include-after-body` and moved right after the subtitle at runtime

## Build

The HTML is built by the Deploy docs GitHub Action and published with the
docs site. To preview locally: `quarto render index.qmd` from this folder.
The output is one self-contained HTML file (`embed-resources: true`).
