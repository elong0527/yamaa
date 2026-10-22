"""The `values`, `tables`, `records` and `cli` sections of the host interface.

``rust/interface/bindings.yaml`` (format 2) declares the closed vocabularies
and table columns of the public surface, the records a host returns as a dict
or named list, and the command line. ``tools/packaging/generate_bindings.py``
validates these sections here and renders their lines of the normalized
manifest.

Each vocabulary in ``values`` and each table in ``tables`` names the facade
constant that owns it, as ``rust: yamaa::NAME``. The constant is read from the
facade sources under ``rust/crates/yamaa/src``, following an alias such as
``pub const ODM_COLUMNS: [&str; 30] = yamaa_core::odm_items::COLUMNS;`` into
the core or adapters crate, and the declaration must list exactly its strings,
in order. A value or column added in Rust therefore fails generation until it
is declared, and a declared one that Rust lacks fails too. The constant must be reachable
at the facade's crate root, where an embedder finds it.

``records`` and ``cli`` are validated against their grammar alone: a record's
fields are what a host builds, as ``info()`` and a selftest's ``info`` do from
the facade's ``Info``. ``cli`` declares the ``yamaa`` binary (#3060), whose
own vocabularies, such as the output formats, no facade constant owns;
``tools/tests/test_bindings_cli.py`` keeps it true to the binary's parser.
"""

import re

from tools.paths import WORKSPACE

FACADE = WORKSPACE / "crates" / "yamaa" / "src"
CORE = WORKSPACE / "crates" / "yamaa-core" / "src"
ADAPTERS = WORKSPACE / "crates" / "yamaa-adapters" / "src"
REGISTRY_VALUES = "yaml/conditions.yaml"

_IDENTIFIER = re.compile(r"[a-z][a-z0-9_]*")
_COLUMN = re.compile(r"[A-Za-z][A-Za-z0-9_]*")
_CONSTANT = re.compile(r"[A-Z][A-Z0-9_]*")
_COLUMN_TYPES = {"str", "int", "float", "date", "datetime"}
_FLAG = re.compile(r"--[a-z][a-z0-9-]*")
_COMMAND = re.compile(r"[a-z][a-z0-9-]*")
_ARGUMENT = re.compile(r"[A-Z][A-Z0-9_]*")


def _mapping(value, what):
    if not isinstance(value, dict):
        raise ValueError(f"{what} must be a mapping")
    return value


def _sequence(value, what, *, empty=False):
    if not isinstance(value, list) or (not value and not empty):
        raise ValueError(f"{what} must be a {'' if empty else 'nonempty '}list")
    return value


def _text(value, what):
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{what} must be nonempty text")
    return value


def _keys(entry, what, required, optional=()):
    entry = _mapping(entry, what)
    missing = set(required) - set(entry)
    if missing:
        raise ValueError(f"{what} is missing {sorted(missing)}")
    unknown = set(entry) - set(required) - set(optional)
    if unknown:
        raise ValueError(f"{what} has unknown keys {sorted(unknown)}")
    return entry


# --- Facade constants ----------------------------------------------------------


def _strip_comments(text):
    """Blank Rust comments, keeping string literals intact."""
    out = []
    i = 0
    while i < len(text):
        if text.startswith("//", i):
            newline = text.find("\n", i)
            i = len(text) if newline == -1 else newline
        elif text.startswith("/*", i):
            close = text.find("*/", i + 2)
            if close == -1:
                raise ValueError("unterminated block comment in Rust source")
            i = close + 2
        elif text[i] == '"':
            j = i + 1
            while j < len(text) and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append(text[i : j + 1])
            i = j + 1
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def _module_source(root, path):
    """The source of a module path below a crate root (`a::b` -> a/b.rs)."""
    *parents, leaf = path
    base = root.joinpath(*parents)
    for candidate in (base / f"{leaf}.rs", base / leaf / "mod.rs"):
        if candidate.is_file():
            return _strip_comments(candidate.read_text(encoding="utf-8"))
    raise ValueError(f"no Rust module {'::'.join(path)} under {root.name}")


