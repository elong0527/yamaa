"""The `handshake` section of the host interface and the code it generates.

``rust/interface/bindings.yaml`` (format 2) declares the interface handshake
of both hosts under ``handshake`` (``rust/interface/README.md``, "Version and
feature handshake"). ``tools/packaging/generate_bindings.py`` validates the
section here and renders, through this module:

- the head of ``python/src/yamaa/__init__.py``: ``InterfaceError``, a subclass
  of ``ImportError``, the request, one message template per reason, and the
  negotiation that runs right after ``_native`` is imported, before any other
  native call;
- the end of ``R/yamaa/R/native-bindings.R``: the request, the templates,
  the ``yamaa_interface_error`` condition, the negotiation and the ``.onLoad``
  that runs it.

The request's version is ``yamaa_boundary::BOUNDARY_VERSION``, read from the
boundary crate's source, so no second copy of the number exists. Every
identifier the section names is one of the crate's ``CAPABILITIES`` or
``FORMATS``, and its optional identifiers are exactly the formats, each with
the Cargo feature that compiles it.

The section also owns the gate: every native function that a binding
registers must reach the boundary's gate (``GATE.pass()``), directly or
through the binding's own functions, apart from the handshake itself and the
informational core version. ``check_gate`` reads the binding sources and
refuses an export that does not, so a new export cannot skip negotiation.
"""

import re

from tools.paths import WORKSPACE

BOUNDARY = WORKSPACE / "crates" / "yamaa-boundary" / "src" / "lib.rs"

_KEYS = {
    "version",
    "python",
    "r",
    "python_error",
    "r_error",
    "required_features",
    "optional_features",
    "messages",
}
# Each reason of the refusal envelope with the fields its message may name.
_REASONS = {
    "version_mismatch": {"requested_version", "supported_version"},
    "malformed_request": {"supported_version"},
    "missing_feature": {"requested_version", "supported_version", "missing_features"},
    "missing_interface": {"requested_version"},
}
_IDENTIFIER = re.compile(r"[a-z][a-z0-9_]*")
_PLACEHOLDER = re.compile(r"\{([a-z_]+)\}")
_PY_WIDTH = 88
_FN = re.compile(r"\bfn\s+([A-Za-z_]\w*)")
_CALL = re.compile(r"\b([A-Za-z_]\w*)\s*\(")
_GATE = re.compile(r"\bGATE\s*\.\s*pass\s*\(")


def boundary_version():
    """The boundary version that yamaa-boundary's lib.rs declares."""
    source = BOUNDARY.read_text(encoding="utf-8")
    match = re.search(r"pub const BOUNDARY_VERSION: u32 = ([1-9][0-9]*);", source)
    if match is None:
        raise ValueError("yamaa-boundary declares no BOUNDARY_VERSION")
    return int(match.group(1))


def identifiers():
    """yamaa-boundary's capability identifiers, and its format identifiers
    with the feature of each, in order."""
    source = BOUNDARY.read_text(encoding="utf-8")
    capabilities = re.search(
        r"pub const CAPABILITIES: \[&str; \d+\] = \[(.*?)\];", source, re.S
    )
    formats = re.search(
        r"pub const FORMATS: \[\(&str, &str\); \d+\] = \[(.*?)\];", source, re.S
    )
    if capabilities is None or formats is None:
        raise ValueError("yamaa-boundary declares no CAPABILITIES or FORMATS")
    return (
        re.findall(r'"([a-z0-9_]+)"', capabilities.group(1)),
        re.findall(r'\("([a-z0-9_]+)",\s*"([a-z0-9_]+)"\)', formats.group(1)),
    )


