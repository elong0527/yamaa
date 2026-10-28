"""Generate core's expression table from the declarations of its functions.

Two declarations state the expression functions (#3800):

- the ``vocabulary`` of ``yaml/grammar/expression.yaml`` states each function
  of the closed grammar with its argument bounds and its ``result``, and each
  reducer with the argument forms it takes;
- the ``expressions`` registry of the ``yaml/schema_expression_*.yaml``
  modules states each registered expression, a verb, with its arguments,
  their types, their defaults and the values they admit.

This writes both as Rust, ``rust/crates/yamaa-core/src/expression_table.rs``,
the one place the engine spells a function's name, argument bounds,
arguments, defaults and admitted contexts, and the enumerations that the
lowering and the executor dispatch on:

- ``Function`` and ``Reducer``, a variant for each name in the grammar's
  order, with the bounds, result and contexts of a function and whether a
  reducer takes the record star. A function's result decides where it
  stands (FORMULA-27): a number in every context, a truth value only in the
  predicate context, whose root is one.
- ``Verb``, a variant for each registered expression in the registry's
  order, with its form, its operand fields, its handlers, its shorthand
  field (SCHEMA-10) and its defaults. A verb whose arguments include a
  ``window_selection`` is a window verb, one of :data:`STRUCTURES` is a
  structure the lowering builds by hand, and every other is a scalar verb.
- ``ScalarOp`` and ``WindowFunction``, the written parameters of each scalar
  and window verb, read by ``read`` as the declaration types them, with
  ``Slot`` naming each operand field and ``Handlers`` each handler field.
- An enum for each closed list of values a parameter takes: a named value
  type's (``temporal_type`` is ``TemporalType``), or an inline list's, named
  by :data:`VALUE_ENUMS` after its field and holding the values of every
  field of that name.

An argument's role follows its kind (yaml/README.md): a ``name`` is an
operand, an ``expression`` text is a template whose placeholders are the
operands, a ``substitute_value`` is a handler, a ``window_selection`` is the
window, and an ``identifier``, which names a declared item such as a
codelist, and any other ``value`` are parameters. A name is a variant's,
field's or slot's spelling in upper camel case by its ``_``-separated words,
``LOG10`` being ``Log10`` and ``mdy_slash`` ``MdySlash``; a spelling that
joins its words without ``_``, such as ``NULLIF`` or ``datetime``, names them
in :data:`JOINED`, and a field ``type`` is ``kind``, a Rust keyword
otherwise.

The generator also writes the table of expression modules in
``docs/articles/schema.md``, between its markers, each module with the
expressions it registers and, for one it registers alone, the note
:data:`NOTES` gives it. The default mode compares both with the text
regenerated from the declarations, and ``--write`` regenerates them.
"""

import argparse
import re
import textwrap
from dataclasses import dataclass
from pathlib import Path

import yaml
from tools.paths import ROOT
from tools.validation._repository_yaml import UniqueKeyLoader
from tools.validation.schema_bundle import BUILTINS, Bundle

GRAMMAR = Path("yaml/grammar/expression.yaml")
SCHEMA = Path("yaml")
ENTRY = "schema_spec.yaml"
REGISTRY = "expressions"
MODULE = Path("rust/crates/yamaa-core/src/expression_table.rs")
PAGE = Path("docs/articles/schema.md")
START = "<!-- expression-modules:start -->"
END = "<!-- expression-modules:end -->"
WRITE = "python -m tools.packaging.generate_expression_table --write"
# The verbs whose node is a structure of its own, which the lowering builds
# by hand.
STRUCTURES = (
    "source",
    "first_available",
    "case",
    "flag",
    "aggregate",
    "odm",
    "function",
)
# The enum of each inline list of values a parameter takes, by field name.
VALUE_ENUMS = {
    "unit": "TimeUnit",
    "closed": "Closed",
    "to": "TextCase",
    "part": "FlagPart",
    "method": "RankMethod",
}
# The words of each declared spelling that joins them without `_`.
JOINED = {
    "NULLIF": "NullIf",
    "STDDEV": "StdDev",
    "GEOMEAN": "GeoMean",
    "datetime": "DateTime",
    "ddmonyyyy": "DdMonYyyy",
    "monyyyy": "MonYyyy",
}
# What the schema page says of an expression its module registers alone,
# after its name.
NOTES = {
    "odm": "which reads an item of the collected ODM data",
    "function": "which calls a project function",
}
# The named types an argument's role is read from.
HANDLER = "substitute_value"
WINDOW = "window_selection"
TEMPLATE = "string_template"
DICTIONARY = "mapping_dict"
# The context whose root is a truth value (FORMULA-27).
TRUTH = "predicate"
FIELD_NAMES = {"type": "kind"}
VARIANT = re.compile(r"[A-Z][A-Za-z0-9]*")


