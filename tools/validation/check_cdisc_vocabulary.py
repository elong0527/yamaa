"""Keep the closed CDISC vocabularies in one data file.

``yaml/vocabulary/cdisc.yaml`` holds the dataset classes and subclasses,
standard names, origin types and sources, and column roles that a
specification and a project file are read against (#3030 P-VOC-1). This check
keeps the file the one source of those lists:

- each vocabulary lists unique values, each with the language version that
  admitted it, none newer than the file's ``version``;
- a vocabulary that names a Define-XML ``enumeration`` equals that simple type
  of the Define-XML schema bundled with the engine, whose release and
  terminology package the file's ``source`` names;
- each schema set, the 0.5 bundles in ``yaml/`` and the 1.0 bundles in
  ``yaml/v1/``, spells a vocabulary as the named type it names, with exactly its
  values in its order, and the 1.0 bundles declare the file's ``version``.
"""

import argparse
import re
import sys
import xml.etree.ElementTree as ElementTree
from pathlib import Path

import yaml
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.repository_schema import SchemaSourceError
from tools.validation.schema_bundle import Bundle

VOCABULARY = Path("yaml/vocabulary/cdisc.yaml")
ENUMERATIONS = Path("rust/tests/schemas/define/2.1/define-enumerations.xsd")
SCHEMA_SETS = (Path("yaml"), Path("yaml/v1"))
VERSIONED_SET = Path("yaml/v1")
XSD = "{http://www.w3.org/2001/XMLSchema}"
VERSION = re.compile(r"1\.(?:0|[1-9][0-9]*)")
PACKAGE_DATE = re.compile(r"\(version (\d{4}-\d{2}-\d{2})\)")
DOCUMENT_KEYS = {"version", "source", "vocabularies"}
SOURCE_KEYS = {"define_xml", "terminology"}
VOCABULARY_KEYS = {"type", "enumeration", "values"}


def minor(version):
    """The minor number of a ``1.N`` version, which orders versions."""
    return int(version.split(".", 1)[1])


def load_vocabulary(root):
    """The data file, or the error that makes it unreadable."""
    path = root / VOCABULARY
    try:
        document = yaml.load(path.read_text(encoding="utf-8"), Loader=UniqueKeyLoader)
    except (OSError, UnicodeError, yaml.YAMLError, ValueError) as exc:
        return None, [f"ERROR: {VOCABULARY}: {exc}"]
    if not isinstance(document, dict):
        return None, [f"ERROR: {VOCABULARY}: expected a mapping"]
    return document, []


def document_errors(document):
    """The file's own shape: keys, versions and unique values."""
    errors = []
    for key in sorted(set(document) - DOCUMENT_KEYS):
        errors.append(f"ERROR: {VOCABULARY}.{key}: unknown field")
    version = document.get("version")
    if not (isinstance(version, str) and VERSION.fullmatch(version)):
        errors.append(f"ERROR: {VOCABULARY}.version: expected a quoted 1.N version")
        version = None
    source = document.get("source")
    if not isinstance(source, dict) or set(source) != SOURCE_KEYS:
        errors.append(
            f"ERROR: {VOCABULARY}.source: expected exactly {sorted(SOURCE_KEYS)}"
        )
    vocabularies = document.get("vocabularies")
    if not isinstance(vocabularies, dict) or not vocabularies:
        errors.append(f"ERROR: {VOCABULARY}.vocabularies: expected a mapping")
        return errors
    for name, vocabulary in vocabularies.items():
        label = f"{VOCABULARY}.vocabularies.{name}"
        if not isinstance(vocabulary, dict):
            errors.append(f"ERROR: {label}: expected a mapping")
            continue
        for key in sorted(set(vocabulary) - VOCABULARY_KEYS):
            errors.append(f"ERROR: {label}.{key}: unknown field")
        if not isinstance(vocabulary.get("type"), str):
            errors.append(f"ERROR: {label}.type: expected the schema type name")
        if "enumeration" in vocabulary and not isinstance(
            vocabulary["enumeration"], str
        ):
            errors.append(f"ERROR: {label}.enumeration: expected a simple type name")
        entries = vocabulary.get("values")
        if not isinstance(entries, list) or not entries:
            errors.append(f"ERROR: {label}.values: expected a non-empty list")
            continue
        seen = set()
        for index, entry in enumerate(entries):
            where = f"{label}.values[{index}]"
            if not isinstance(entry, dict) or set(entry) != {"value", "since"}:
                errors.append(f"ERROR: {where}: expected exactly value and since")
                continue
            value, since = entry["value"], entry["since"]
            if not isinstance(value, str) or not value:
                errors.append(f"ERROR: {where}.value: expected non-empty text")
            elif value in seen:
                errors.append(f"ERROR: {where}.value: {value!r} is listed twice")
            else:
                seen.add(value)
            if not (isinstance(since, str) and VERSION.fullmatch(since)):
                errors.append(f"ERROR: {where}.since: expected a quoted 1.N version")
            elif version is not None and minor(since) > minor(version):
                errors.append(
                    f"ERROR: {where}.since: {since} is newer than the file's "
                    f"version {version}"
                )
    return errors