def validate(definition):
    """Check the handshake section against the native sections and the
    boundary crate; return it."""
    handshake = definition["handshake"]
    if not isinstance(handshake, dict) or set(handshake) != _KEYS:
        raise ValueError(f"the handshake section must hold exactly {sorted(_KEYS)}")
    if handshake["version"] != "boundary.version":
        raise ValueError("the handshake version must be boundary.version")
    python = {
        export["name"]
        for export in definition["python"]["exports"]
        if export["kind"] == "function"
    }
    r = {
        function["name"]
        for group in definition["r"]["groups"]
        for function in group["functions"]
    }
    for host, exports in (("python", python), ("r", r)):
        if handshake[host] not in exports:
            raise ValueError(
                f"the handshake names {handshake[host]}, no {host} function"
            )
    if not re.fullmatch(r"[A-Z][A-Za-z]*Error", str(handshake["python_error"])):
        raise ValueError("the handshake's python_error must be a Python error class")
    if not _IDENTIFIER.fullmatch(str(handshake["r_error"])):
        raise ValueError("the handshake's r_error must be an R condition class")
    capabilities, formats = identifiers()
    known = capabilities + [identifier for identifier, _ in formats]
    required = handshake["required_features"]
    optional = handshake["optional_features"]
    if not isinstance(required, list) or not required:
        raise ValueError("the handshake's required_features must be a nonempty list")
    for identifier in required:
        if identifier not in known:
            raise ValueError(f"{identifier!r} is no identifier of yamaa-boundary")
    expected = {
        identifier: f"yamaa-adapters/{feature}" for identifier, feature in formats
    }
    if not isinstance(optional, dict) or list(optional.items()) != list(
        expected.items()
    ):
        raise ValueError(
            "the handshake's optional_features must be yamaa-boundary's formats, "
            "in order, each with its yamaa-adapters feature"
        )
    names = required + list(optional)
    if len(set(names)) != len(names):
        raise ValueError("the handshake names an identifier twice")
    messages = handshake["messages"]
    if not isinstance(messages, dict) or set(messages) != set(_REASONS):
        raise ValueError(f"the handshake's messages must be {sorted(_REASONS)}")
    for reason, template in messages.items():
        if not isinstance(template, str) or not template.strip():
            raise ValueError(f"the {reason} message must be nonempty text")
        if '"' in template or "\\" in template:
            raise ValueError(f"the {reason} message cannot hold a quote or backslash")
        fields = set(_PLACEHOLDER.findall(template))
        rest = _PLACEHOLDER.sub("", template)
        if not fields <= _REASONS[reason] or "{" in rest or "}" in rest:
            raise ValueError(
                f"the {reason} message may name only {sorted(_REASONS[reason])}"
            )
    return handshake


def _request(handshake):
    """The request: the version, then the required and optional identifiers."""
    return (
        boundary_version(),
        handshake["required_features"],
        list(handshake["optional_features"]),
    )


def _py_items(key, items, indent):
    """One dict entry holding a list of text, as ruff format lays it out."""
    quoted = [f'"{item}"' for item in items]
    single = f'{indent}"{key}": [{", ".join(quoted)}],'
    if len(single) <= _PY_WIDTH:
        return [single]
    return [
        f'{indent}"{key}": [',
        *(f"{indent}    {item}," for item in quoted),
        f"{indent}],",
    ]


def python_lines(definition):
    """The handshake block of the generated Python module."""
    handshake = definition["handshake"]
    version, required, optional = _request(handshake)
    error = handshake["python_error"]
    fields = (
        "accepted",
        "requested_version",
        "supported_version",
        "reason",
        "missing_features",
    )
    return [
        f"class {error}(ImportError):",
        '    """yamaa\'s Python code and its native module do not match, so it '
        'cannot be imported; the attributes are the handshake\'s refusal."""',
        "",
        "    def __init__(self, outcome: dict):",
        '        super().__init__(_interface_message(outcome), name="yamaa")',
        *(f'        self.{field} = outcome["{field}"]' for field in fields),
        "",
        "",
        "_INTERFACE_REQUEST = {",
        f'    "version": {version},',
        *_py_items("required", required, "    "),
        *_py_items("optional", optional, "    "),
        "}",
        "_INTERFACE_MESSAGES = {",
        *(
            f'    "{reason}": "{template}",'
            for reason, template in handshake["messages"].items()
        ),
        "}",
        "",
        "",
        "def _interface_message(outcome: dict) -> str:",
        '    """A refused handshake\'s message: the template of its reason, '
        'filled from the refusal."""',
        '    message = _INTERFACE_MESSAGES[outcome["reason"]]',
        '    for field in ("requested_version", "supported_version"):',
        "        if outcome[field] is not None:",
        '            message = message.replace("{" + field + "}", str(outcome[field]))',
        '    features = ", ".join(outcome["missing_features"])',
        '    return message.replace("{missing_features}", features)',
        "",
        "",
        "def _negotiate(native) -> None:",
        '    """Send the interface request before any other native call, and '
        'refuse the import unless the native module accepts it."""',
        f'    handshake = getattr(native, "{handshake["python"]}", None)',
        "    if handshake is None:",
        "        outcome = {",
        '            "accepted": False,',
        '            "requested_version": _INTERFACE_REQUEST["version"],',
        '            "supported_version": None,',
        '            "reason": "missing_interface",',
        '            "missing_features": [],',
        "        }",
        "    else:",
        "        outcome = handshake(_INTERFACE_REQUEST)",
        '    if not outcome["accepted"]:',
        f"        raise {error}(outcome)",
        "",
        "",
        "_negotiate(_native)",
    ]


