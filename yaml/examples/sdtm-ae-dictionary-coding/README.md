# SDTM AE dictionary coding

This fixture covers value mapping where the mapping rule lives in a separate
file rather than inline in the specification, and where a source value has no
entry in that file.

MedDRA coding is the canonical case: `AETERM` holds the verbatim term the site
reported, and the dictionary translates it into `AEDECOD` and `AEBODSYS`.

## Why this is not an R003 join

R003 matches on applicable keys, meaning output `keys` that also exist in the
right-side dataset. This lookup matches on the reported term, which is not an
output key and never will be. The key-based join cannot express it.

The `mapping_from` expression names its dataset, match column, and return column
explicitly:

```yaml
mapping_from:
  source: AE_RAW.AETERM
  dataset: MEDDRA
  key: LLTNAME
  value: PTNAME
```

The source dataset is `AE_RAW`, not `AE`. The output `domain` is `AE`, and a
source dataset must not reuse that name, so that a qualified reference cannot be
read as addressing the dataset being derived. See R002.

`dataset` must name a dataset declared in root `datasets` and is resolved by
R002 like any other. That makes the dictionary an ordinary declared input
rather than a special mechanism.

Two derivations read the same dictionary on the same key and return different
columns, which is how one coding pass populates several output variables.

## Undefined values

`Felt a bit off` and lowercase `headache` are non-missing values that are not
exact keys in the dictionary, so `mapping_from` takes its local `unmapped`
path. A blank term takes the distinct `missing` path:

```yaml
missing: NOT CODED
unmapped: NOT CODED
```

Without that handler the run fails, which is the correct default: an
uncodeable term is normally a data-management query, not something to pass
through silently. Declaring the handler is how a specification states that it
has considered the case.

R008 treats these as local handlers because both belong directly to the
`mapping_from` expression that encounters the input condition.

The same handler appears in `adam-adsl-mapping` against an inline `mapping`.
The two together show that the undefined-value case is a property of the
mapping, not of where the dictionary is stored.

## Dictionary version

The version is recorded twice, deliberately:

- in the filename, `input/meddra_26_1.csv`, so the artifact is self-identifying;
- in root `metadata`, as `dictionary` and `dictionary_version`, so it is
  machine-readable for downstream define.xml generation.

This is the first use of `metadata` in any fixture. **No rule governs the field
yet**, so nothing constrains these key names or requires an implementation to
carry them through. Coding dictionary and version are submission-critical
provenance, so this is a real gap rather than a cosmetic one.