class DeclarationError(SystemExit):
    """A declaration the generator cannot write as Rust."""

    def __init__(self, message):
        super().__init__(f"ERROR: {message}")


def camel(name):
    """``name``'s words in upper camel case: its ``_``-separated words, or
    those :data:`JOINED` gives it."""
    if name in JOINED:
        return JOINED[name]
    variant = "".join(word[:1].upper() + word[1:].lower() for word in name.split("_"))
    if not VARIANT.fullmatch(variant):
        raise DeclarationError(f"{name!r} names no Rust variant")
    return variant


def field_name(name):
    """The Rust field that holds the argument ``name``."""
    return FIELD_NAMES.get(name, name)


def literal(text):
    """A Rust string literal for printable ASCII text."""
    text = str(text)
    if not (text.isascii() and text.isprintable()):
        raise DeclarationError(f"{ascii(text)} is not printable ASCII")
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def phrase(names):
    """``names`` as code spans joined as prose: ``a``, ``b`` and ``c``."""
    spans = [f"`{name}`" for name in names]
    return spans[0] if len(spans) == 1 else ", ".join(spans[:-1]) + " and " + spans[-1]


def doc(text, indent=""):
    """A doc comment wrapped at the generated module's width."""
    width = 76 - len(indent)
    return [f"{indent}/// {line}" for line in textwrap.wrap(text, width=width)]


# -- the declarations ----------------------------------------------------


@dataclass(frozen=True)
class Function:
    name: str
    minimum: int
    maximum: int | None
    result: str


@dataclass(frozen=True)
class Reducer:
    name: str
    forms: tuple


@dataclass(frozen=True)
class Values:
    """A closed list of values, the enum that spells it and where it is read."""

    rust: str
    values: tuple
    source: str


@dataclass(frozen=True)
class Parameter:
    """How a parameter is held and read: its Rust type, the reader method of
    ``Written`` and the enum, list or union its text is spelled in."""

    rust: str
    reader: str
    values: Values | None = None
    union: tuple | None = None


@dataclass(frozen=True)
class Argument:
    name: str
    role: str
    required: bool
    default: object
    form: str | None = None
    parameter: Parameter | None = None


@dataclass(frozen=True)
class Verb:
    name: str
    module: str
    form: str
    arguments: tuple
    shorthand: str | None


def grammar(root=ROOT):
    """The grammar's functions, reducers and contexts, in the file's order."""
    document = yaml.load(
        (root / GRAMMAR).read_text(encoding="ascii"), Loader=UniqueKeyLoader
    )
    vocabulary = document["vocabulary"]
    functions = [
        Function(name, entry["min_arguments"], entry["max_arguments"], entry["result"])
        for name, entry in vocabulary["function"].items()
    ]
    reducers = [
        Reducer(name, tuple(entry["argument"]))
        for name, entry in vocabulary["reducer"].items()
    ]
    contexts = list(document["contexts"])
    if TRUTH not in contexts:
        raise DeclarationError(f"{GRAMMAR.as_posix()}: no {TRUTH} context")
    for item in [*functions, *reducers]:
        camel(item.name)
    return functions, reducers, contexts


