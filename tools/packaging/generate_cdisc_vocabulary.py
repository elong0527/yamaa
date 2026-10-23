"""Generate core's closed CDISC vocabularies from their data file (P-VOC-1).

``yaml/vocabulary/cdisc.yaml`` lists the values of every closed CDISC
vocabulary in order. This writes each as an enum of core
(``rust/crates/yamaa-core/src/cdisc_vocabulary.rs``), named by :data:`TYPES`,
with one variant for each value in the file's order. A variant is named by its
value's words in upper camel case: ``Not Available`` is ``NotAvailable``,
``partialDate`` is ``PartialDate`` and ``URI`` is ``Uri``. Each enum gives every
value with its spelling (``VALUES``), a value's spelling (``name``) and the
value a spelling names (``from_name``).

A vocabulary only grows, so a value the file adds becomes a variant that every
exhaustive match over its enum must then place. The default mode compares the
committed file with the text regenerated from the data file, and ``--write``
regenerates it.
"""

import argparse
import re
import textwrap
from pathlib import Path

from tools.paths import ROOT
from tools.validation.check_cdisc_vocabulary import (
    VOCABULARY,
    document_errors,
    load_vocabulary,
    values,
)

MODULE = Path("rust/crates/yamaa-core/src/cdisc_vocabulary.rs")
TARGET = ROOT / MODULE
# The enum that holds each vocabulary in core.
TYPES = {
    "model": "ModelKind",
    "dataset_class": "DatasetClass",
    "dataset_subclass": "DatasetSubclass",
    "standard_name": "StandardName",
    "standard_status": "GuideStatus",
    "publishing_set": "PublishingSet",
    "origin_type": "OriginType",
    "origin_source": "OriginSource",
    "role": "ColumnRole",
    "core_designation": "Core",
    "submission_data_type": "DataType",
    "method_type": "MethodType",
    "page_reference_type": "PageType",
    "context": "DefineContext",
    "comparator": "WhereComparator",
}
# A word of a value: capitals before a capitalized word, a lower-case run after
# an optional capital, a run of capitals, or a run of digits.
WORD = re.compile(r"[A-Z]+(?=[A-Z][a-z])|[A-Z]?[a-z]+|[A-Z]+|[0-9]+")
IDENTIFIER = re.compile(r"[A-Z][A-Za-z0-9]*")
# Rust keywords that a capitalized word can spell.
RESERVED = frozenset({"Self"})


def vocabularies(root=ROOT):
    """Each vocabulary of the data file, by name, in the file's order."""
    document, errors = load_vocabulary(root)
    if document is not None:
        errors = document_errors(document)
    if errors:
        raise SystemExit("\n".join(errors))
    found = document["vocabularies"]
    problems = [
        f"ERROR: {VOCABULARY}.vocabularies.{name}: no enum in TYPES holds it"
        for name in found
        if name not in TYPES
    ] + [
        f"ERROR: TYPES.{name}: {VOCABULARY} has no such vocabulary"
        for name in TYPES
        if name not in found
    ]
    if problems:
        raise SystemExit("\n".join(problems))
    return found


def variant(value):
    """The variant that names ``value``: its words in upper camel case."""
    return "".join(word[0].upper() + word[1:].lower() for word in WORD.findall(value))


def literal(text):
    """A Rust string literal for printable ASCII text."""
    if not (text.isascii() and text.isprintable()):
        raise SystemExit(f"ERROR: {ascii(text)} is not printable ASCII")
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def variants(name, vocabulary):
    """Each value of a vocabulary with the variant that names it, in order."""
    pairs = [(value, variant(value)) for value in values(vocabulary)]
    label = f"{VOCABULARY}.vocabularies.{name}"
    problems = []
    seen = {}
    for value, item in pairs:
        if not IDENTIFIER.fullmatch(item) or item in RESERVED:
            problems.append(f"ERROR: {label}: {value!r} names no variant")
        elif item in seen:
            problems.append(
                f"ERROR: {label}: {seen[item]!r} and {value!r} both name {item}"
            )
        seen.setdefault(item, value)
    if problems:
        raise SystemExit("\n".join(problems))
    return pairs


def doc(name, vocabulary):
    """The doc comment of a vocabulary's enum."""
    if "type" in vocabulary:
        spelled = f"the schema type `{vocabulary['type']}`"
    else:
        spelled = f"the class field `{vocabulary['field']}`"
    text = f"The `{name}` vocabulary, which {spelled} spells"
    if "enumeration" in vocabulary:
        text += f", with the values of Define-XML's `{vocabulary['enumeration']}`"
    return [f"/// {line}" for line in textwrap.wrap(text + ".", width=76)]


def enum(name, vocabulary):
    """The enum of one vocabulary, and the impl that spells it."""
    rust = TYPES[name]
    pairs = variants(name, vocabulary)
    return [
        "",
        *doc(name, vocabulary),
        "#[rustfmt::skip]",
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
        f"pub enum {rust} {{",
        *(f"    {item}," for _, item in pairs),
        "}",
        "",
        "#[rustfmt::skip]",
        f"impl {rust} {{",
        "    /// Every value with its spelling, in the data file's order.",
        "    pub const VALUES: &'static [(&'static str, Self)] = &[",
        *(f"        ({literal(value)}, Self::{item})," for value, item in pairs),
        "    ];",
        "    /// The value's spelling.",
        "    pub fn name(self) -> &'static str {",
        "        match self {",
        *(f"            Self::{item} => {literal(value)}," for value, item in pairs),
        "        }",
        "    }",
        "    /// The value a spelling names, or `None` for any other text.",
        "    pub fn from_name(name: &str) -> Option<Self> {",
        "        Self::VALUES.iter().find(|(spelling, _)| *spelling == name).map(|&(_, value)| value)",
        "    }",
        "}",
    ]


def render(root=ROOT):
    """The generated module: the header, then each vocabulary's enum in the
    data file's order."""
    lines = [
        "// Generated by tools/packaging/generate_cdisc_vocabulary.py --write. Do not edit.",
        f"// Authoritative source: {VOCABULARY.as_posix()}.",
        "//! The closed CDISC vocabularies (P-VOC-1): an enum for each, with a",
        "//! variant for each of its values in the data file's order.",
    ]
    for name, vocabulary in vocabularies(root).items():
        lines += enum(name, vocabulary)
    return "\n".join([*lines, ""])


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args(argv)
    expected = render()
    if args.write:
        TARGET.write_text(expected, encoding="ascii")
        return
    committed = TARGET.read_text(encoding="ascii") if TARGET.exists() else None
    if committed != expected:
        raise SystemExit(
            f"ERROR: {MODULE.as_posix()} differs from "
            f"{VOCABULARY.as_posix()}; regenerate it with "
            "python -m tools.packaging.generate_cdisc_vocabulary --write"
        )
    print(f"PASS: core's CDISC vocabularies are {VOCABULARY.as_posix()}'s")


if __name__ == "__main__":
    main()
