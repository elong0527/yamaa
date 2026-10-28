"""The `public` section of the host interface and the host layers it generates.

``rust/interface/bindings.yaml`` (format 2) declares each host's user-facing
surface under ``public``: the module functions, the result and lazy classes
with their members, the refusal error and the type tokens. From it
``tools/packaging/generate_bindings.py`` writes, through this module:

- ``python/src/yamaa/__init__.py``, the public module: ``__all__``, the error
  class and its native mapping, and the delegating classes and functions;
- ``R/yamaa/R/public-bindings.R``: R's exported functions, the constructors of
  its result and lazy objects, its S3 methods and private accessors;
- ``R/yamaa/NAMESPACE``;
- the ``public`` lines of the normalized manifest.

Generated code only delegates. The handwritten rest stays where it was:
Python's path conversion, lazy-object pickling and version lookup in
``python/src/yamaa/_support.py``; R's reply and argument
conversions, host-function capabilities, locked-object and table-memo
helpers in ``R/yamaa/R/api.R``; and every native body in the binding crates.

A class with ``hosts: [python]`` is a native class of ``yamaa._native`` that
the public module exports as it is: today only ``yamaa.Table``, which every
call returns for a table, as the native call does. Each of its members is a
method of that class, which ``host_only`` names, and the generator writes no
code for it beyond the export.

Delegation is declared, never written as code. A class ``holds`` an ordered
set of values, each named by what it fills: ``handle``, the native handle the
object wraps, or the name of a native argument, which the value fills in the
object's native calls. A class that holds no handle is lazy: it holds paths,
and it keeps a plans handle when a native call it makes takes ``plans``. A
member or a function ``calls`` an operation of the ``operations`` table, and
each native argument of that operation's Python and R exports is bound by
name: to the member's own arguments, to a held value, to the plans handle, or
in R to the host's ``version`` and ``capabilities``. A member that returns a
class makes it from the call's result and the values it holds. README.md lists
the member kinds and these rules.
"""

import re

_PY_WIDTH = 88
_R_WIDTH = 100

_PUBLIC_KEYS = {"python", "types", "value_types", "functions", "classes", "errors"}
# Python annotation of each type token in generated code; None leaves the
# argument unannotated.
_PY_ANNOTATION = {
    "path": None,
    "path_list": None,
    "optional_path": None,
    "text": "str",
    "optional_text": "str | None",
    # A progress callback, which the native call checks (#3762).
    "progress": None,
    "count": "int",
    "flag": "bool",
    "names": "list[str]",
    "paths_written": "list[str]",
    "tree": "dict",
    "json_text": "str",
    # A table is the native yamaa.Table that the native call returns.
    "table": "Table",
    "optional_table": "Table | None",
    "run_output": "Table | None",
    "status": "str",
    "file_type": "str",
    "info": "dict",
    "nothing": "None",
    # Tokens with only a Python projection, which only the members of a
    # Python-only class use; the generator writes no annotation with them.
    "rows": "list[dict[str, object]]",
    "arrow_capsule": "object",
    "schema_request": "object | None",
    "arrow_table": "object",
    "polars_frame": "object",
    "pandas_frame": "object",
}
# The language's column types, each projected into both hosts by
# public.value_types; a table column's type is one of them, possibly inside
# list<T> or optional<T>.
_VALUE_TYPES = ("str", "int", "float", "date", "datetime", "list")
# Tokens whose value is one of the `records`; a member returning one names it.
_RECORD_TOKENS = {"info"}
# Tokens whose value is text from the vocabulary of the same name in
# `values`; each host projection names that vocabulary.
_VOCABULARY_TOKENS = {"status", "file_type"}
_ARGUMENT_TOKENS = {
    "path",
    "path_list",
    "optional_path",
    "text",
    "optional_text",
    "progress",
    "count",
    "flag",
    "file_type",
    "schema_request",
}
_DEFAULTS = {
    "optional_path": {"None"},
    "optional_text": {"None"},
    "progress": {"None"},
    "schema_request": {"None"},
    "flag": {"false", "true"},
    "count": None,  # a nonnegative integer
}
# Handwritten conversions the generated code calls.
_PY_ARGUMENT = {
    "path": "_fspath",
    "path_list": "_fspaths",
    "optional_path": "_optional_fspath",
}
_R_ARGUMENT = {
    "path": ".text",
    "path_list": ".texts",
    "optional_path": ".optional_text",
    "text": ".text",
    "optional_text": ".optional_text",
    "file_type": ".text",
    "progress": ".progress",
}
_NATIVE_PY = {"text": "str", "bool": "bool", "index": "int", "i32": "int"}
_PY_DEFAULT = {"None": "None", "false": "False", "true": "True"}
_R_DEFAULT = {"None": "NULL", "false": "FALSE", "true": "TRUE"}
_KINDS = {"property", "method", "index", "protocol", "private", "printing"}
_PROTOCOLS = {"__contains__", "__iter__", "__len__"}
# R's names() and length() of a result (D44): a container's names are the
# keys its `names` member gives; any other result names these members.
_R_LISTED = {"property", "method"}
# The R generics every result has, listing and counting its names (D44).
_R_NAMES = ("names", "length")
# The return types of a table: a result's property of one keeps the data frame
# it read, so a later read converts nothing until a method of the result runs
# (#4071). R's helper keeps only a data frame: the NULL of a failed run and a
# streamed run's output stream are read again each time.
_R_KEPT = {"table", "optional_table", "run_output"}
# The member kinds of a Python-only class: a property is a native getter, a
# protocol or printing member a native special method spelled by `python`.
_NATIVE_KINDS = {"property", "method", "protocol", "printing"}
_MODULE = re.compile(r"[a-z][a-z0-9_]*")
# How a printing member is spelled: Python's repr() and str() return the text
# its operation renders, and R's format() returns those lines and print()
# writes them, returning the object invisibly (D37).
_PRINTING = {"python": "__repr__", "r": ["print", "format"]}
_MEMBER_KEYS = {
    "name",
    "kind",
    "args",
    "keyword_only",
    "returns",
    "table",
    "record",
    "raises",
    "calls",
    "python",
    "r",
    "doc",
}
_NAME = re.compile(r"[a-z][a-z0-9_]*")
_CLASS = re.compile(r"[A-Z][A-Za-z0-9]*")
_R_NAME = re.compile(r"[a-z][a-z0-9_.]*")


class _Call:
    """A call expression, kept as a tree so it can be wrapped like a formatter."""

    def __init__(self, function, arguments=()):
        self.function = function
        self.arguments = list(arguments)

    def __str__(self):
        return f"{self.function}({', '.join(map(str, self.arguments))})"


def _snake(name):
    return re.sub(r"(?<!^)(?=[A-Z])", "_", name).lower()


def _keys(entry, what, required, optional=()):
    if not isinstance(entry, dict):
        raise ValueError(f"{what} must be a mapping")
    missing = set(required) - set(entry)
    if missing:
        raise ValueError(f"{what} is missing {sorted(missing)}")
    unknown = set(entry) - set(required) - set(optional)
    if unknown:
        raise ValueError(f"{what} has unknown keys {sorted(unknown)}")
    return entry


def _doc(text, what):
    if not isinstance(text, str) or not text.strip():
        raise ValueError(f"{what} must be nonempty text")
    if '"""' in text or "\\" in text or text.endswith('"') or text != text.strip():
        raise ValueError(f"{what} cannot be written as a docstring")
    return text