class Declarations:
    """The registered expressions of the specification's schema bundle."""

    def __init__(self, root=ROOT):
        self.bundle = Bundle(root / SCHEMA, ENTRY)
        self.values = {}
        self.unions = {}
        entries = self.bundle.registries[REGISTRY]
        missing = [name for name in STRUCTURES if name not in entries]
        if missing:
            raise DeclarationError(
                f"STRUCTURES names {missing[0]}, which no module declares"
            )
        self.verbs = [self.verb(name, payload) for name, payload in entries.items()]

    def module(self, name):
        for module, document in self.bundle.modules.items():
            if name in document.get(REGISTRY, {}):
                return module
        raise DeclarationError(f"no module declares {name}")

    def payload(self, name, payload):
        """A verb's arguments, and the field a bare payload fills (SCHEMA-10)."""
        if isinstance(payload, list):
            return self.bundle.payload_fields(REGISTRY, name), None
        members = self.bundle.members(payload)
        classes = [item.name for item in members if item.name in self.bundle.classes]
        if len(classes) != 1:
            return [], None
        fields = self.bundle.fields(classes[0])
        others = [item for item in members if item.name != classes[0]]
        required = [item for item in fields if item.descriptor.get("required")]
        shorthand = None
        if len(others) == 1 and len(required) == 1:
            if self.bundle.members(required[0].descriptor) == others:
                shorthand = required[0].name
        return fields, shorthand

    def verb(self, name, payload):
        fields, shorthand = self.payload(name, payload)
        module = self.module(name)
        types = {
            item.name
            for field in fields
            for item in self.bundle.members(field.descriptor)
        }
        if name in STRUCTURES:
            form = "structure"
        elif WINDOW in types:
            form = "window"
        else:
            form = "scalar"
        arguments = tuple(self.argument(name, form, field) for field in fields)
        return Verb(name, module, form, arguments, shorthand)

    def argument(self, verb, form, field):
        descriptor = field.descriptor
        label = f"{REGISTRY}.{verb}.{field.name}"
        members = self.bundle.members(descriptor)
        names = [item.name for item in members]
        required = bool(descriptor.get("required"))
        default = descriptor.get("default")
        kind = self.bundle.descriptor_kind(descriptor, label)
        if names == [HANDLER]:
            return Argument(field.name, "handler", required, default)
        if names == [WINDOW]:
            return Argument(field.name, "window", required, default)
        if form == "structure":
            return Argument(field.name, kind, required, default)
        if kind == "name":
            return Argument(
                field.name, "operand", required, default, form=self.operand(members)
            )
        if kind == "expression":
            if names != [TEMPLATE]:
                raise DeclarationError(
                    f"{label}: an expression argument is a {TEMPLATE}"
                )
            parameter = Parameter("String", "text")
            return Argument(
                field.name,
                "operand",
                required,
                default,
                form="Placeholders",
                parameter=parameter,
            )
        if kind not in ("value", "identifier"):
            raise DeclarationError(f"{label}: a {kind} argument has no role")
        parameter = self.parameter(verb, field.name, descriptor, label)
        return Argument(field.name, "parameter", required, default, parameter=parameter)

    def operand(self, members):
        """How the lowering reads an operand field."""
        if len(members) == 1 and members[0].name == "list":
            return "Names"
        if any(self.bundle.holds_class(item.name) for item in members):
            return "Binding"
        return "Name"

    def enum(self, rust, values, source):
        found = self.values.get(rust)
        if found is None:
            self.values[rust] = Values(rust, tuple(values), source)
        else:
            merged = (
                *found.values,
                *(value for value in values if value not in found.values),
            )
            self.values[rust] = Values(rust, merged, found.source)
        return self.values[rust]

    def named_values(self, name, label):
        """The enum of a named value type that lists its values."""
        alias = self.bundle.aliases.get(name)
        if alias is None or "values" not in alias:
            return None
        return self.enum(camel(name), alias["values"], f"the schema type `{name}`")

    def scalar(self, item, label):
        """The Rust type and reader of one member of a parameter's type."""
        if item.name in ("int", "str", "bool"):
            return {
                "int": ("String", "int"),
                "str": ("String", "text"),
                "bool": ("bool", "boolean"),
            }[item.name]
        alias = self.bundle.aliases.get(item.name)
        if alias is None or "registry" in alias:
            raise DeclarationError(f"{label}: no reader for {item}")
        if item.name == DICTIONARY:
            return ("Dictionary", "dictionary")
        members = self.bundle.members(alias)
        if len(members) == 1 and members[0].name == "str" and "values" not in alias:
            return ("String", "text")
        raise DeclarationError(f"{label}: no reader for {item}")

    def parameter(self, verb, name, descriptor, label):
        members = self.bundle.members(descriptor)
        if len(members) == 1:
            (item,) = members
            if item.name == "str" and "values" in descriptor:
                rust = VALUE_ENUMS.get(name)
                if rust is None:
                    raise DeclarationError(
                        f"{label}: VALUE_ENUMS names no enum for {name}"
                    )
                values = self.enum(rust, descriptor["values"], f"a field `{name}`")
                return Parameter(rust, "text", values=values)
            if item.name == "list":
                (element,) = item.arguments
                values = self.named_values(element.name, label)
                if values is not None:
                    return Parameter(f"Vec<{values.rust}>", "texts", values=values)
                if element.name == "float":
                    return Parameter("Vec<f64>", "numbers")
                if element.name == "str":
                    return Parameter("Vec<String>", "texts")
                raise DeclarationError(f"{label}: no reader for {item}")
            values = self.named_values(item.name, label)
            if values is not None:
                return Parameter(values.rust, "text", values=values)
            rust, reader = self.scalar(item, label)
            return Parameter(rust, reader)
        # A union of a builtin and a closed list: an enum of the two.
        builtins = [item for item in members if item.name in BUILTINS]
        lists = [self.named_values(item.name, label) for item in members]
        lists = [values for values in lists if values is not None]
        if len(members) != 2 or len(builtins) != 1 or len(lists) != 1:
            raise DeclarationError(f"{label}: no reader for its union")
        rust = camel(verb) + camel(name)
        builtin, values = builtins[0].name, lists[0]
        held, reader = self.scalar(builtins[0], label)
        union = ((camel(builtin), held, reader), (values.rust, values.rust, "text"))
        self.unions[rust] = (union, label, values)
        return Parameter(rust, "union", values=values, union=union)


