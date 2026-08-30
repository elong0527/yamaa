# yamaa <img src="docs/assets/logo.jpeg" align="right" width="120" alt="YAMAA logo" />

YAMAA is a language-neutral YAML specification for reproducible clinical-trial data pipelines from ODM to SDTM, ADaM, and TLFs. 

YAMAA is designed for AI-first workflow while keeping derivations reviewable, version-controlled, and consistent across implementations.

## Design

![YAMAA design: inherited templates become study specifications that drive validated SDTM, ADaM, and TLF builds](docs/diagrams/design.svg)

Reusable templates flow from organization to compound to study. Approved study specifications then drive deterministic, validated builds while preserving metadata lineage.

## Repository

- [`yaml/`](yaml/) - schemas, execution rules, and examples
- [`cdiscbuildeR/`](cdiscbuildeR/) and [`R/`](R/) - R implementation and workflows
- [`python/`](python/) - Python implementation
- [`docs/`](docs/) - diagrams and assets

Start with the [YAML specification](yaml/README.md) or browse the [worked examples](yaml/examples/README.md).

## Example 

The spec is designed to be deterministic in principle with SQL syntax, and AI agents could be involved to generate the spec based on the study design and other information.

```yaml
- name: BMI
    type: float
    label: Body Mass Index (kg/m2)
    derivation:
        compute:
            expr: "WEIGHTKG / POWER(NULLIF(HEIGHTCM, 0) / 100, 2)"
```