def _definitions(source):
    """Every `const NAME: TYPE = VALUE;` of a source, public or not."""
    # The type may hold a bracketed `;`, as in `[&str; 30]`.
    pattern = re.compile(
        r"(?:pub(?:\([^)]*\))?\s+)?const\s+([A-Z][A-Z0-9_]*)\s*:"
        r"(?:\[[^\]]*\]|[^=;\[])*=\s*([^;]+);"
    )
    return {name: value.strip() for name, value in pattern.findall(source)}


def _strings(source, value, crate, seen):
    """The strings a constant expression names, following aliases."""
    definitions = _definitions(source)
    array = re.fullmatch(r"&?\[(.*)\]", value, re.S)
    if array:
        result = []
        for element in filter(None, (part.strip() for part in array[1].split(","))):
            literal = re.fullmatch(r'"([^"\\]*)"', element)
            if literal:
                result.append(literal[1])
            elif _CONSTANT.fullmatch(element) and element in definitions:
                single = re.fullmatch(r'"([^"\\]*)"', definitions[element])
                if not single:
                    raise ValueError(f"constant {element} is not one string literal")
                result.append(single[1])
            else:
                raise ValueError(f"unsupported constant element: {element}")
        return result
    path = value.split("::")
    # The crates a facade constant may alias, by their Rust names.
    crates = {"yamaa_core": CORE, "yamaa_adapters": ADAPTERS}
    if len(path) >= 3 and path[0] in crates and _CONSTANT.fullmatch(path[-1]):
        if value in seen:
            raise ValueError(f"constant alias cycle at {value}")
        target = _module_source(crates[path[0]], path[1:-1])
        found = _definitions(target).get(path[-1])
        if found is None:
            raise ValueError(f"{value} does not resolve to a constant")
        return _strings(target, found, path[0], seen | {value})
    raise ValueError(f"unsupported constant value in {crate}: {value}")


def _root_exports():
    lib = _strip_comments((FACADE / "lib.rs").read_text(encoding="utf-8"))
    names = set(_definitions(lib))
    for body in re.findall(r"pub use\s+[A-Za-z_][\w:]*::(\{[^}]*\}|\w+)\s*;", lib):
        names.update(re.findall(r"\w+", body))
    return names


def facade_constant(reference):
    """The strings of the facade constant `yamaa::NAME`, in order."""
    crate, _, name = reference.partition("::")
    if crate != "yamaa" or not _CONSTANT.fullmatch(name):
        raise ValueError(f"{reference} must name a facade constant as yamaa::NAME")
    if name not in _root_exports():
        raise ValueError(f"the facade does not export {name} at its crate root")
    found = []
    for path in sorted(FACADE.glob("*.rs")):
        source = _strip_comments(path.read_text(encoding="utf-8"))
        if re.search(rf"\bpub\s+const\s+{name}\s*:", source):
            found.append((source, _definitions(source)[name]))
    if len(found) != 1:
        raise ValueError(f"the facade defines {name} {len(found)} times, not once")
    source, value = found[0]
    return _strings(source, value, "yamaa", {reference})


# --- values, tables and records --------------------------------------------------


def _column_type(text, what):
    """Check a column type: a base type, or optional<T> or list<T>."""
    _text(text, what)
    match = re.fullmatch(r"(optional|list)<(.+)>", text)
    if match:
        if match[1] == "optional" and match[2].startswith("optional<"):
            raise ValueError(f"{what} nests optional types: {text}")
        _column_type(match[2], what)
    elif text not in _COLUMN_TYPES:
        raise ValueError(f"{what} has an unknown column type: {text}")


def validate_values(values, *, check_rust=True):
    for name, vocabulary in _mapping(values, "values").items():
        if not _IDENTIFIER.fullmatch(name):
            raise ValueError(f"invalid vocabulary name: {name!r}")
        what = f"values.{name}"
        entry = _keys(vocabulary, what, {"rust", "values"})
        listed = _sequence(entry["values"], f"{what}.values")
        for value in listed:
            if not isinstance(value, str) or not _IDENTIFIER.fullmatch(value):
                raise ValueError(f"{what} has an invalid value: {value!r}")
        if len(listed) != len(set(listed)):
            raise ValueError(f"{what} lists a value twice")
        if check_rust:
            owned = facade_constant(_text(entry["rust"], f"{what}.rust"))
            if owned != listed:
                raise ValueError(
                    f"{what} differs from {entry['rust']}: {listed} != {owned}"
                )