# -- the Rust module -----------------------------------------------------


def spelled(rust, summary, spellings, order, lookup=False):
    """An enum with a variant for each spelling, then every variant in
    ``order``, each variant's spelling and the variant a spelling names: by
    ``lookup`` in any letter case, else by ``from_name`` exactly."""
    variants = [(spelling, camel(spelling)) for spelling in spellings]
    seen = {}
    for spelling, variant in variants:
        if variant in seen:
            raise DeclarationError(
                f"{rust}: {seen[variant]!r} and {spelling!r} both name {variant}"
            )
        seen[variant] = spelling
    if lookup:
        find = [
            "    /// The variant a spelling names in any letter case.",
            "    pub(crate) fn lookup(name: &str) -> Option<Self> {",
            "        Self::ALL.into_iter().find(|value| name.eq_ignore_ascii_case(value.name()))",
            "    }",
        ]
    else:
        find = [
            "    /// The variant a spelling names, or `None` for any other text.",
            "    pub fn from_name(name: &str) -> Option<Self> {",
            "        Self::ALL.into_iter().find(|value| value.name() == name)",
            "    }",
        ]
    return [
        "",
        *doc(summary),
        "#[rustfmt::skip]",
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
        f"pub enum {rust} {{",
        *(f"    {variant}," for _, variant in variants),
        "}",
        "",
        "#[rustfmt::skip]",
        f"impl {rust} {{",
        f"    /// Every variant, in {order}.",
        f"    pub const ALL: [Self; {len(variants)}] = [",
        *(f"        Self::{variant}," for _, variant in variants),
        "    ];",
        "    /// The variant's spelling.",
        "    pub const fn name(self) -> &'static str {",
        "        match self {",
        *(
            f"            Self::{variant} => {literal(spelling)},"
            for spelling, variant in variants
        ),
        "        }",
        "    }",
        *find,
        "}",
    ]


def functions_rust(functions, contexts):
    results = {item.result for item in functions}
    unknown = sorted(results - {"numeric", "boolean"})
    if unknown:
        raise DeclarationError(f"{GRAMMAR.as_posix()}: no Rust result {unknown[0]}")
    every = ", ".join(f"Context::{camel(context)}" for context in contexts)
    return [
        *spelled(
            "Function",
            "A function of the grammar's closed vocabulary (FORMULA-8, FORMULA-13).",
            [item.name for item in functions],
            "the grammar file's order",
            lookup=True,
        ),
        "",
        "/// What a function's call stands for.",
        "#[rustfmt::skip]",
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
        "pub enum FunctionResult {",
        "    /// A number, which stands where a value does.",
        "    Numeric,",
        "    /// A truth value, which stands where a predicate needs one.",
        "    Boolean,",
        "}",
        "",
        "#[rustfmt::skip]",
        "impl Function {",
        "    /// The fewest arguments a call takes.",
        "    pub const fn min_arguments(self) -> usize {",
        "        match self {",
        *(
            f"            Self::{camel(item.name)} => {item.minimum},"
            for item in functions
        ),
        "        }",
        "    }",
        "    /// The most arguments a call takes, or `None` without a bound.",
        "    pub const fn max_arguments(self) -> Option<usize> {",
        "        match self {",
        *(
            f"            Self::{camel(item.name)} => "
            + ("None" if item.maximum is None else f"Some({item.maximum})")
            + ","
            for item in functions
        ),
        "        }",
        "    }",
        "    /// Whether a call may write `count` arguments.",
        "    pub const fn admits(self, count: usize) -> bool {",
        "        count >= self.min_arguments()",
        "            && match self.max_arguments() {",
        "                Some(most) => count <= most,",
        "                None => true,",
        "            }",
        "    }",
        "    /// What a call stands for.",
        "    pub const fn result(self) -> FunctionResult {",
        "        match self {",
        *(
            f"            Self::{camel(item.name)} => FunctionResult::{camel(item.result)},"
            for item in functions
        ),
        "        }",
        "    }",
        "    /// The contexts that admit a call: every context for a number, and",
        "    /// for a truth value the one whose root is a truth value (FORMULA-27).",
        "    pub const fn contexts(self) -> &'static [Context] {",
        "        match self.result() {",
        f"            FunctionResult::Numeric => &[{every}],",
        f"            FunctionResult::Boolean => &[Context::{camel(TRUTH)}],",
        "        }",
        "    }",
        "}",
    ]


