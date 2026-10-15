"""Repository YAML scalar codecs; schema admission belongs to native Rust."""

import copy
import math
import re

import yaml
from tools.validation.editorial import UniqueKeyLoader


class UniqueKeyLoader(UniqueKeyLoader):
    """Keep the repository YAML 1.2 codec resolver table local."""

    yaml_implicit_resolvers = copy.deepcopy(UniqueKeyLoader.yaml_implicit_resolvers)


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
    """Apply R011's normalization at a numeric value boundary."""
    if type(value) is float and not math.isfinite(value):
        return None
    return value


def construct_yaml_12_float(loader, node):
    value = yaml.SafeLoader.construct_yaml_float(loader, node)
    return normalize_non_finite_float(value)


UniqueKeyLoader.add_constructor("tag:yaml.org,2002:float", construct_yaml_12_float)