def _argument(spec, what):
    """Split `token` or `token = literal` and check the default."""
    if not isinstance(spec, str):
        raise ValueError(f"{what} must be text")
    token, separator, default = spec.partition(" = ")
    if token not in _ARGUMENT_TOKENS:
        raise ValueError(f"{what} has an unsupported argument type: {token!r}")
    if not separator:
        return token, None
    allowed = _DEFAULTS.get(token, set())
    if allowed is None:
        if not default.isdigit():
            raise ValueError(f"{what} default must be a nonnegative integer")
    elif default not in allowed:
        raise ValueError(f"{what} has an unsupported default: {default!r}")
    return token, default


class Public:
    """The validated public section, with the native exports it delegates to."""

    def __init__(self, definition):
        self.definition = definition
        public = _keys(definition["public"], "public", _PUBLIC_KEYS)
        self.public = public
        self.classes = public["classes"]
        self.functions = public["functions"]
        self.errors = public["errors"]
        self.operations = definition["operations"]
        exports = definition["python"]["exports"]
        self.py_functions = {
            export["name"]: export for export in exports if export["kind"] == "function"
        }
        self.py_classes = {
            export["name"]: {method["name"]: method for method in export["methods"]}
            for export in exports
            if export["kind"] == "class"
        }
        self.py_exceptions = [
            export["name"] for export in exports if export["kind"] == "exception"
        ]
        self.r_functions = {
            function["name"]: function
            for group in definition["r"]["groups"]
            for function in group["functions"]
        }
        self.package = definition["r"]["module"]
        self._validate()

    # --- Validation --------------------------------------------------------------

    def _validate(self):
        python = _keys(self.public["python"], "public.python", {"version", "doc"})
        if python["version"] != "__version__":
            raise ValueError("public.python.version must be __version__")
        _doc(python["doc"], "public.python.doc")
        types = self.public["types"]
        if not isinstance(types, dict) or not types:
            raise ValueError("public.types must be a nonempty mapping")
        for token, projection in types.items():
            if token not in _PY_ANNOTATION:
                raise ValueError(f"public.types declares an unknown token: {token!r}")
            # A token without an R projection is Python-only.
            entry = _keys(projection, f"public.types.{token}", {"python"}, {"r"})
            for host, text in entry.items():
                if not isinstance(text, str) or not text.strip():
                    raise ValueError(f"public.types.{token}.{host} must be text")
                if token in _VOCABULARY_TOKENS and f"values.{token}" not in text:
                    raise ValueError(
                        f"public.types.{token}.{host} must name values.{token}"
                    )
            if token in _VOCABULARY_TOKENS and token not in self.definition["values"]:
                raise ValueError(f"public.types.{token} names an undeclared vocabulary")
        self._validate_value_types()
        self._validate_errors()
        if not isinstance(self.classes, dict) or not self.classes:
            raise ValueError("public.classes must be a nonempty mapping")
        defined = set()
        for name, cls in self.classes.items():
            if self.python_only(name):
                self._validate_native_class(name, cls)
            else:
                self._validate_class(name, cls, defined)
            defined.add(name)
        # The table tokens annotate the class that every table is.
        if "Table" not in self.classes or not self.python_only("Table"):
            raise ValueError(
                "public.classes.Table, the class every table is, is missing"
            )
        if not isinstance(self.functions, list) or not self.functions:
            raise ValueError("public.functions must be a nonempty list")
        names = set()
        for function in self.functions:
            self._validate_function(function)
            if function["name"] in names:
                raise ValueError(f"public.functions names {function['name']} twice")
            names.add(function["name"])
        r_names = [function["r"] for function in self.functions]
        if len(r_names) != len(set(r_names)):
            raise ValueError("public.functions repeat an R name")
        # Binding every native argument happens while rendering, so render
        # once here: a declaration that cannot delegate fails as it loads.
        self.render_python([])
        self.render_r([])

    def _token(self, token, what, python_only=False):
        if token not in self.public["types"]:
            raise ValueError(f"{what} uses an undeclared type token: {token!r}")
        if not python_only and "r" not in self.public["types"][token]:
            raise ValueError(f"{what} uses the Python-only type token {token!r}")
        return token

    def _returns(self, value, what, defined):
        if value in self.classes:
            if value not in defined:
                raise ValueError(f"{what} returns {value}, which is defined later")
            if self.python_only(value):
                raise ValueError(f"{what} returns the Python-only class {value}")
            return value
        return self._token(value, what)

    def _validate_value_types(self):
        """Both hosts' projection of each column type, covering every table."""
        value_types = self.public["value_types"]
        if not isinstance(value_types, dict) or list(value_types) != list(_VALUE_TYPES):
            raise ValueError(
                f"public.value_types must project {', '.join(_VALUE_TYPES)}, in order"
            )
        for kind, projection in value_types.items():
            entry = _keys(projection, f"public.value_types.{kind}", {"python", "r"})
            for host, text in entry.items():
                if not isinstance(text, str) or not text.strip():
                    raise ValueError(f"public.value_types.{kind}.{host} must be text")
        for table, entry in self.definition["tables"].items():
            for column in entry["columns"]:
                kind = column["type"]
                while kind.startswith(("optional<", "list<")) and kind.endswith(">"):
                    kind = kind.partition("<")[2][:-1]
                if kind not in value_types:
                    raise ValueError(
                        f"tables.{table}.{column['name']} has a type that "
                        "public.value_types does not project"
                    )

    def _validate_errors(self):
        if not isinstance(self.errors, dict) or set(self.errors) != {"refusal"}:
            raise ValueError("public.errors must declare exactly the refusal")
        error = _keys(
            self.errors["refusal"],
            "public.errors.refusal",
            {"python", "r"},
            {"table", "doc"},
        )
        python = _keys(
            error["python"],
            "public.errors.refusal.python",
            {"class", "base", "attributes"},
        )
        if not re.fullmatch(r"yamaa\.[A-Z][A-Za-z]*", str(python["class"])):
            raise ValueError("public.errors.refusal.python.class must be yamaa.<Name>")
        if python["base"] != "Exception":
            raise ValueError("the refusal must derive from Exception")
        attributes = python["attributes"]
        if not isinstance(attributes, dict) or not attributes:
            raise ValueError(
                "the refusal's Python attributes must be a nonempty mapping"
            )
        for attribute, token in attributes.items():
            if not _NAME.fullmatch(attribute):
                raise ValueError(f"invalid refusal attribute: {attribute!r}")
            self._token(token, f"public.errors.refusal.python.{attribute}")
            if _PY_ANNOTATION[token] in {None, "None"}:
                raise ValueError(f"the refusal's {attribute} needs a value type")
        r = _keys(error["r"], "public.errors.refusal.r", {"class", "fields"})
        classes = r["class"]
        if (
            not isinstance(classes, list)
            or not all(isinstance(name, str) for name in classes)
            or classes[-2:] != ["error", "condition"]
        ):
            raise ValueError("the R refusal must be an error condition")
        if not isinstance(r["fields"], dict) or not r["fields"]:
            raise ValueError("the R refusal's fields must be a nonempty mapping")
        for field, kind in r["fields"].items():
            if not _NAME.fullmatch(field) or (
                kind != "NULL" and "r" not in self.public["types"].get(kind, {})
            ):
                raise ValueError(f"invalid R refusal field: {field}: {kind}")
        if "table" in error and error["table"] not in self.definition["tables"]:
            raise ValueError("the refusal names an undeclared table")
        if "doc" in error:
            _doc(error["doc"], "public.errors.refusal.doc")
        if len(self.py_exceptions) != 1:
            raise ValueError("the refusal needs exactly one native Python exception")

    def _validate_signature(self, entry, what, python_only=False):
        args = entry.get("args", {})
        if not isinstance(args, dict):
            raise ValueError(f"{what}.args must be a mapping")
        defaults = False
        for argument, spec in args.items():
            if not _NAME.fullmatch(argument):
                raise ValueError(f"{what} has an invalid argument name: {argument!r}")
            token, default = _argument(spec, f"{what}.{argument}")
            self._token(token, f"{what}.{argument}", python_only)
            if default is None and defaults:
                raise ValueError(
                    f"{what}.{argument} without a default follows one with"
                )
            defaults = defaults or default is not None
        keyword = entry.get("keyword_only", [])
        names = list(args)
        if (
            not isinstance(keyword, list)
            or keyword != names[len(names) - len(keyword) :]
        ):
            raise ValueError(f"{what}.keyword_only must name its trailing arguments")

    def _validate_result(self, entry, what, defined):
        returns = self._returns(entry["returns"], what, defined)
        if "table" in entry:
            if returns not in {"table", "optional_table"}:
                raise ValueError(f"{what} names a table but returns {returns}")
            if entry["table"] not in self.definition["tables"]:
                raise ValueError(
                    f"{what} names an undeclared table: {entry['table']!r}"
                )
        if ("record" in entry) != (returns in _RECORD_TOKENS):
            raise ValueError(f"{what} names a record exactly when it returns one")
        if "record" in entry and entry["record"] not in self.definition["records"]:
            raise ValueError(f"{what} names an undeclared record: {entry['record']!r}")
        if "raises" in entry and entry["raises"] not in self.errors:
            raise ValueError(f"{what} raises an undeclared error: {entry['raises']!r}")
        if "doc" in entry:
            _doc(entry["doc"], f"{what}.doc")
        if "calls" in entry and entry["calls"] not in self.operations:
            raise ValueError(f"{what} calls an unknown operation: {entry['calls']!r}")

    def _validate_function(self, function):
        what = f"public.functions[{function.get('name')!r}]"
        _keys(
            function,
            what,
            {"name", "python", "r", "returns"},
            {"args", "keyword_only", "table", "record", "raises", "calls", "doc"},
        )
        name = function["name"]
        if not isinstance(name, str) or not _NAME.fullmatch(name):
            raise ValueError(f"invalid public function name: {name!r}")
        if function["python"] != f"yamaa.{name}":
            raise ValueError(f"{what}.python must be yamaa.{name}")
        if not isinstance(function["r"], str) or not _R_NAME.fullmatch(function["r"]):
            raise ValueError(f"{what}.r must be an R function name")
        self._validate_signature(function, what)
        self._validate_result(function, what, set(self.classes))
        if "calls" not in function and function["returns"] not in self.classes:
            raise ValueError(f"{what} calls nothing, so it must return a class")
        if "calls" in function:
            self._python_target(function["calls"], None, what)

    def _validate_class(self, name, cls, defined):
        what = f"public.classes.{name}"
        _keys(cls, what, {"python", "r", "holds", "members"}, {"doc"})
        if not _CLASS.fullmatch(name) or cls["python"] != f"yamaa.{name}":
            raise ValueError(f"{what}.python must be yamaa.{name}")
        if not isinstance(cls["r"], str) or not re.fullmatch(
            r"yamaa_[a-z_]+", cls["r"]
        ):
            raise ValueError(f"{what}.r must be an S3 class yamaa_<name>")
        if "doc" in cls:
            _doc(cls["doc"], f"{what}.doc")
        holds = cls["holds"]
        if not isinstance(holds, dict) or not holds:
            raise ValueError(f"{what}.holds must be a nonempty mapping")
        for hold, role in holds.items():
            if not _NAME.fullmatch(hold) or not isinstance(role, str):
                raise ValueError(f"{what}.holds has an invalid entry: {hold}: {role}")
        roles = list(holds.values())
        if len(roles) != len(set(roles)):
            raise ValueError(f"{what}.holds repeats a role")
        if "handle" in roles and roles[0] != "handle":
            raise ValueError(f"{what}.holds must hold its handle first")
        members = cls["members"]
        if not isinstance(members, list) or not members:
            raise ValueError(f"{what}.members must be a nonempty list")
        names = set()
        for member in members:
            mwhat = (
                f"{what}.{member.get('name') if isinstance(member, dict) else member}"
            )
            _keys(member, mwhat, {"name", "kind"}, _MEMBER_KEYS)
            if not isinstance(member["name"], str) or not _NAME.fullmatch(
                member["name"]
            ):
                raise ValueError(f"invalid member name in {what}: {member['name']!r}")
            if member["name"] in names:
                raise ValueError(f"{what} declares {member['name']} twice")
            names.add(member["name"])
        for member in members:
            self._validate_member(
                name, cls, member, f"{what}.{member['name']}", defined
            )
        if self.kind(name) == "handle":
            native = {
                self._python_target(m["calls"], name, what)[1]
                for m in members
                if "calls" in m
            }
            if len(native) != 1:
                raise ValueError(f"{what} members must call one native class: {native}")

    def _validate_member(self, class_name, cls, member, what, defined):
        kind = member["kind"]
        if kind not in _KINDS:
            raise ValueError(f"{what} has an unknown kind: {kind!r}")
        self._validate_signature(member, what)
        spelled = {key for key in ("python", "r") if key in member}
        if kind == "protocol":
            if member.get("python") not in _PROTOCOLS or set(member) - {
                "name",
                "kind",
                "python",
                "doc",
            }:
                raise ValueError(
                    f"{what} must be a Python protocol: {sorted(_PROTOCOLS)}"
                )
            if "names" not in {m["name"] for m in cls["members"]}:
                raise ValueError(f"{what} needs a names member")
            if "doc" in member:
                _doc(member["doc"], f"{what}.doc")
            return
        if kind == "printing":
            if (
                any(
                    member.get(host) != spelling for host, spelling in _PRINTING.items()
                )
                or set(member) - {"name", "kind", "python", "r", "calls", "doc"}
                or "calls" not in member
            ):
                raise ValueError(
                    f"{what} must be spelled __repr__ and [print, format] and call "
                    "the operation that renders its text"
                )
            # A result's text comes from its handle and a lazy object's from
            # the paths it holds; the call's binding is checked as it renders.
            if member["calls"] not in self.operations:
                raise ValueError(
                    f"{what} calls an unknown operation: {member['calls']!r}"
                )
            self._python_target(member["calls"], class_name, what)
            if "doc" in member:
                _doc(member["doc"], f"{what}.doc")
            return
        if "returns" not in member:
            raise ValueError(f"{what} must declare what it returns")
        self._validate_result(member, what, defined)
        if kind == "index":
            if member.get("python") != "__getitem__" or member.get("r") != "[[":
                raise ValueError(f"{what} must be spelled __getitem__ and [[")
            if (
                len(member.get("args", {})) != 1
                or member["returns"] not in self.classes
            ):
                raise ValueError(f"{what} takes one key and returns a class")
            if member.get("raises") != "refusal" or "calls" not in member:
                raise ValueError(f"{what} must call the refusal for an absent key")
            if "names" not in {m["name"] for m in cls["members"]}:
                raise ValueError(f"{what} needs a names member")
        elif spelled:
            raise ValueError(
                f"{what} spells a host name, which only index and printing do"
            )
        if kind in {"index", "private"} and "handle" not in cls["holds"].values():
            raise ValueError(f"{what} reaches a handle, which {class_name} lacks")
        if "calls" not in member:
            raise ValueError(f"{what} must name the operation it calls")
        if kind == "property" and member.get("args"):
            raise ValueError(f"{what} is a property and takes no arguments")
        if "calls" in member:
            self._python_target(member["calls"], class_name, what)

    def _validate_native_class(self, name, cls):
        """A Python-only class: a native class the public module exports, each
        member one of its methods, which host_only names."""
        what = f"public.classes.{name}"
        _keys(cls, what, {"python", "hosts", "members"})
        if not _CLASS.fullmatch(name) or cls["python"] != f"yamaa.{name}":
            raise ValueError(f"{what}.python must be yamaa.{name}")
        if name not in self.py_classes:
            raise ValueError(f"{what} is no class of yamaa._native")
        native = self.py_classes[name]
        members = cls["members"]
        if not isinstance(members, list) or not members:
            raise ValueError(f"{what}.members must be a nonempty list")
        names, methods = set(), set()
        for member in members:
            mwhat = (
                f"{what}.{member.get('name') if isinstance(member, dict) else member}"
            )
            _keys(
                member,
                mwhat,
                {"name", "kind"},
                {"python", "args", "returns", "imports"},
            )
            if not isinstance(member["name"], str) or not _NAME.fullmatch(
                member["name"]
            ):
                raise ValueError(f"invalid member name in {what}: {member['name']!r}")
            if member["name"] in names:
                raise ValueError(f"{what} declares {member['name']} twice")
            names.add(member["name"])
            kind = member["kind"]
            if kind not in _NATIVE_KINDS:
                raise ValueError(f"{mwhat} has an unknown kind: {kind!r}")
            if kind in {"protocol", "printing"}:
                method = member.get("python")
                if not isinstance(method, str) or not re.fullmatch(
                    r"__[a-z][a-z0-9_]*__", method
                ):
                    raise ValueError(f"{mwhat} must spell its special method")
                if kind == "printing" and (method != "__repr__" or "returns" in member):
                    raise ValueError(
                        f"{mwhat} must be __repr__, which returns its text"
                    )
            elif "python" in member:
                raise ValueError(
                    f"{mwhat} spells a host name, which only protocol and printing do"
                )
            else:
                method = member["name"]
            if method not in native:
                raise ValueError(f"{mwhat} is no method of yamaa._native.{name}")
            if method in methods:
                raise ValueError(f"{what} declares the native {method} twice")
            methods.add(method)
            if (kind == "property") != (native[method].get("kind") == "getter"):
                raise ValueError(
                    f"{mwhat} is a property exactly when its native method is a getter"
                )
            if kind != "printing":
                if "returns" not in member:
                    raise ValueError(f"{mwhat} must declare what it returns")
                self._token(member["returns"], mwhat, python_only=True)
            self._validate_signature(member, mwhat, python_only=True)
            if list(member.get("args", {})) != list(native[method]["args"]):
                raise ValueError(f"{mwhat} must take its native method's arguments")
            imports = member.get("imports")
            if imports is not None and (
                kind != "method"
                or not isinstance(imports, list)
                or not imports
                or len(set(imports)) != len(imports)
                or not all(
                    isinstance(module, str) and _MODULE.fullmatch(module)
                    for module in imports
                )
            ):
                raise ValueError(
                    f"{mwhat}.imports must list the modules a method imports"
                )
        if methods != set(native):
            raise ValueError(
                f"{what} must declare every native method: "
                f"{', '.join(sorted(set(native) - methods))}"
            )
        transport = set(self.definition["host_only"]["python"])
        missing = sorted(f"{name}.{method}" for method in native)
        missing = [method for method in missing if method not in transport]
        if missing:
            raise ValueError(f"{what} needs host_only entries: {', '.join(missing)}")

    # --- Classes and native targets ------------------------------------------------

    def python_only(self, name):
        """Whether `name` is a Python-only class, a native class exported as is."""
        return self.classes[name].get("hosts") == ["python"]

    def delegating_classes(self):
        """The classes the generator writes in both hosts, each delegating to
        its native calls, by name in definition order."""
        return {
            name: cls
            for name, cls in self.classes.items()
            if not self.python_only(name)
        }

    def holds(self, name):
        return list(self.classes[name]["holds"].items())

    def kind(self, name):
        """A native class, a result wrapping a handle, or a lazy object."""
        if self.python_only(name):
            return "native"
        return "handle" if "handle" in self.classes[name]["holds"].values() else "lazy"

    def _handle(self, name):
        for hold, role in self.holds(name):
            if role == "handle":
                return hold
        return None

    def _operation(self, operation, what):
        hosts = self.operations[operation]
        if len(hosts["python"]) != 1 or len(hosts["r"]) != 1:
            raise ValueError(
                f"{what} calls {operation}, which needs one export per host"
            )
        return hosts["python"][0], hosts["r"][0]

    def _python_target(self, operation, class_name, what):
        """('method', native class, declaration) or ('function', None, declaration)."""
        reference, _ = self._operation(operation, what)
        handle = class_name is not None and self.kind(class_name) == "handle"
        if "." in reference:
            owner, _, method = reference.partition(".")
            if not handle:
                raise ValueError(f"{what} calls {reference} without holding a handle")
            if method not in self.py_classes.get(owner, {}):
                raise ValueError(f"{what} calls {reference}, which is not a method")
            return "method", owner, self.py_classes[owner][method]
        if handle:
            raise ValueError(f"{what} holds a handle but calls {reference}")
        if reference not in self.py_functions:
            raise ValueError(f"{what} calls {reference}, which is not a function")
        return "function", None, self.py_functions[reference]

    def _native_class(self, name):
        for member in self.classes[name]["members"]:
            if "calls" in member:
                return self._python_target(member["calls"], name, name)[1]
        return None

    def plans(self, name):
        """Whether a lazy class keeps a plans handle for its native calls."""
        if self.kind(name) != "lazy":
            return False
        for member in self.classes[name]["members"]:
            if "calls" in member:
                _, _, target = self._python_target(member["calls"], name, name)
                if "plans" in target["args"]:
                    return True
        return False

    def _plans_exports(self):
        python, r = self._operation("plans", "the plans holder")
        if self.py_functions[python]["args"] or len(self.r_functions[r]["args"]) != 1:
            raise ValueError("the plans operation has unexpected native arguments")
        return python, r

    # --- Python ------------------------------------------------------------------

    def _py_argument(self, name, token):
        helper = _PY_ARGUMENT.get(token)
        return _Call(helper, [name]) if helper else name

    def _py_context(self, role, hold, class_name, args):
        """The Python value that fills `role` (or `hold`) in this context."""
        if role == "handle":
            if class_name is None or self.kind(class_name) != "handle":
                return None
            return f"self._{self._handle(class_name)}"
        for key in (hold, role):
            if key in args:
                return key
        if class_name is not None:
            for name, filled in self.holds(class_name):
                if filled == role and role != "handle":
                    return f"self._{name}"
        return None

    def _py_call(self, entry, class_name, what):
        kind, owner, target = self._python_target(entry["calls"], class_name, what)
        args = {
            name: _argument(spec, what)[0]
            for name, spec in entry.get("args", {}).items()
        }
        if kind == "method":
            callable_ = f"self._{self._handle(class_name)}.{target['name']}"
        else:
            callable_ = f"_native.{target['name']}"
        values, used = [], set()
        for native in target["args"]:
            if native in args:
                values.append(self._py_argument(native, args[native]))
                used.add(native)
                continue
            found = None
            if class_name is not None:
                for hold, role in self.holds(class_name):
                    if role == native:
                        found = f"self._{hold}"
            if (
                found is None
                and native == "plans"
                and class_name
                and self.plans(class_name)
            ):
                found = "self._plans"
            if found is None:
                raise ValueError(f"{what}: nothing fills the Python argument {native}")
            values.append(found)
        if set(args) - used:
            raise ValueError(
                f"{what}: {sorted(set(args) - used)} fill no Python argument"
            )
        if entry.get("raises"):
            return _Call("_call", [callable_, *values]), target
        return _Call(callable_, values), target

    def _py_construct(self, target, first, class_name, args, what):
        self._same_native(target, first, class_name, what)
        values = []
        for index, (hold, role) in enumerate(self.holds(target)):
            if index == 0 and first is not None:
                values.append(first)
                continue
            values.append(self._py_context(role, hold, class_name, args))
            if values[-1] is None and (self.kind(target) != "lazy" or index == 0):
                raise ValueError(f"{what}: nothing makes {target}'s {hold}")
        # A lazy object's later paths default to None; only trailing ones may.
        while values and values[-1] is None:
            values.pop()
        if None in values:
            raise ValueError(f"{what}: {target} would skip a held value")
        return _Call(target, values)

    def _same_native(self, target, first, class_name, what):
        """A handle passed on from the caller must be the class's native type."""
        if first is not None or self.kind(target) != "handle":
            return
        if class_name is None or self.kind(class_name) != "handle":
            raise ValueError(f"{what}: nothing passes {target} a handle")
        if self._native_class(target) != self._native_class(class_name):
            raise ValueError(f"{what}: {class_name} cannot pass its handle to {target}")

    def _py_result(self, entry, call, native, class_name, what):
        returns = entry["returns"]
        args = dict(entry.get("args", {}))
        if returns in self.classes:
            if self.kind(returns) == "handle" and native[
                "returns"
            ] != self._native_class(returns):
                raise ValueError(f"{what} returns {native['returns']}, not {returns}")
            return self._py_construct(returns, call, class_name, args, what)
        return call

    def _py_signature(self, entry, first=None):
        parts = [first] if first else []
        keyword = entry.get("keyword_only", [])
        for name, spec in entry.get("args", {}).items():
            token, default = _argument(spec, name)
            if keyword and name == keyword[0]:
                parts.append("*")
            annotation = _PY_ANNOTATION[token]
            part = f"{name}: {annotation}" if annotation else name
            if default is not None:
                literal = _PY_DEFAULT.get(default, default)
                part += f" = {literal}" if annotation else f"={literal}"
            parts.append(part)
        return ", ".join(parts)

    def _py_annotation(self, returns):
        return returns if returns in self.classes else _PY_ANNOTATION[returns]

    def _py_init(self, name):
        kind = self.kind(name)
        params, body = [], []
        for index, (hold, role) in enumerate(self.holds(name)):
            if kind == "lazy" and index > 0:
                params.append(f"{hold}=None")
            elif kind == "handle" and role != "handle":
                params.append(f"{hold}: {self._held_annotation(name, role)}")
            else:
                params.append(hold)
            if kind == "lazy" and index == 0:
                body.append(f"self._{hold} = _fspath({hold})")
            else:
                body.append(f"self._{hold} = {hold}")
        if self.plans(name):
            python, _ = self._plans_exports()
            body.append(f"self._plans = _native.{python}()")
        lines = _py_def("__init__", ", ".join(["self", *params]), None, "    ")
        return lines + [f"        {line}" for line in body]

    def _held_annotation(self, name, role):
        for member in self.classes[name]["members"]:
            if "calls" in member:
                _, _, target = self._python_target(member["calls"], name, name)
                if role in target["args"]:
                    native = target["args"][role]
                    if native not in _NATIVE_PY:
                        break
                    return _NATIVE_PY[native]
        raise ValueError(f"public.classes.{name}: no native argument types {role}")

    def _names_call(self, name):
        member = next(m for m in self.classes[name]["members"] if m["name"] == "names")
        call, _ = self._py_call(member, name, f"public.classes.{name}.names")
        return call

    def _py_member(self, name, member):
        what = f"public.classes.{name}.{member['name']}"
        doc = [f'        """{member["doc"]}"""'] if "doc" in member else []
        kind = member["kind"]
        if kind == "protocol":
            names = str(self._names_call(name))
            if member["python"] == "__contains__":
                header = _py_def("__contains__", "self, name: object", "bool", "    ")
                return header + doc + [f"        return name in {names}"]
            if member["python"] == "__len__":
                header = _py_def("__len__", "self", "int", "    ")
                return header + doc + [f"        return len({names})"]
            return (
                _py_def("__iter__", "self", None, "    ")
                + doc
                + [f"        return iter({names})"]
            )
        if kind == "index":
            (key,) = member["args"]
            refusal, _ = self._py_call(member, name, what)
            made = self._py_construct(
                member["returns"], None, name, member["args"], what
            )
            signature = self._py_signature(member, "self")
            header = _py_def("__getitem__", signature, member["returns"], "    ")
            return (
                header
                + doc
                + [
                    f"        if {key} not in {self._names_call(name)}:",
                    f"            {refusal}  # raises the engine's refusal",
                    *_py_lines(made, "        ", "return "),
                ]
            )
        if kind == "printing":
            # repr() returns the rendered text, and str() falls back to it.
            call, _ = self._py_call(member, name, what)
            header = _py_def(member["python"], "self", "str", "    ")
            return header + doc + _py_lines(call, "        ", "return ")
        call, native = self._py_call(member, name, what)
        result = self._py_result(member, call, native, name, what)
        annotation = self._py_annotation(member["returns"])
        header = _py_def(
            member["name"], self._py_signature(member, "self"), annotation, "    "
        )
        if kind == "property":
            header = ["    @property", *header]
        if member["returns"] == "nothing":
            body = _py_lines(result, "        ")
        else:
            body = _py_lines(result, "        ", "return ")
        return header + doc + body

    def _py_class(self, name):
        cls = self.classes[name]
        base = "(_Lazy)" if self.plans(name) else ""
        lines = [f"class {name}{base}:"]
        if "doc" in cls:
            lines += [f'    """{cls["doc"]}"""', ""]
        lines += self._py_init(name)
        for member in cls["members"]:
            if member["kind"] == "private":
                continue
            lines += ["", *self._py_member(name, member)]
        return lines

    def _py_function(self, function):
        what = f"public.functions.{function['name']}"
        args = dict(function.get("args", {}))
        if "calls" in function:
            call, native = self._py_call(function, None, what)
            result = self._py_result(function, call, native, None, what)
        else:
            result = self._py_construct(function["returns"], None, None, args, what)
        header = _py_def(
            function["name"],
            self._py_signature(function),
            self._py_annotation(function["returns"]),
            "",
        )
        doc = [f'    """{function["doc"]}"""'] if "doc" in function else []
        return header + doc + _py_lines(result, "    ", "return ")

    def python_names(self):
        refusal = self.errors["refusal"]["python"]["class"].removeprefix("yamaa.")
        names = [refusal, self.public["python"]["version"], *self._native_exports()]
        return sorted(names + [function["name"] for function in self.functions])

    def _native_exports(self):
        """The Python-only classes, which the public module imports as they are."""
        return [name for name in self.classes if self.python_only(name)]

    def render_python(self, header, handshake=()):
        """The public module. `handshake`, the lines that negotiate the
        interface, runs right after `_native` is imported, so the imports that
        follow it mark that they come after code."""
        error = self.errors["refusal"]
        refusal = error["python"]["class"].removeprefix("yamaa.")
        attributes = error["python"]["attributes"]
        body = [f"class {refusal}({error['python']['base']}):"]
        if "doc" in error:
            body += [f'    """{error["doc"]}"""', ""]
        signature = ", ".join(
            ["self", "message: str"]
            + [f"{name}: {_PY_ANNOTATION[token]}" for name, token in attributes.items()]
        )
        body += _py_def("__init__", signature, None, "    ")
        body.append("        super().__init__(message)")
        body += [f"        self.{name} = {name}" for name in attributes]
        mapped = [f'getattr(error, "{name}")' for name in attributes]
        body += [
            "",
            "",
            "def _call(function, *args):",
            "    try:",
            "        return function(*args)",
            f"    except _native.{self.py_exceptions[0]} as error:",
            "        # The binding sets the refusal's attributes on the native",
            "        # exception, so they are read as dynamic attributes.",
            *_py_lines(
                _Call(refusal, ["str(error)", *mapped]),
                "        ",
                "raise ",
                " from None",
            ),
        ]
        for name in self.delegating_classes():
            body += ["", "", *self._py_class(name)]
        for function in self.functions:
            body += ["", "", *self._py_function(function)]
        text = "\n".join(body)
        # Import exactly the handwritten helpers the generated code uses, in
        # the order ruff's import sorting gives them.
        helpers = [
            name
            for name in ("_fspath", "_fspaths", "_Lazy", "_optional_fspath")
            if re.search(rf"(?<![\w.]){name}\b", text)
        ]
        imported = ["__version__", *helpers]
        single = f"from yamaa._support import {', '.join(imported)}"
        # After the handshake an import carries its reason, which counts
        # toward the width ruff wraps it at.
        reason = "  # noqa: E402" if handshake else ""
        if len(single) + len(reason) > _PY_WIDTH:
            # ruff format splits a long import one name per line.
            single = "\n".join(
                [
                    "from yamaa._support import (",
                    *(f"    {name}," for name in imported),
                    ")",
                ]
            )
        doc = self.public["python"]["doc"].split("\n")
        quoted = [f'"{name}"' for name in self.python_names()]
        exports = f"__all__ = [{', '.join(quoted)}]"
        if len(exports) > _PY_WIDTH:
            # ruff format splits a long list one item per line.
            exports = "\n".join(
                ["__all__ = [", *(f"    {name}," for name in quoted), "]"]
            )
        native = self._native_exports()
        imports = [f"from yamaa._native import {', '.join(native)}"] if native else []
        imports.append(single)
        negotiated = []
        if handshake:
            negotiated = ["", "", *handshake, ""]
            # Each import statement after the handshake's code says why it
            # follows code, on its first line.
            imports = [
                statement.replace("\n", "  # noqa: E402\n", 1)
                if "\n" in statement
                else f"{statement}  # noqa: E402"
                for statement in imports
            ]
        lines = [
            *header,
            f'"""{doc[0]}',
            *doc[1:],
            '"""',
            "",
            "from yamaa import _native",
            *negotiated,
            *imports,
            "",
            exports,
            "",
            "",
            text,
        ]
        return "\n".join(lines) + "\n"

    # --- R -------------------------------------------------------------------------

    def _r_argument(self, name, token):
        helper = _R_ARGUMENT.get(token)
        return _Call(helper, [name, f'"{name}"']) if helper else name

    def _r_call(self, entry, class_name, what, *, handle=None, converted=()):
        """The R native call of `entry` and whether it takes the plans handle."""
        _, reference = self._operation(entry["calls"], what)
        target = self.r_functions[reference]
        args = {
            name: _argument(spec, what)[0]
            for name, spec in entry.get("args", {}).items()
        }
        values, used, plans = [], set(), False
        for native in target["args"]:
            if native == "handle" and handle:
                values.append(handle)
            elif native == "version":
                values.append(_Call(".version"))
            elif native == "capabilities":
                values.append(_Call(".capabilities"))
            elif native == "plans" and class_name and self.plans(class_name):
                values.append("plans")
                plans = True
            elif native in args:
                used.add(native)
                if native in converted:
                    values.append(native)
                else:
                    values.append(self._r_argument(native, args[native]))
            else:
                held = [
                    hold
                    for hold, role in (self.holds(class_name) if class_name else [])
                    if role == native
                ]
                if not held:
                    raise ValueError(f"{what}: nothing fills the R argument {native}")
                values.append(held[0])
        if set(args) - used:
            raise ValueError(f"{what}: {sorted(set(args) - used)} fill no R argument")
        return _Call(".reply", [_Call(f".native_{reference}", values)]), plans

    def _r_context(self, role, hold, class_name, args, handle, converted):
        if role == "handle":
            return handle
        for key in (hold, role):
            if key in args:
                if key in converted:
                    return key
                return self._r_argument(key, _argument(args[key], key)[0])
        if class_name is not None:
            for name, filled in self.holds(class_name):
                if filled == role and role != "handle":
                    return name
        return None

    def _r_construct(
        self, target, first, class_name, args, what, handle=None, converted=()
    ):
        values = []
        for index, (hold, role) in enumerate(self.holds(target)):
            if index == 0 and first is not None:
                if self.kind(target) == "lazy":
                    first = _Call(".text", [first, f'"{hold}"'])
                values.append(first)
                continue
            found = self._r_context(role, hold, class_name, args, handle, converted)
            if found is None:
                if self.kind(target) == "lazy" and index > 0:
                    values.append("NULL")
                    continue
                raise ValueError(f"{what}: nothing makes {target}'s {hold}")
            values.append(found)
        return _Call(f".{_snake(target)}", values)

    def _r_result(self, entry, call, class_name, what, handle=None):
        returns = entry["returns"]
        if returns in self.classes:
            return self._r_construct(
                returns, call, class_name, entry.get("args", {}), what, handle
            )
        # A method that writes returns its result invisibly, as `save()` does.
        if returns in {"nothing", "paths_written"}:
            return _Call("invisible", [call])
        return call

    def _r_signature(self, entry):
        parts = []
        for name, spec in entry.get("args", {}).items():
            _, default = _argument(spec, name)
            parts.append(
                name
                if default is None
                else f"{name} = {_R_DEFAULT.get(default, default)}"
            )
        return ", ".join(parts)

    def _r_bound(self, entry, class_name):
        """The held values that the R call of `entry` takes."""
        _, reference = self._operation(entry["calls"], class_name)
        args = entry.get("args", {})
        bound = set()
        for native in self.r_functions[reference]["args"]:
            if native in args or native in {"version", "capabilities", "plans"}:
                continue
            bound.update(
                hold for hold, role in self.holds(class_name) if role == native
            )
        return bound

    def _r_binding(self, name, hold):
        """The hidden binding that keeps `hold`: `.handle` for the handle."""
        return ".handle" if hold == self._handle(name) else f".{hold}"

    def _r_hidden(self, name):
        """The held values that an S3 method or a private accessor reaches,
        in hold order; the object keeps each in its hidden binding."""
        reached = set()
        for member in self.classes[name]["members"]:
            if member["kind"] in {"index", "private", "printing"}:
                reached |= self._r_bound(member, name)
        if self.kind(name) == "handle" and self._r_listed(name) is None:
            reached |= self._r_bound(self._names_member(name), name)
        return [hold for hold, _ in self.holds(name) if hold in reached]

    def _names_member(self, name):
        return next(m for m in self.classes[name]["members"] if m["name"] == "names")

    def _r_listed(self, name):
        """The public members that R's names() of a result lists, in
        declaration order; None for a container, one with an `index` member,
        whose names() are the keys its `names` member gives (D44)."""
        members = self.classes[name]["members"]
        if any(member["kind"] == "index" for member in members):
            return None
        return [member["name"] for member in members if member["kind"] in _R_LISTED]

    def _r_names(self, name):
        """R's names() and length() of a result: S3 methods that list its
        public members, or a container's keys, and count them."""
        cls = self.classes[name]
        listed = self._r_listed(name)
        if listed is None:
            member = self._names_member(name)
            what = f"public.classes.{name}.names"
            call, _ = self._r_call(member, name, what, handle=self._handle(name))
            return [
                "",
                f"names.{cls['r']} <- function(x) {{",
                *self._r_reads(name, member, "x"),
                *_r_lines(call, "  "),
                "}",
                "",
                f"length.{cls['r']} <- function(x) length(names(x))",
            ]
        names = _Call("c", [f'"{member}"' for member in listed])
        return [
            "",
            *_r_closure(f"names.{cls['r']} <- ", "x", names, ""),
            "",
            f"length.{cls['r']} <- function(x) {len(listed)}L",
        ]

    def _r_reads(self, name, entry, envir):
        """Lines reading each held value the call of `entry` takes from its
        hidden binding in `envir`, as a variable named for the value."""
        bound = self._r_bound(entry, name)
        return [
            f'  {hold} <- get("{self._r_binding(name, hold)}", envir = {envir}, '
            "inherits = FALSE)"
            for hold, _ in self.holds(name)
            if hold in bound
        ]

    def _r_indexed(self, name):
        """Whether a result has an `index` member, whose objects share its memo."""
        return any(m["kind"] == "index" for m in self.classes[name]["members"])

    def _r_constructor(self, name):
        cls = self.classes[name]
        holds = [hold for hold, _ in self.holds(name)]
        handle = self._handle(name)
        # A result's tables are kept in the memo of its handle, which a member
        # made by its index shares (#4071).
        params = [*holds, "memo = .memo()"] if handle else holds
        forced = [*holds, "memo"] if handle else holds
        lines = [f".{_snake(name)} <- function({', '.join(params)}) {{"]
        lines += [f"  force({value})" for value in forced]
        if self.plans(name):
            lines.append("  plans <- NULL")
        lines.append("  object <- new.env(parent = emptyenv())")
        lines += [
            f"  object${self._r_binding(name, hold)} <- {hold}"
            for hold in self._r_hidden(name)
        ]
        if handle and self._r_indexed(name):
            lines.append("  object$.memo <- memo")
        for member in cls["members"]:
            lines += self._r_member(name, member, handle)
        lines += [f'  .object(object, "{cls["r"]}")', "}"]
        return lines

    def _r_member(self, name, member, handle):
        what = f"public.classes.{name}.{member['name']}"
        kind = member["kind"]
        if kind not in {"property", "method"}:
            return []
        call, plans = self._r_call(member, name, what, handle=handle)
        if handle and kind == "method":
            # A result's method, such as save(), can change what its handle
            # answers, so the memo starts again however the call ends.
            call = _Call(".changing", ["memo", call])
        result = self._r_result(member, call, name, what, handle)
        if kind == "property":
            if handle and member["returns"] in _R_KEPT:
                prefix = f'  .kept(object, "{member["name"]}", memo, '
            else:
                prefix = f'  .read_only(object, "{member["name"]}", '
            return _r_closure(prefix, "", result, "  ", ")")
        prefix = f"  object${member['name']} <- "
        signature = self._r_signature(member)
        if plans:
            _, refresh = self._plans_exports()
            return [
                *_r_header(prefix, signature),
                f"    plans <<- .reply(.native_{refresh}(plans))",
                *_r_lines(result, "    "),
                "  }",
            ]
        return _r_closure(prefix, signature, result, "  ")

    def _r_extras(self, name):
        """S3 methods and private accessors that reach an object's handle."""
        cls = self.classes[name]
        lines = self._r_names(name) if self.kind(name) == "handle" else []
        for member in cls["members"]:
            what = f"public.classes.{name}.{member['name']}"
            if member["kind"] == "index":
                (key,) = member["args"]
                refusal, _ = self._r_call(
                    member, name, what, handle="native", converted={key}
                )
                made = self._r_construct(
                    member["returns"], None, name, member["args"], what, "native", {key}
                )
                # The member reads its container's handle, so it shares the
                # container's memo, which the container's save() starts again.
                made.arguments.append('get(".memo", envir = x, inherits = FALSE)')
                names_member = next(m for m in cls["members"] if m["name"] == "names")
                names, _ = self._r_call(names_member, name, what, handle="native")
                token = _argument(member["args"][key], key)[0]
                helper = _R_ARGUMENT.get(token)
                converted = f'{helper}(i, "{key}")' if helper else "i"
                lines += [
                    "",
                    f"`{member['r']}.{cls['r']}` <- function(x, i, ...) {{",
                    '  native <- get(".handle", envir = x, inherits = FALSE)',
                    f"  {key} <- {converted}",
                    f"  if (!i %in% {names}) {{",
                    f"    {refusal} # raises the engine's refusal",
                    "  }",
                    *_r_lines(made, "  "),
                    "}",
                ]
            elif member["kind"] == "private":
                owner = _snake(name)
                call, _ = self._r_call(member, name, what, handle="native")
                signature = ", ".join(filter(None, [owner, self._r_signature(member)]))
                lines += [
                    "",
                    f".{owner}_{member['name']} <- function({signature}) {{",
                    f'  native <- get(".handle", envir = {owner}, inherits = FALSE)',
                    *_r_lines(call, "  "),
                    "}",
                ]
            elif member["kind"] == "printing":
                # format() returns the lines Rust renders, from the handle of
                # a result or the path of a lazy object; print() writes them,
                # each ended by a line feed, and returns the object invisibly.
                call, _ = self._r_call(member, name, what, handle=self._handle(name))
                lines += [
                    "",
                    f"print.{cls['r']} <- function(x, ...) {{",
                    '  cat(format(x), sep = "\\n")',
                    "  invisible(x)",
                    "}",
                    "",
                    f"format.{cls['r']} <- function(x, ...) {{",
                    *self._r_reads(name, member, "x"),
                    *_r_lines(call, "  "),
                    "}",
                ]
        return lines

    def _r_function(self, function):
        what = f"public.functions.{function['name']}"
        args = dict(function.get("args", {}))
        if "calls" in function:
            call, _ = self._r_call(function, None, what)
            result = self._r_result(function, call, None, what)
        else:
            result = self._r_construct(function["returns"], None, None, args, what)
        return _r_closure(
            f"{function['r']} <- ", self._r_signature(function), result, ""
        )

    def render_r(self, header):
        lines = list(header)
        for name in self.delegating_classes():
            lines += ["", *self._r_constructor(name), *self._r_extras(name)]
        for function in self.functions:
            lines += ["", *self._r_function(function)]
        return "\n".join(lines) + "\n"

    def render_namespace(self, header):
        lines = [*header, f"useDynLib({self.package}, .registration = TRUE)"]
        lines += [f"export({function['r']})" for function in self.functions]
        for name, cls in self.delegating_classes().items():
            if self.kind(name) == "handle":
                lines += [f"S3method({generic}, {cls['r']})" for generic in _R_NAMES]
            for member in cls["members"]:
                if member["kind"] == "index":
                    lines.append(f'S3method("{member["r"]}", {cls["r"]})')
                elif member["kind"] == "printing":
                    lines += [
                        f"S3method({generic}, {cls['r']})" for generic in member["r"]
                    ]
        return "\n".join(lines) + "\n"

    # --- Manifest ----------------------------------------------------------------

    def _result_facts(self, entry):
        facts = [f"returns {entry['returns']}"]
        if "table" in entry:
            facts.append(f"table {entry['table']}")
        if "record" in entry:
            facts.append(f"record {entry['record']}")
        if "raises" in entry:
            facts.append(f"raises {entry['raises']}")
        if "imports" in entry:
            facts.append(f"imports {', '.join(entry['imports'])}")
        return facts

    def _argument_lines(self, prefix, entry):
        keyword = set(entry.get("keyword_only", []))
        lines = []
        for index, (name, spec) in enumerate(entry.get("args", {}).items()):
            facts = f"{name}: {spec}" + ("; keyword-only" if name in keyword else "")
            lines.append(f"{prefix}.args[{index}]: {facts}")
        return lines

    def manifest_lines(self):
        """The frozen elements of the public surface, one per line."""
        lines = []
        for token, projection in self.public["types"].items():
            facts = [f"python {projection['python']}"]
            if "r" in projection:
                facts.append(f"r {projection['r']}")
            lines.append(f"public.types.{token}: {'; '.join(facts)}")
        for kind, projection in self.public["value_types"].items():
            lines.append(
                f"public.value_types.{kind}: python {projection['python']}; "
                f"r {projection['r']}"
            )
        for function in self.functions:
            prefix = f"public.functions.{function['name']}"
            facts = [f"python {function['python']}", f"r {function['r']}"]
            lines.append(f"{prefix}: {'; '.join(facts + self._result_facts(function))}")
            lines += self._argument_lines(prefix, function)
        for name, cls in self.classes.items():
            prefix = f"public.classes.{name}"
            if self.python_only(name):
                lines.append(f"{prefix}: python {cls['python']}")
            else:
                lines.append(f"{prefix}: python {cls['python']}; r {cls['r']}")
            for member in cls["members"]:
                if member["kind"] == "private":
                    continue
                facts = [member["kind"]]
                for host in ("python", "r"):
                    spelling = member.get(host)
                    if isinstance(spelling, list):
                        spelling = ", ".join(spelling)
                    if spelling is not None:
                        facts.append(f"{host} {spelling}")
                if "returns" in member:
                    facts += self._result_facts(member)
                lines.append(f"{prefix}.{member['name']}: {'; '.join(facts)}")
                lines += self._argument_lines(f"{prefix}.{member['name']}", member)
        error = self.errors["refusal"]
        python, r = error["python"], error["r"]
        attributes = ", ".join(
            f"{name}: {token}" for name, token in python["attributes"].items()
        )
        fields = ", ".join(f"{name}: {kind}" for name, kind in r["fields"].items())
        lines.append(
            f"public.errors.refusal: python {python['class']}({python['base']}) with "
            f"{attributes}; r {', '.join(r['class'])} with {fields}"
        )
        lines += [f"public.python.exports.{name}" for name in self.python_names()]
        lines += [f"public.r.exports.{function['r']}" for function in self.functions]
        for name, cls in self.delegating_classes().items():
            if self.kind(name) == "handle":
                listed = self._r_listed(name)
                facts = "keys of names" if listed is None else ", ".join(listed)
                lines += [
                    f"public.r.s3methods.{cls['r']}.names: {facts}",
                    f"public.r.s3methods.{cls['r']}.length",
                ]
            for member in cls["members"]:
                if member["kind"] == "index":
                    lines.append(f"public.r.s3methods.{cls['r']}.{member['r']}")
                elif member["kind"] == "printing":
                    lines += [
                        f"public.r.s3methods.{cls['r']}.{generic}"
                        for generic in member["r"]
                    ]
        return lines