def reducers_rust(reducers):
    return [
        *spelled(
            "Reducer",
            "A reducer of the grammar's closed vocabulary (FORMULA-18); only the "
            "aggregate context admits a reduction.",
            [item.name for item in reducers],
            "the grammar file's order",
            lookup=True,
        ),
        "",
        "#[rustfmt::skip]",
        "impl Reducer {",
        "    /// Whether the reducer takes the record star `D.*` as its argument.",
        "    pub const fn takes_star(self) -> bool {",
        "        match self {",
        *(
            f"            Self::{camel(item.name)} => {'true' if 'star' in item.forms else 'false'},"
            for item in reducers
        ),
        "        }",
        "    }",
        "}",
    ]


def verbs_rust(declarations):
    verbs = declarations.verbs
    lines = [
        *spelled(
            "Verb",
            "An expression of the `expressions` registry (EXPR-1), spelled by "
            "the keyword a payload writes.",
            [verb.name for verb in verbs],
            "the registry's order",
        ),
        "",
        "/// The node a verb lowers to.",
        "#[rustfmt::skip]",
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
        "pub enum Form {",
        "    /// A row-wise verb over its operands, an `ExprKind::Scalar`.",
        "    Scalar,",
        "    /// A verb over a window of rows, an `ExprKind::Window`.",
        "    Window,",
        "    /// A structure of its own, which the lowering builds by hand.",
        "    Structure,",
        "}",
        "",
        "/// How the lowering reads an operand field.",
        "#[rustfmt::skip]",
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
        "pub enum OperandForm {",
        "    /// A name, or a written number (`shift.days`, `shift.amount`).",
        "    Name,",
        "    /// A list of names, each one operand.",
        "    Names,",
        "    /// A source binding: a name, or a variable with its record selection.",
        "    Binding,",
        "    /// A template, whose placeholders are the operands.",
        "    Placeholders,",
        "}",
        "",
        "/// One operand field of a verb, in the verb's declared order.",
        "#[rustfmt::skip]",
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
        "pub struct OperandField {",
        "    pub field: &'static str,",
        "    pub slot: Slot,",
        "    pub required: bool,",
        "    pub form: OperandForm,",
        "}",
        "",
        "/// The default a declaration gives an argument a payload omits.",
        "#[rustfmt::skip]",
        "#[derive(Clone, Copy, Debug, PartialEq, Eq)]",
        "pub enum DeclaredDefault {",
        "    Text(&'static str),",
        "    /// An integer as written.",
        "    Integer(&'static str),",
        "    Boolean(bool),",
        "    /// An empty list or mapping.",
        "    Empty,",
        "}",
        "",
        "#[rustfmt::skip]",
        "impl Verb {",
        "    /// The node the verb lowers to.",
        "    pub const fn form(self) -> Form {",
        "        match self {",
    ]
    for form in ("scalar", "window", "structure"):
        named = [f"Self::{camel(verb.name)}" for verb in verbs if verb.form == form]
        if named:
            lines += wrap_arm(named, f"Form::{camel(form)}", "            ")
    lines += [
        "        }",
        "    }",
        "    /// The field a bare payload fills (SCHEMA-10), if the verb has one.",
        "    pub const fn shorthand(self) -> Option<&'static str> {",
        "        match self {",
    ]
    for verb in verbs:
        if verb.shorthand:
            lines.append(
                f"            Self::{camel(verb.name)} => Some({literal(verb.shorthand)}),"
            )
    lines += [
        "            _ => None,",
        "        }",
        "    }",
        "    /// The handler fields the verb declares, in declared order.",
        "    pub const fn handlers(self) -> &'static [&'static str] {",
        "        match self {",
    ]
    for verb in verbs:
        handlers = [item.name for item in verb.arguments if item.role == "handler"]
        if handlers:
            written = ", ".join(literal(name) for name in handlers)
            lines.append(f"            Self::{camel(verb.name)} => &[{written}],")
    lines += [
        "            _ => &[],",
        "        }",
        "    }",
        "    /// The operand fields of a scalar or window verb, in declared order.",
        "    pub const fn operands(self) -> &'static [OperandField] {",
        "        match self {",
    ]
    for verb in verbs:
        operands = [item for item in verb.arguments if item.role == "operand"]
        if verb.form == "structure" or not operands:
            continue
        lines.append(f"            Self::{camel(verb.name)} => &[")
        for item in operands:
            lines.append(
                f"                OperandField {{ field: {literal(item.name)}, "
                f"slot: Slot::{camel(item.name)}, "
                f"required: {'true' if item.required else 'false'}, "
                f"form: OperandForm::{item.form} }},"
            )
        lines.append("            ],")
    lines += [
        "            _ => &[],",
        "        }",
        "    }",
        "    /// The default the declaration gives `field`, if it gives one.",
        "    pub fn default_of(self, field: &str) -> Option<DeclaredDefault> {",
        "        Some(match (self, field) {",
    ]
    for verb in verbs:
        for item in verb.arguments:
            if item.default is None:
                continue
            lines.append(
                f"            (Self::{camel(verb.name)}, {literal(item.name)}) => "
                f"{default_rust(item.default, verb.name, item.name)},"
            )
    lines += [
        "            _ => return None,",
        "        })",
        "    }",
        "}",
    ]
    return lines