def _fields(fields, what, values):
    names = []
    for index, field in enumerate(_sequence(fields, what)):
        entry = _keys(
            field, f"{what}[{index}]", {"name", "type"}, {"values", "meaning"}
        )
        name = entry["name"]
        if not isinstance(name, str) or not _COLUMN.fullmatch(name):
            raise ValueError(f"{what}[{index}] has an invalid name: {name!r}")
        _column_type(entry["type"], f"{what}.{name}")
        if "values" in entry and entry["values"] not in {*values, REGISTRY_VALUES}:
            raise ValueError(
                f"{what}.{name} names an undeclared vocabulary: {entry['values']!r}"
            )
        if "meaning" in entry:
            _text(entry["meaning"], f"{what}.{name}.meaning")
        names.append(name)
    if len(names) != len(set(names)):
        raise ValueError(f"{what} names a field twice")
    return names


def validate_tables(tables, values, *, check_rust=True):
    for name, table in _mapping(tables, "tables").items():
        if not _IDENTIFIER.fullmatch(name):
            raise ValueError(f"invalid table name: {name!r}")
        what = f"tables.{name}"
        entry = _keys(table, what, {"rust", "columns"}, {"order", "owner"})
        for key in ("order", "owner"):
            if key in entry:
                _text(entry[key], f"{what}.{key}")
        columns = _fields(entry["columns"], f"{what}.columns", values)
        if check_rust:
            owned = facade_constant(_text(entry["rust"], f"{what}.rust"))
            if owned != columns:
                raise ValueError(
                    f"{what} columns differ from {entry['rust']}: {columns} != {owned}"
                )


def validate_records(records, values):
    for name, record in _mapping(records, "records").items():
        if not _IDENTIFIER.fullmatch(name):
            raise ValueError(f"invalid record name: {name!r}")
        what = f"records.{name}"
        entry = _keys(record, what, {"fields"}, {"owner"})
        if "owner" in entry:
            _text(entry["owner"], f"{what}.owner")
        _fields(entry["fields"], f"{what}.fields", values)


# --- cli ---------------------------------------------------------------------------

_CLI_REQUIRED = {
    "binary",
    "crate",
    "synopsis",
    "global_flags",
    "exit_status",
    "commands",
}
_CLI_OPTIONAL = {"depends_on", "inputs", "values", "streams", "interrupt", "json"}
_FLAG_OPTIONAL = {
    "value",
    "values",
    "default",
    "position",
    "effect",
    "valid_when",
    "host",
    "meaning",
}
_ARGUMENT_OPTIONAL = {"type", "values", "valid_when", "repeat", "meaning"}
_COMMAND_OPTIONAL = {"args", "flags", "effect", "text", "refusal_keys"}


def _flags(flags, what, values):
    names = []
    for index, flag in enumerate(_sequence(flags, what, empty=True)):
        entry = _keys(flag, f"{what}[{index}]", {"name"}, _FLAG_OPTIONAL)
        name = entry["name"]
        if not isinstance(name, str) or not _FLAG.fullmatch(name):
            raise ValueError(f"{what}[{index}] has an invalid flag name: {name!r}")
        if "value" in entry and not _ARGUMENT.fullmatch(str(entry["value"])):
            raise ValueError(f"{what}.{name} has an invalid value name")
        if "values" in entry and entry["values"] not in values:
            raise ValueError(f"{what}.{name} names an undeclared vocabulary")
        if "default" in entry and not isinstance(entry["default"], (str, int)):
            raise ValueError(f"{what}.{name} default must be text or an integer")
        for key in _FLAG_OPTIONAL - {"value", "values", "default"}:
            if key in entry:
                _text(entry[key], f"{what}.{name}.{key}")
        names.append(name)
    if len(names) != len(set(names)):
        raise ValueError(f"{what} names a flag twice")
    return names