# --- Formatting ----------------------------------------------------------------------


def _py_def(name, params, returns, indent):
    """A `def` line laid out as ruff format lays it out."""
    tail = f" -> {returns}:" if returns else ":"
    single = f"{indent}def {name}({params}){tail}"
    if len(single) <= _PY_WIDTH:
        return [single]
    inner = indent + "    "
    if len(inner + params) <= _PY_WIDTH:
        return [f"{indent}def {name}(", inner + params, f"{indent}){tail}"]
    return [
        f"{indent}def {name}(",
        *(f"{inner}{part}," for part in params.split(", ")),
        f"{indent}){tail}",
    ]


def _py_lines(expression, indent, prefix="", suffix=""):
    """`prefix + expression + suffix`, split at brackets as ruff format splits."""
    flat = f"{indent}{prefix}{expression}{suffix}"
    if (
        len(flat) <= _PY_WIDTH
        or not isinstance(expression, _Call)
        or not expression.arguments
    ):
        return [flat]
    inner = indent + "    "
    lines = [f"{indent}{prefix}{expression.function}("]
    arguments = expression.arguments
    joined = ", ".join(map(str, arguments))
    if len(arguments) == 1:
        lines += _py_lines(arguments[0], inner)
    elif len(inner + joined) <= _PY_WIDTH:
        lines.append(inner + joined)
    else:
        for argument in arguments:
            lines += _py_lines(argument, inner, suffix=",")
    lines.append(f"{indent}){suffix}")
    return lines