def default_rust(default, verb, name):
    if isinstance(default, bool):
        return f"DeclaredDefault::Boolean({'true' if default else 'false'})"
    if isinstance(default, int):
        return f"DeclaredDefault::Integer({literal(default)})"
    if isinstance(default, str):
        return f"DeclaredDefault::Text({literal(default)})"
    if default in ({}, []):
        return "DeclaredDefault::Empty"
    raise DeclarationError(f"{REGISTRY}.{verb}.{name}: no Rust default {default!r}")


def wrap_arm(patterns, result, indent):
    """One match arm over `patterns`, a line each when they do not fit one."""
    single = f"{indent}{' | '.join(patterns)} => {result},"
    if len(single) <= 100:
        return [single]
    return [
        f"{indent}{patterns[0]}",
        *(f"{indent}| {pattern}" for pattern in patterns[1:-1]),
        f"{indent}| {patterns[-1]} => {result},",
    ]


def slots_rust(declarations):
    names = []
    for verb in declarations.verbs:
        if verb.form == "structure":
            continue
        for item in verb.arguments:
            if item.role == "operand" and item.name not in names:
                names.append(item.name)
    return spelled(
        "Slot",
        "The field an operand of a scalar or window verb is written in, which "
        "names the role it plays; each slot is spelled by its field.",
        names,
        "the registry's order",
    )


def handlers_rust(declarations):
    names = []
    for verb in declarations.verbs:
        if verb.form != "scalar":
            continue
        for item in verb.arguments:
            if item.role == "handler" and item.name not in names:
                names.append(item.name)
    return [
        "",
        *doc(
            "The answers a scalar verb declares, each as written; each verb "
            "declares its own subset (`Verb::handlers`)."
        ),
        "#[rustfmt::skip]",
        "#[derive(Clone, Debug, Default, PartialEq)]",
        "pub struct Handlers {",
        *(f"    pub {name}: Option<Constant>," for name in names),
        "}",
        "",
        "#[rustfmt::skip]",
        "impl Handlers {",
        "    /// The handlers `verb` declares, each as its payload writes it.",
        "    pub(crate) fn read(verb: Verb, written: &mut dyn Written) -> Self {",
        "        let mut handler = |field: &'static str| {",
        "            if verb.handlers().contains(&field) { written.constant(field) } else { None }",
        "        };",
        "        Self {",
        *(f"            {name}: handler({literal(name)})," for name in names),
        "        }",
        "    }",
        "}",
    ]


def values_rust(values):
    return spelled(
        values.rust,
        f"The values {values.source} takes, each spelled as written.",
        values.values,
        "declared order",
    )


def unions_rust(declarations):
    lines = []
    for rust, (union, label, values) in declarations.unions.items():
        lines += [
            "",
            *doc(
                f"`{label.split('.', 1)[1]}`: one of the two types its declaration admits."
            ),
            "#[rustfmt::skip]",
            "#[derive(Clone, Debug, PartialEq, Eq)]",
            f"pub enum {rust} {{",
            *(f"    {variant}({held})," for variant, held, _ in union),
            "}",
        ]
    return lines