def _arguments(arguments, what, values):
    for index, argument in enumerate(_sequence(arguments, what)):
        entry = _keys(argument, f"{what}[{index}]", {"name"}, _ARGUMENT_OPTIONAL)
        name = entry["name"]
        if not isinstance(name, str) or not _ARGUMENT.fullmatch(name):
            raise ValueError(f"{what}[{index}] has an invalid argument name: {name!r}")
        if "values" in entry and entry["values"] not in values:
            raise ValueError(f"{what}.{name} names an undeclared vocabulary")
        if "repeat" in entry and not isinstance(entry["repeat"], bool):
            raise ValueError(f"{what}.{name}.repeat must be a boolean")
        for key in ("type", "valid_when", "meaning"):
            if key in entry:
                _text(entry[key], f"{what}.{name}.{key}")


def _json_keys(keys, what, *, prose=True):
    """A list of JSON keys, or, where `prose` allows, text naming a document
    that has no fixed keys, such as the JSON Schema `schema` prints."""
    if prose and isinstance(keys, str):
        return _text(keys, what)
    for key in _sequence(keys, what):
        if not isinstance(key, str) or not _IDENTIFIER.fullmatch(key):
            raise ValueError(f"{what} has an invalid JSON key: {key!r}")
    if len(keys) != len(set(keys)):
        raise ValueError(f"{what} lists a JSON key twice")
    return keys


def _cli_values(entry, values):
    """The vocabularies a flag or argument may name: those of `values`, and
    the command line's own in `cli.values`, which no facade constant owns, so
    each lists its values alone."""
    own = _mapping(entry.get("values", {}), "cli.values")
    for name, listed in own.items():
        if not isinstance(name, str) or not _IDENTIFIER.fullmatch(name):
            raise ValueError(f"invalid cli.values name: {name!r}")
        if name in values:
            raise ValueError(f"cli.values.{name} shadows values.{name}")
        for value in _sequence(listed, f"cli.values.{name}"):
            if not isinstance(value, str) or not _IDENTIFIER.fullmatch(value):
                raise ValueError(f"cli.values.{name} has an invalid value: {value!r}")
        if len(listed) != len(set(listed)):
            raise ValueError(f"cli.values.{name} lists a value twice")
    return {**values, **own}


def validate_cli(cli, values, operations):
    """Check the command line's grammar; an empty mapping declares none."""
    if not _mapping(cli, "cli"):
        return
    entry = _keys(cli, "cli", _CLI_REQUIRED, _CLI_OPTIONAL)
    values = _cli_values(entry, values)
    if entry["binary"] != "yamaa":
        raise ValueError("cli.binary must be yamaa")
    if entry["crate"] != "rust/crates/yamaa-cli":
        raise ValueError("cli.crate must be rust/crates/yamaa-cli")
    for line in _sequence(entry["synopsis"], "cli.synopsis"):
        _text(line, "cli.synopsis line")
    for key in ("depends_on", "inputs", "interrupt"):
        if key in entry:
            _text(entry[key], f"cli.{key}")
    if "streams" in entry:
        streams = _keys(entry["streams"], "cli.streams", {"stdout", "stderr"})
        for name, text in streams.items():
            _text(text, f"cli.streams.{name}")
    _flags(entry["global_flags"], "cli.global_flags", values)
    statuses = _mapping(entry["exit_status"], "cli.exit_status")
    if not statuses:
        raise ValueError("cli.exit_status must declare at least one status")
    for status, meaning in statuses.items():
        if not isinstance(status, int) or isinstance(status, bool):
            raise ValueError(f"cli.exit_status key must be an integer: {status!r}")
        if not 0 <= status <= 125:
            raise ValueError(f"cli.exit_status {status} is outside 0 to 125")
        _text(meaning, f"cli.exit_status.{status}")
    commands = []
    for index, command in enumerate(_sequence(entry["commands"], "cli.commands")):
        what = f"cli.commands[{index}]"
        item = _keys(
            command,
            what,
            {"name", "synopsis", "operations", "json_keys"},
            _COMMAND_OPTIONAL,
        )
        name = item["name"]
        if not isinstance(name, str) or not _COMMAND.fullmatch(name):
            raise ValueError(f"{what} has an invalid command name: {name!r}")
        what = f"cli.commands.{name}"
        _text(item["synopsis"], f"{what}.synopsis")
        if not item["synopsis"].startswith(f"yamaa {name}"):
            raise ValueError(f"{what}.synopsis must start with 'yamaa {name}'")
        for operation in _sequence(item["operations"], f"{what}.operations"):
            if operation not in operations:
                raise ValueError(f"{what} names an unknown operation: {operation!r}")
        if "args" in item:
            _arguments(item["args"], f"{what}.args", values)
        if "flags" in item:
            _flags(item["flags"], f"{what}.flags", values)
        for key in ("effect", "text"):
            if key in item:
                _text(item[key], f"{what}.{key}")
        _json_keys(item["json_keys"], f"{what}.json_keys")
        # A refusal prints the command's own document, so its keys are named.
        if "refusal_keys" in item:
            _json_keys(item["refusal_keys"], f"{what}.refusal_keys", prose=False)
        commands.append(name)
    if len(commands) != len(set(commands)):
        raise ValueError("cli names a command twice")
    if "json" in entry:
        for name, keys in _mapping(entry["json"], "cli.json").items():
            if not _IDENTIFIER.fullmatch(name):
                raise ValueError(f"invalid cli.json entry: {name!r}")
            _json_keys(keys, f"cli.json.{name}")


