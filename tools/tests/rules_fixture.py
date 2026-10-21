"""A small repository with two card files, for rule tests."""

from pathlib import Path

INDEX = """\
# Rules

## Principles

**P1. Layers are ordered.** Layers compose in one order. Enforced by
[LAYER-1](inheritance.md#layer-1) and [TYPE-1](values/types.md#type-1).

## Rule files

| File | Prefix | Covers |
| --- | --- | --- |
| [Inheritance](inheritance.md) | LAYER | Resolve inherited layers. |
| [Types](values/types.md) | TYPE | Define column types. |

## Terminology

A [glossary](reference/glossary.md) is informative.
"""

CARDS = """\
# Inheritance

Layers compose into one specification.

## Layers

<a id="layer-1"></a>

### LAYER-1. Layers compose depth first

Each layer's parents are visited left to right, depth first, and then the
layer itself.

Fails: `inheritance_cycle`.
Evidence: `schema-inheritance`,
`rules/values/types.md::column`.

A cycle reports its path. A null clears under [LAYER-2](#layer-2).

<a id="layer-2"></a>

### LAYER-2. A null clears an inherited field

A null written over an inherited field clears it; see
[TYPE-1](values/types.md#type-1).
"""

TYPES = """\
# Types

## Columns

<a id="type-1"></a>

### TYPE-1. A column has one type

A column has one type.

<a id="type-2"></a>

### TYPE-2. A failed conversion fails

A value that does not convert to its column's type fails.

Fails: `conversion_failed`.
"""

CONDITIONS = """\
version: "1.0"
conditions:
  inheritance_cycle: {phases: [validation]}
  conversion_failed: {phases: [convert]}
"""

FILES = {
    "rules/README.md": INDEX,
    "rules/inheritance.md": CARDS,
    "rules/values/types.md": TYPES,
    "rules/reference/glossary.md": "# Glossary\n\nInformative terms.\n",
    "yaml/conditions.yaml": CONDITIONS,
    "benchmarks/schema-inheritance/README.md": "# Schema inheritance\n",
}


def write(root, relative, text):
    path = Path(root) / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


def write_repository(root):
    """Write the fixture repository under ``root``."""
    for relative, text in FILES.items():
        write(root, relative, text)
    return Path(root)