def read_rust(item, written="written"):
    """The expression reading one parameter, typed as its declaration types it."""
    parameter = item.parameter
    field = literal(item.name)
    reader = f"{written}.{parameter.reader}({field})?"
    values = parameter.values
    if parameter.reader == "union":
        (first, held, method), (enum, _, _) = parameter.union
        rust = parameter.rust
        read = (
            f"match {written}.{method}({field}) {{ "
            f"Some(value) => value.map({rust}::{first}), "
            f"None => Some({rust}::{enum}({enum}::from_name(&{written}.text({field})??)?)) }}"
        )
        if item.required:
            return f"({read})?"
        return read
    if values is not None and parameter.reader == "texts":
        each = f".iter().map(|text| {values.rust}::from_name(text)).collect::<Option<_>>()?"
        if item.required:
            return f"{reader}?{each}"
        return f"match {reader} {{ Some(texts) => Some(texts{each}), None => None }}"
    if values is not None:
        if item.default is not None:
            return (
                f"match {reader} {{ Some(text) => {values.rust}::from_name(&text)?, "
                f"None => {values.rust}::{camel(item.default)} }}"
            )
        if item.required:
            return f"{values.rust}::from_name(&{reader}?)?"
        return f"match {reader} {{ Some(text) => Some({values.rust}::from_name(&text)?), None => None }}"
    if item.default is not None:
        if parameter.reader == "boolean":
            return f"{reader}.unwrap_or({'true' if item.default else 'false'})"
        if parameter.reader in ("int", "text"):
            return f"{reader}.unwrap_or_else(|| {literal(item.default)}.into())"
        raise DeclarationError(f"{item.name}: no default for a {parameter.reader}")
    if item.required:
        return f"{reader}?"
    return reader


def operations_rust(declarations, form, enum, summary):
    verbs = [verb for verb in declarations.verbs if verb.form == form]
    lines = [
        "",
        *doc(summary),
        "#[rustfmt::skip]",
        "#[derive(Clone, Debug, PartialEq)]",
        f"pub enum {enum} {{",
    ]
    for verb in verbs:
        parameters = [item for item in verb.arguments if item.parameter is not None]
        lines.append(f"    /// `{verb.name}`.")
        if not parameters:
            lines.append(f"    {camel(verb.name)},")
            continue
        lines.append(f"    {camel(verb.name)} {{")
        for item in parameters:
            rust = item.parameter.rust
            if not item.required and item.default is None:
                rust = f"Option<{rust}>"
            lines.append(f"        {field_name(item.name)}: {rust},")
        lines.append("    },")
    lines += [
        "}",
        "",
        "#[rustfmt::skip]",
        f"impl {enum} {{",
        "    /// The verb whose parameters these are.",
        "    pub const fn verb(&self) -> Verb {",
        "        match self {",
    ]
    for verb in verbs:
        parameters = [item for item in verb.arguments if item.parameter is not None]
        pattern = f"Self::{camel(verb.name)}" + (" { .. }" if parameters else "")
        lines.append(f"            {pattern} => Verb::{camel(verb.name)},")
    lines += [
        "        }",
        "    }",
        "    /// The parameters `verb` writes, as its declaration types them, or",
        "    /// `None` when one it requires is omitted or one holds a value its type",
        "    /// cannot read; the operation built from them validates the rest.",
        "    pub(crate) fn read(verb: Verb, written: &mut dyn Written) -> Option<Self> {",
        "        Some(match verb {",
    ]
    for verb in verbs:
        parameters = [item for item in verb.arguments if item.parameter is not None]
        if not parameters:
            lines.append(
                f"            Verb::{camel(verb.name)} => Self::{camel(verb.name)},"
            )
            continue
        lines.append(
            f"            Verb::{camel(verb.name)} => Self::{camel(verb.name)} {{"
        )
        for item in parameters:
            lines.append(f"                {field_name(item.name)}: {read_rust(item)},")
        lines.append("            },")
    lines += [
        "            _ => return None,",
        "        })",
        "    }",
        "}",
    ]
    return lines