# --- Manifest lines ------------------------------------------------------------------


def _field_facts(field):
    facts = f"{field['name']}: {field['type']}"
    if "values" in field:
        facts += f", values {field['values']}"
    return facts


def _flag_facts(flag):
    facts = [flag.get("value", "")] if "value" in flag else []
    if "values" in flag:
        facts.append(f"values {flag['values']}")
    if "default" in flag:
        facts.append(f"default {flag['default']}")
    return "; ".join(facts) or "flag"


def manifest_lines(definition):
    """The frozen elements of values, tables, records and cli, one per line."""
    lines = []
    for name, vocabulary in definition["values"].items():
        lines.extend(f"values.{name}.{value}" for value in vocabulary["values"])
    for name, table in definition["tables"].items():
        lines.extend(
            f"tables.{name}.columns[{index}]: {_field_facts(column)}"
            for index, column in enumerate(table["columns"])
        )
    for name, record in definition["records"].items():
        lines.extend(
            f"records.{name}.fields[{index}]: {_field_facts(field)}"
            for index, field in enumerate(record["fields"])
        )
    cli = definition["cli"]
    if not cli:
        return lines
    lines.extend(f"cli.synopsis: {line}" for line in cli["synopsis"])
    for name, listed in cli.get("values", {}).items():
        lines.extend(f"cli.values.{name}.{value}" for value in listed)
    lines.extend(
        f"cli.global_flags.{flag['name']}: {_flag_facts(flag)}"
        for flag in cli["global_flags"]
    )
    lines.extend(
        f"cli.exit_status.{status}: {meaning}"
        for status, meaning in sorted(cli["exit_status"].items())
    )
    for command in cli["commands"]:
        prefix = f"cli.commands.{command['name']}"
        lines.append(f"{prefix}: {command['synopsis']}")
        for index, argument in enumerate(command.get("args", [])):
            facts = argument.get("type", argument.get("values", "value"))
            if argument.get("repeat"):
                facts += ", repeated"
            lines.append(f"{prefix}.args[{index}]: {argument['name']}: {facts}")
        lines.extend(
            f"{prefix}.flags.{flag['name']}: {_flag_facts(flag)}"
            for flag in command.get("flags", [])
        )
        keys = command["json_keys"]
        if isinstance(keys, list):
            lines.extend(f"{prefix}.json_keys.{key}" for key in keys)
        lines.extend(
            f"{prefix}.refusal_keys.{key}" for key in command.get("refusal_keys", [])
        )
    for name, keys in cli.get("json", {}).items():
        if isinstance(keys, list):
            lines.extend(f"cli.json.{name}.{key}" for key in keys)
    return lines
