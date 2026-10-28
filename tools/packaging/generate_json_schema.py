"""Generate one JSON Schema per 1.0 file type from the schema files in yaml/.

Each JSON Schema is a view of one file type's schema bundle: a class becomes a
closed object, a union ``anyOf``, a registry a one-entry object, and every
field carries its kind and its schema-notation type as ``x-yamaa-kind`` and
``x-yamaa-type``, and a field a later language version introduced its
``since`` as ``x-yamaa-since``. A specification file may be one layer of an inheritance
chain, so the spec schema checks what one layer may write (LAYER-4), and its
``$defs/root_class`` checks a complete specification. The schemas are
committed beside their sources; without ``--write`` this check refuses a
stale or extra one.
"""

import argparse
import json

from tools.paths import ROOT
from tools.validation.schema_bundle import Bundle

SOURCE = ROOT / "yaml"
TARGET = SOURCE / "json"
DIALECT = "https://json-schema.org/draft/2020-12/schema"
# File type: the entry module of its bundle and its root class.
FILE_TYPES = {
    "spec": ("schema_spec.yaml", "root_class"),
    "project": ("schema_project.yaml", "project_class"),
    "codelists": ("schema_codelists.yaml", "codelist_file_class"),
    "function": ("schema_function.yaml", "function_definition_class"),
}
SCALARS = {
    "str": "string",
    "int": "integer",
    "float": "number",
    "bool": "boolean",
    "null": "null",
}
# Every layer writes the header (D4). Inside a member of these root
# collections, and inside these root blocks, which compose field by field as
# a member does, a layer defers requiredness to the composed specification,
# except the identity a member composes by (LAYER-4, LAYER-5).
HEADER = ("yamaa", "type")
COLLECTIONS = ("input", "windows", "intermediates", "columns", "rows")
BLOCKS = ("output", "submission")
IDENTITIES = {
    "column_class": "name",
    "column_copy_class": "copy",
    "intermediate_class": "id",
    "row_class": "id",
}
LAYER = "layer"
FRAGMENT = "fragment."
GENERATED = "Generated from yaml/ by tools/packaging/generate_json_schema.py."


def target(file_type):
    return TARGET / f"{file_type}.schema.json"


def plain(member):
    """Whether a rendered member is a bare scalar type, mergeable into a type list."""
    return set(member) == {"type"} and member["type"] in SCALARS.values()


