"""The one loader tools read repository YAML with.

`UniqueKeyLoader` reads the YAML 1.2 core schema the engine reads: `yes` and
`on` are text, `0o12` is octal, a date is text, and a non-finite float
becomes None, as at a numeric value boundary. It refuses what the repository's
YAML never uses: duplicate or unhashable keys, aliases, anchors, explicit tags
and merge keys. Schema admission belongs to native Rust.
"""

import copy
import math
import re
from collections.abc import Hashable

import yaml

# Tools run from the repository root, whose yaml/ directory holds the
# specification schemas. Without PyYAML installed, `import yaml` finds that
# directory as an empty namespace package, and the loader below would fail on
# a missing attribute; refuse whatever stands in for PyYAML by name.
if not (hasattr(yaml, "__version__") and hasattr(yaml, "SafeLoader")):
    found = (
        getattr(yaml, "__file__", None)
        or ", ".join(getattr(yaml, "__path__", []))
        or repr(yaml)
    )
    raise ImportError(
        f"{__name__} needs PyYAML, but `import yaml` found {found}, "
        "which is not PyYAML: install the locked tooling environment with "
        "`uv sync --project python --locked --group tooling --no-install-package yamaa`"
    )


class UniqueKeyLoader(yaml.SafeLoader):
    yaml_implicit_resolvers = copy.deepcopy(yaml.SafeLoader.yaml_implicit_resolvers)

    def compose_node(self, parent, index):
        event = self.peek_event()
        if isinstance(event, yaml.events.AliasEvent):
            raise yaml.composer.ComposerError(
                None,
                None,
                "YAML aliases are not allowed",
                event.start_mark,
            )
        if getattr(event, "anchor", None) is not None:
            raise yaml.composer.ComposerError(
                None,
                None,
                "YAML anchors are not allowed",
                event.start_mark,
            )
        if getattr(event, "tag", None) is not None:
            raise yaml.composer.ComposerError(
                None,
                None,
                "explicit YAML tags are not allowed",
                event.start_mark,
            )
        return super().compose_node(parent, index)

    def flatten_mapping(self, node):
        for key_node, _ in node.value:
            if key_node.tag == "tag:yaml.org,2002:merge":
                raise yaml.constructor.ConstructorError(
                    None,
                    None,
                    "YAML merge keys are not allowed",
                    key_node.start_mark,
                )
        return super().flatten_mapping(node)

    def construct_mapping(self, node, deep=False):
        # SafeConstructor.construct_mapping starts with this call; without it
        # the merge-key guard in flatten_mapping above never runs and a merge
        # key dies later with "could not determine a constructor" instead of
        # the intended "YAML merge keys are not allowed".
        self.flatten_mapping(node)
        mapping = {}
        for key_node, value_node in node.value:
            key = self.construct_object(key_node, deep=deep)
            if not isinstance(key, Hashable):
                # SafeConstructor raises this for a sequence or mapping used
                # as a key; without the guard the membership test below dies
                # with a raw TypeError that escapes the callers catching
                # yaml.YAMLError and crashes the whole validation run.
                raise yaml.constructor.ConstructorError(
                    None,
                    None,
                    "found unhashable key",
                    key_node.start_mark,
                )
            if key in mapping:
                raise yaml.constructor.ConstructorError(
                    "while constructing a mapping",
                    node.start_mark,
                    f"found duplicate key '{key}'",
                    key_node.start_mark,
                )
            mapping[key] = self.construct_object(value_node, deep=deep)
        return mapping


for first_char, resolvers in list(UniqueKeyLoader.yaml_implicit_resolvers.items()):
    UniqueKeyLoader.yaml_implicit_resolvers[first_char] = [
        (tag, regexp)
        for tag, regexp in resolvers
        if tag
        not in {
            "tag:yaml.org,2002:bool",
            "tag:yaml.org,2002:float",
            "tag:yaml.org,2002:int",
            "tag:yaml.org,2002:timestamp",
        }
    ]


UniqueKeyLoader.add_implicit_resolver(
    "tag:yaml.org,2002:bool",
    re.compile(r"^(?:true|True|TRUE|false|False|FALSE)$"),
    list("tTfF"),
)


UniqueKeyLoader.add_implicit_resolver(
    "tag:yaml.org,2002:int",
    re.compile(r"^(?:[-+]?[0-9]+|0o[0-7]+|0x[0-9a-fA-F]+)$"),
    list("-+0123456789"),
)


UniqueKeyLoader.add_implicit_resolver(
    "tag:yaml.org,2002:float",
    re.compile(
        r"^(?:"
        r"[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)(?:[eE][-+]?[0-9]+)?"
        r"|[-+]?\.(?:inf|Inf|INF)"
        r"|\.(?:nan|NaN|NAN)"
        r")$"
    ),
    list("-+0123456789."),
)


def construct_yaml_12_int(loader, node):
    value = loader.construct_scalar(node)
    sign = 1
    if value.startswith("-"):
        sign = -1
    if value.startswith(("-", "+")):
        value = value[1:]
    if value.startswith("0o"):
        return sign * int(value[2:], 8)
    if value.startswith("0x"):
        return sign * int(value[2:], 16)
    return sign * int(value, 10)


UniqueKeyLoader.add_constructor("tag:yaml.org,2002:int", construct_yaml_12_int)


def normalize_non_finite_float(value):
    """Apply non-finite normalization at a numeric value boundary."""
    if type(value) is float and not math.isfinite(value):
        return None
    return value


def construct_yaml_12_float(loader, node):
    value = yaml.SafeLoader.construct_yaml_float(loader, node)
    return normalize_non_finite_float(value)


UniqueKeyLoader.add_constructor("tag:yaml.org,2002:float", construct_yaml_12_float)