def _r_text(items):
    return ", ".join(f'"{item}"' for item in items)


def r_lines(definition):
    """The handshake block of the generated R native wrappers."""
    handshake = definition["handshake"]
    version, required, optional = _request(handshake)
    messages = list(handshake["messages"].items())
    return [
        "# The interface handshake (rust/interface/README.md): .onLoad sends this",
        "# request before any other native call, and a refusal stops the load.",
        ".interface_request <- list(",
        f"  version = {version}L,",
        f"  required = c({_r_text(required)}),",
        f"  optional = c({_r_text(optional)})",
        ")",
        "",
        "# One message per reason, filled from the refusal envelope.",
        ".interface_messages <- c(",
        *(
            f'  {reason} = "{template}"' + ("," if index < len(messages) - 1 else "")
            for index, (reason, template) in enumerate(messages)
        ),
        ")",
        "",
        f"# The {handshake['r_error']} of a refused handshake, carrying the",
        "# envelope's fields.",
        ".interface_error <- function(outcome) {",
        "  message <- .interface_messages[[outcome$reason]]",
        '  for (field in c("requested_version", "supported_version")) {',
        "    if (!is.null(outcome[[field]])) {",
        '      message <- gsub(paste0("{", field, "}"),',
        "        format(outcome[[field]], scientific = FALSE), message, fixed = TRUE)",
        "    }",
        "  }",
        '  message <- gsub("{missing_features}",',
        '    paste(outcome$missing_features, collapse = ", "), message, fixed = TRUE)',
        "  structure(c(list(message = message, call = NULL), outcome),",
        f'    class = c("{handshake["r_error"]}", "error", "condition"))',
        "}",
        "",
        "# Negotiate through `routine`, which gives the function that calls one of",
        "# this package's native routines, or NULL where its library has none.",
        ".interface_negotiate <- function(routine) {",
        f'  handshake <- routine("{handshake["r"]}")',
        "  if (is.null(handshake)) {",
        "    outcome <- list(accepted = FALSE, requested_version = "
        ".interface_request$version,",
        '      supported_version = NULL, reason = "missing_interface",',
        "      missing_features = character())",
        "  } else {",
        "    outcome <- .reply(handshake(.interface_request))",
        "  }",
        "  if (!isTRUE(outcome$accepted)) stop(.interface_error(outcome))",
        "  invisible(outcome)",
        "}",
        "",
        ".onLoad <- function(libname, pkgname) {",
        "  namespace <- topenv()",
        "  .interface_negotiate(function(name) {",
        '    if (exists(paste0("wrap__", name), envir = namespace, inherits = FALSE)) {',
        '      get(paste0(".native_", name), envir = namespace, inherits = FALSE)',
        "    }",
        "  })",
        "}",
    ]


def free_functions(paths, strip, block):
    """Each top-level function of the binding sources at `paths`, by name: the
    names its body calls, and whether it calls the boundary's gate.

    `strip` blanks comments and literals and `block` returns a brace block's
    body (generate_bindings.py). Functions of one name in several modules
    count as one.
    """
    functions = {}
    for path in paths:
        text = strip(path.read_text(encoding="utf-8"))
        depth = index = 0
        while index < len(text):
            char = text[index]
            if char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
            elif depth == 0 and (match := _FN.match(text, index)):
                body, index = block(text, text.index("{", match.end()))
                calls, gated = functions.setdefault(match.group(1), (set(), []))
                calls.update(_CALL.findall(body))
                gated.append(bool(_GATE.search(body)))
                continue
            index += 1
    return {name: (calls, any(gated)) for name, (calls, gated) in functions.items()}


def check_gate(host, registered, exempt, functions):
    """Refuse a registered native function of `host` that does not reach the
    gate, directly or through the binding's own functions."""
    reached = {name for name, (_, gated) in functions.items() if gated}
    if not reached:
        raise ValueError(f"the {host} binding has no function that passes the gate")
    growing = True
    while growing:
        growing = False
        for name, (calls, _) in functions.items():
            if name not in reached and calls & reached:
                reached.add(name)
                growing = True
    unreached = sorted(set(registered) - set(exempt) - reached)
    if unreached:
        raise ValueError(
            f"{host} native functions that do not reach the interface gate: "
            + ", ".join(unreached)
        )