class Renderer:
    """Render the named types one root reaches as JSON Schema definitions."""

    def __init__(self, bundle):
        self.bundle = bundle
        self.definitions = {}

    def reference(self, name, fragment):
        fragment = fragment and self.bundle.holds_class(name)
        key = FRAGMENT + name if fragment else name
        if key not in self.definitions:
            self.definitions[key] = None  # Reserved while a recursive type renders.
            self.definitions[key] = self.named(name, fragment)
        return {"$ref": f"#/$defs/{key}"}

    def named(self, name, fragment):
        if name in self.bundle.classes:
            return self.object(name, self.bundle.fields(name), fragment)
        alias = self.bundle.aliases[name]
        if "registry" in alias:
            return self.registry(alias["registry"], fragment)
        return self.descriptor(alias, fragment)

    def type(self, item, fragment):
        if item.name in SCALARS:
            return {"type": SCALARS[item.name]}
        if item.name == "list":
            return {"type": "array", "items": self.type(item.arguments[0], fragment)}
        if item.name == "dict":
            return {
                "type": "object",
                "propertyNames": self.type(item.arguments[0], False),
                "additionalProperties": self.type(item.arguments[1], fragment),
            }
        return self.reference(item.name, fragment)

    def descriptor(self, descriptor, fragment):
        """One descriptor: its type or union, then its constraints."""
        members = [
            self.type(item, fragment) for item in self.bundle.members(descriptor)
        ]
        # Plain scalar members merge into one type list, where the first one stood.
        scalars = [member["type"] for member in members if plain(member)]
        union, merged = [], False
        for member in members:
            if not plain(member):
                union.append(member)
            elif not merged:
                union.append({"type": scalars if len(scalars) > 1 else scalars[0]})
                merged = True
        schema = dict(union[0]) if len(union) == 1 else {"anyOf": union}
        if "values" in descriptor:
            schema["enum"] = list(descriptor["values"])
        if "pattern" in descriptor:
            schema["pattern"] = descriptor["pattern"]
        if "min_length" in descriptor:
            schema["minLength"] = descriptor["min_length"]
        if "size" in descriptor:
            unit = "Items" if schema.get("type") == "array" else "Properties"
            schema[f"min{unit}"] = schema[f"max{unit}"] = descriptor["size"]
        if "default" in descriptor:
            schema["default"] = descriptor["default"]
        if "description" in descriptor:
            schema["description"] = " ".join(descriptor["description"].split())
        return schema

    def properties(self, owner, fields, fragment_of):
        properties = {}
        for item in fields:
            schema = self.descriptor(item.descriptor, fragment_of(item))
            schema["x-yamaa-kind"] = self.bundle.descriptor_kind(
                item.descriptor, f"{owner}.{item.name}"
            )
            schema["x-yamaa-type"] = item.descriptor["type"]
            if "since" in item.descriptor:
                schema["x-yamaa-since"] = item.descriptor["since"]
            properties[item.name] = schema
        return properties

    def object(self, owner, fields, fragment):
        if fragment:
            required = [IDENTITIES[owner]] if owner in IDENTITIES else []
        else:
            required = [item.name for item in fields if item.descriptor.get("required")]
        schema = {
            "type": "object",
            "properties": self.properties(owner, fields, lambda item: fragment),
        }
        if required:
            schema["required"] = required
        schema["additionalProperties"] = False
        return schema

    def registry(self, registry, fragment):
        properties = {}
        for keyword, payload in self.bundle.registries[registry].items():
            if isinstance(payload, list):
                fields = self.bundle.payload_fields(registry, keyword)
                properties[keyword] = self.object(
                    f"{registry}.{keyword}", fields, fragment
                )
            else:
                properties[keyword] = self.descriptor(payload, fragment)
        return {
            "type": "object",
            "properties": properties,
            "additionalProperties": False,
            "minProperties": 1,
            "maxProperties": 1,
        }

    def layer(self, root):
        """What one specification layer may write (LAYER-4, LAYER-7)."""
        fields = self.bundle.fields(root)
        return {
            "type": "object",
            "properties": self.properties(
                root, fields, lambda item: item.name in COLLECTIONS + BLOCKS
            ),
            "required": list(HEADER),
            "additionalProperties": False,
        }


def render(file_type):
    """The JSON Schema text of one file type."""
    entry, root = FILE_TYPES[file_type]
    renderer = Renderer(Bundle(SOURCE, entry))
    if file_type == "spec":
        renderer.definitions[LAYER] = renderer.layer(root)
        renderer.reference(root, False)
        start = LAYER
        description = (
            "One file of the 1.0 spec type. A specification file may be one layer "
            "of an inheritance chain: the header is required in every layer, a "
            "root field it writes is complete there, and inside a member of input, "
            "windows, intermediates, columns or rows, and inside output or "
            "submission, requiredness waits for the composed specification, which "
            "$defs/root_class checks (LAYER-4). "
        )
    else:
        renderer.reference(root, False)
        start = root
        description = f"One file of the 1.0 {file_type} type. "
    document = {
        "$schema": DIALECT,
        "title": f"yamaa 1.0 {file_type} file",
        "description": description
        + "Each field carries its kind as x-yamaa-kind and its schema-notation "
        "type as x-yamaa-type. " + GENERATED,
        "$ref": f"#/$defs/{start}",
        "$defs": dict(sorted(renderer.definitions.items())),
    }
    return json.dumps(document, indent=2, ensure_ascii=True) + "\n"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args(argv)
    expected = {target(file_type): render(file_type) for file_type in FILE_TYPES}
    if args.write:
        TARGET.mkdir(parents=True, exist_ok=True)
        for path, text in expected.items():
            path.write_bytes(text.encode("ascii"))
        return 0
    errors = []
    for path, text in expected.items():
        if not path.is_file() or path.read_bytes() != text.encode("ascii"):
            errors.append(f"ERROR: regenerate {path.relative_to(ROOT).as_posix()}")
    for path in sorted(TARGET.iterdir()) if TARGET.is_dir() else ():
        if path not in expected:
            errors.append(
                f"ERROR: {path.relative_to(ROOT).as_posix()} names no file type"
            )
    for error in errors:
        print(error)
    if errors:
        print(
            "Run: python -m tools.packaging.generate_json_schema --write, then "
            "python -m tools.packaging.generate_shipped_schema --write"
        )
        return 1
    print("PASS: the 1.0 JSON Schemas match the schema files in yaml/")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