def _r_lines(expression, indent, prefix="", suffix=""):
    """`prefix + expression + suffix`, hugging single-argument calls when long."""
    flat = f"{indent}{prefix}{expression}{suffix}"
    if len(flat) <= _R_WIDTH or not isinstance(expression, _Call):
        return [flat]
    head, closers, node = "", "", expression
    while len(node.arguments) == 1 and isinstance(node.arguments[0], _Call):
        head += f"{node.function}("
        closers += ")"
        node = node.arguments[0]
    if not node.arguments:
        return [flat]
    head += f"{node.function}("
    closers += ")"
    inner = indent + "  "
    joined = ", ".join(map(str, node.arguments))
    lines = [f"{indent}{prefix}{head}"]
    if len(inner + joined) <= _R_WIDTH:
        lines.append(inner + joined)
    else:
        last = len(node.arguments) - 1
        for index, argument in enumerate(node.arguments):
            lines += _r_lines(argument, inner, suffix="" if index == last else ",")
    lines.append(f"{indent}{closers}{suffix}")
    return lines


def _r_closure(prefix, params, expression, indent, suffix=""):
    """`prefix` and `function(params)`, then the body: one line, or a block."""
    single = f"{prefix}function({params}) {expression}{suffix}"
    if len(single) <= _R_WIDTH:
        return [single]
    return [
        *_r_header(prefix, params),
        *_r_lines(expression, indent + "  "),
        f"{indent}}}{suffix}",
    ]


def _r_header(prefix, params):
    """`prefix` and `function(params) {`, with the parameters filled to the width."""
    opening = f"{prefix}function("
    if len(f"{opening}{params}) {{") <= _R_WIDTH or not params:
        return [f"{opening}{params}) {{"]
    align = " " * len(opening)
    parts = params.split(", ")
    lines, current, fresh = [], opening, True
    for index, part in enumerate(parts):
        piece = part + (") {" if index == len(parts) - 1 else ",")
        candidate = current + ("" if fresh else " ") + piece
        if len(candidate) > _R_WIDTH and not fresh:
            lines.append(current)
            current = align + piece
        else:
            current = candidate
        fresh = False
    return [*lines, current]