def values(vocabulary):
    """The values a well-formed vocabulary lists, in order."""
    return [entry["value"] for entry in vocabulary["values"]]


def load_enumerations(root):
    """Each simple type of the bundled schema with its values, and its release."""
    tree = ElementTree.parse(root / ENUMERATIONS)
    schema = tree.getroot()
    enumerations = {
        simple.get("name"): [
            item.get("value") for item in simple.iter(f"{XSD}enumeration")
        ]
        for simple in schema.findall(f"{XSD}simpleType")
    }
    documentation = " ".join(
        " ".join((text or "").split())
        for text in (item.text for item in schema.iter(f"{XSD}documentation"))
    )
    package = PACKAGE_DATE.search(documentation)
    release = {
        "define_xml": schema.get("version"),
        "terminology": package[1] if package else None,
    }
    return enumerations, release


def enumeration_errors(document, enumerations, release):
    """Define-XML-backed vocabularies against the bundled schema."""
    errors = []
    for key in sorted(SOURCE_KEYS):
        if document["source"][key] != release[key]:
            errors.append(
                f"ERROR: {VOCABULARY}.source.{key}: {document['source'][key]!r}, "
                f"but {ENUMERATIONS} is {release[key]!r}"
            )
    for name, vocabulary in document["vocabularies"].items():
        enumeration = vocabulary.get("enumeration")
        if enumeration is None:
            continue
        label = f"{VOCABULARY}.vocabularies.{name}"
        published = enumerations.get(enumeration)
        if published is None:
            errors.append(
                f"ERROR: {label}.enumeration: {ENUMERATIONS} has no simple type "
                f"{enumeration}"
            )
        elif values(vocabulary) != published:
            errors.append(
                f"ERROR: {label}: values differ from {enumeration} in "
                f"{ENUMERATIONS}: {published}"
            )
    return errors


def schema_errors(root, document):
    """Each schema set spells each vocabulary with exactly its values."""
    errors = []
    for directory in SCHEMA_SETS:
        try:
            bundle = Bundle(root / directory)
        except SchemaSourceError as exc:
            errors.append(f"ERROR: {directory}: {exc}")
            continue
        if directory == VERSIONED_SET:
            for module, schema in sorted(bundle.modules.items()):
                if schema.get("version") != document["version"]:
                    errors.append(
                        f"ERROR: {directory / module}: version "
                        f"{schema.get('version')!r}, but {VOCABULARY} is "
                        f"{document['version']!r}"
                    )
        for name, vocabulary in document["vocabularies"].items():
            spelled = vocabulary["type"]
            label = f"{VOCABULARY}.vocabularies.{name}"
            alias = bundle.aliases.get(spelled)
            if alias is None:
                errors.append(f"ERROR: {label}.type: {directory} has no type {spelled}")
                continue
            listed = list(bundle.values(alias) or ())
            if listed != values(vocabulary):
                errors.append(
                    f"ERROR: {directory / bundle.module_of[spelled]}: {spelled} "
                    f"lists {listed}, but {label} lists {values(vocabulary)}"
                )
    return errors


def check(root):
    """Every error in the data file and its two comparisons."""
    root = Path(root)
    document, errors = load_vocabulary(root)
    if document is None:
        return errors
    errors = document_errors(document)
    if errors:
        return errors
    try:
        enumerations, release = load_enumerations(root)
    except (OSError, ElementTree.ParseError) as exc:
        return [f"ERROR: {ENUMERATIONS}: {exc}"]
    return [
        *enumeration_errors(document, enumerations, release),
        *schema_errors(root, document),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=ROOT)
    errors = check(parser.parse_args().root)
    for error in errors:
        print(error)
    if errors:
        return 1
    print(
        f"PASS: {VOCABULARY} matches {ENUMERATIONS} and the schema types that "
        "spell its vocabularies."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