def written_rust():
    return [
        "",
        "/// How the lowering reads what a verb's payload writes in one field. A",
        "/// reader answers `None` when the field holds a value its type cannot",
        "/// read, and `Some(None)` when the field is omitted or null.",
        "#[rustfmt::skip]",
        "pub(crate) trait Written {",
        "    /// The integer written, as written.",
        "    fn int(&mut self, field: &'static str) -> Option<Option<String>>;",
        "    fn text(&mut self, field: &'static str) -> Option<Option<String>>;",
        "    fn boolean(&mut self, field: &'static str) -> Option<Option<bool>>;",
        "    /// A list of numbers, each an integer or a float.",
        "    fn numbers(&mut self, field: &'static str) -> Option<Option<Vec<f64>>>;",
        "    fn texts(&mut self, field: &'static str) -> Option<Option<Vec<String>>>;",
        "    /// A mapping dictionary, inline or the path of a file holding it.",
        "    fn dictionary(&mut self, field: &'static str) -> Option<Option<Dictionary>>;",
        "    /// A handler: written, even as null, or omitted.",
        "    fn constant(&mut self, field: &'static str) -> Option<Constant>;",
        "}",
    ]


def render(root=ROOT):
    """The generated module."""
    functions, reducers, contexts = grammar(root)
    declarations = Declarations(root)
    spelled_names = {
        *(item.name for item in [*functions, *reducers]),
        *(value for values in declarations.values.values() for value in values.values),
    }
    unused = sorted(set(JOINED) - spelled_names)
    if unused:
        raise DeclarationError(
            f"JOINED names {unused[0]!r}, which no declaration spells"
        )
    body = [
        *functions_rust(functions, contexts),
        *reducers_rust(reducers),
        *verbs_rust(declarations),
        *slots_rust(declarations),
        *handlers_rust(declarations),
        *operations_rust(
            declarations,
            "scalar",
            "ScalarOp",
            "A scalar verb's written parameters; its operands and handlers are "
            "separate. The parameters are kept as written, so that lowering "
            "stays total: building the executor's operation validates them "
            "(`crate::scalar_verb`).",
        ),
        *operations_rust(
            declarations,
            "window",
            "WindowFunction",
            "A window verb's written parameters; its operands, partitions, order "
            "and filter are the window's.",
        ),
        *unions_rust(declarations),
    ]
    for values in declarations.values.values():
        body += values_rust(values)
    body += written_rust()
    lines = [
        "// Generated by tools/packaging/generate_expression_table.py --write. Do not edit.",
        f"// Authoritative sources: {GRAMMAR.as_posix()} and the {REGISTRY} registry of yaml/schema_expression_*.yaml.",
        "//! The expression functions as their declarations state them: the",
        "//! grammar's functions and reducers, and each registered expression with",
        "//! its form, operands, handlers, defaults and written parameters.",
        "",
        "use crate::ast::{Constant, Dictionary};",
        "use crate::expression::Context;",
        *body,
    ]
    return "\n".join([*lines, ""])


# -- the documentation table ---------------------------------------------


def page_table(root=ROOT):
    """The table of expression modules, between the markers of the page."""
    declarations = Declarations(root)
    modules = {}
    for verb in declarations.verbs:
        modules.setdefault(verb.module, []).append(verb.name)
    alone = {names[0] for names in modules.values() if len(names) == 1}
    unplaced = sorted(set(NOTES) - alone)
    if unplaced:
        raise DeclarationError(
            f"NOTES names {unplaced[0]!r}, which no module registers alone"
        )
    lines = [START, "", "| Module | Expressions |", "|---|---|"]
    for module, names in modules.items():
        cell = phrase(names)
        if names[0] in NOTES and len(names) == 1:
            cell += f", {NOTES[names[0]]}"
        lines.append(f"| [`{module}`](../../yaml/{module}) | {cell} |")
    lines += ["", END]
    return "\n".join(lines)


def page(root=ROOT):
    """The page with its table regenerated."""
    text = (root / PAGE).read_text(encoding="ascii")
    if (
        text.count(START) != 1
        or text.count(END) != 1
        or text.index(START) > text.index(END)
    ):
        raise DeclarationError(
            f"{PAGE.as_posix()}: expected one {START} before one {END}"
        )
    before, rest = text.split(START, 1)
    _, after = rest.split(END, 1)
    return before + page_table(root) + after


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args(argv)
    root = ROOT
    outputs = {MODULE: render(root), PAGE: page(root)}
    if args.write:
        for path, text in outputs.items():
            (root / path).write_text(text, encoding="ascii")
        return
    stale = [
        path.as_posix()
        for path, text in outputs.items()
        if not (root / path).exists()
        or (root / path).read_text(encoding="ascii") != text
    ]
    if stale:
        raise SystemExit(
            f"ERROR: {', '.join(stale)} differ from the declarations of the expression "
            f"functions; regenerate them with {WRITE}"
        )
    print("PASS: core's expression table and the schema page are the declarations'")


if __name__ == "__main__":
    main()
