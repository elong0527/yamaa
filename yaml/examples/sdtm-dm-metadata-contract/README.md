# SDTM DM metadata and artifact contract

This focused probe answers one question: how much of a submission metadata
contract can the schema carry today?

## Rule and record grain

`DM_RAW` is the base, so each collected subject produces one DM row. The
derivations are deliberately ordinary: direct sources, two literals, and one
`str_concat` building `USUBJID` from study, site, and subject. The subject of
the fixture is the metadata around them, not the values.

Every column declares a `label`. Every column declares `metadata` with an
`origin`, a `length`, and, where a controlled terminology list applies, a
`codelist`. The root declares a dataset label, class, structure, and the
standard version.

## What is carried, and what carrying it means

`label` is a first-class field, so a label is part of the specification and
survives review. Everything else is a free-form `dict[str, str]`.

That has consequences the fixture makes visible.

- **There is no vocabulary.** `origin`, `length`, and `codelist` are names this
  fixture invented. Another specification could call them `Origin`, `len`, and
  `ct` and be equally valid. Nothing can be validated, compared, or
  automatically transformed into Define-XML.
- **Lengths are strings.** `metadata` values are typed `str`, so `"20"` is
  quoted text. No implementation can enforce it, and nothing connects it to the
  declared `type`.
- **A codelist is a name, not a reference.** `SEX` declares
  `codelist: SEX` and separately declares `allowed_values: [F, M, U]`. The two
  are unrelated: one is documentation, one is enforced, and nothing keeps them
  consistent.
- **Origin is not derivable.** `USUBJID` is marked `Derived` by hand even
  though the schema already knows it comes from a `str_concat` over three
  collected columns. The lineage exists in the specification and is not exposed.
- **Dictionary versions live in two places.** `../sdtm-ae-dictionary-coding`
  declares `dictionary_version` in root metadata while the dictionary itself is
  an input dataset. Nothing ties the declared version to the file loaded.

## No expected artifact

X11 asks for a metadata manifest to be asserted alongside the data. This
fixture commits `expected/dm.csv` only.

The plan's fixture contract says that until a machine-readable diagnostics or
metadata schema is defined, expectations belong in a README rather than in an
invented file shape. Writing an `expected/metadata.yaml` here would fix a shape
by accident. The missing piece is therefore not this fixture; it is the
decision about what a metadata artifact contains and which parts of it are
normative.

Output column order is declaration order, which for this fixture matches the
conventional DM order. `../sdtm-suppmh-qualifiers` shows that output *row*
order has no control at all, and a transport artifact needs both.

## Status and named gaps

This fixture is a **probe**, and it is the one that passes while proving the
least. It names five gaps.

1. Metadata is an ungoverned string map with no vocabulary and no validation.
2. Length is declared as text and is unconnected to the column type, so a
   transport format cannot be produced from the specification.
3. Controlled terminology is named in metadata and enforced separately in
   verifications, with no link between them.
4. Origin and lineage are hand-written although the derivation already encodes
   them.
5. There is no expected metadata artifact, so none of the above is asserted by
   any golden file.

## Diagnostics and verifications

No handler path is declared. Rows remain in `DM_RAW` order; the key is
`[STUDYID, USUBJID]`; exactly three rows are expected. `STUDYID` and `USUBJID`
must be present, `AGE` must be within a plausible range, and `SEX` must be one
of three values.
