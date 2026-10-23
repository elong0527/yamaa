"""The command line's section of the host interface stays true to the binary.

`cli` in rust/interface/bindings.yaml declares what the `yamaa` binary takes
(#3060). These tests read the same facts from the binary's own sources: the
parser table `COMMANDS` and its flag constants in
rust/crates/yamaa-cli/src/arguments.rs, and the usage text and exit statuses
in main.rs. Neither side can then change alone. The JSON objects the commands
print are the facade's issue, lint finding, selftest case, catalogue
condition and engine information records, so their keys are the columns and
fields the hosts return.
"""

import re
import unittest

from tools.packaging import bindings_surface as surface
from tools.packaging import generate_bindings as generator
from tools.paths import WORKSPACE

SOURCES = WORKSPACE / "crates" / "yamaa-cli" / "src"
# A flag constant, such as `const SPEC: Flag = flag("--spec", Takes::Text);`.
FLAG = re.compile(r'const (\w+): Flag = flag\("([^"]+)", Takes::(\w+)\);')
TABLE = re.compile(r"static COMMANDS: \[Command; (\d+)\] = \[(.*?)\n\];", re.S)
ENTRY = re.compile(r"Command \{(.*?)\}", re.S)
STRING = r'"((?:[^"\\]|\\.)*)"'


def source(name):
    """A source file of the binary, without its comments."""
    return surface._strip_comments((SOURCES / name).read_text(encoding="utf-8"))


def text(literal):
    """The text of a Rust string literal's body. A backslash that ends a line
    skips the line break and the next line's leading whitespace."""
    literal = re.sub(r"\\\n\s*", "", literal)
    return re.sub(
        r"\\(.)", lambda escape: {"n": "\n"}.get(escape[1], escape[1]), literal
    )


def field(entry, name, pattern):
    """The value of one field of a `COMMANDS` entry."""
    found = re.search(rf"\b{name}: {pattern}", entry, re.S)
    if found is None:
        raise AssertionError(f"no {name} in the COMMANDS entry {entry.strip()[:40]!r}")
    return found[1]


def parsed_commands():
    """Each command of the parser table, in order: its synopsis, its
    positional arguments with whether each repeats, and its flags with
    whether each takes a value."""
    rust = source("arguments.rs")
    flags = {constant: (name, takes) for constant, name, takes in FLAG.findall(rust)}
    size, body = TABLE.search(rust).groups()
    commands = {}
    for entry in ENTRY.findall(body):
        arguments = re.findall(STRING, field(entry, "arguments", r"&\[([^\]]*)\]"))
        repeats = field(entry, "repeats", "(true|false)") == "true"
        constants = re.findall(r"\w+", field(entry, "flags", r"&\[([^\]]*)\]"))
        commands[text(field(entry, "name", STRING))] = {
            "synopsis": "yamaa " + text(field(entry, "synopsis", STRING)),
            "args": [
                (argument, repeats and index == len(arguments) - 1)
                for index, argument in enumerate(arguments)
            ],
            "flags": [
                (flags[constant][0], flags[constant][1] != "Nothing")
                for constant in constants
            ],
        }
    if len(commands) != int(size):
        raise AssertionError(f"read {len(commands)} of the {size} COMMANDS entries")
    return commands


def declared_commands(cli):
    """Each command `cli` declares, in the form of `parsed_commands`."""
    return {
        command["name"]: {
            "synopsis": command["synopsis"],
            "args": [
                (argument["name"], argument.get("repeat", False))
                for argument in command.get("args", [])
            ],
            "flags": [
                (flag["name"], "value" in flag or "values" in flag)
                for flag in command.get("flags", [])
            ],
        }
        for command in cli["commands"]
    }


class CommandLineTests(unittest.TestCase):
    def setUp(self):
        self.definition = generator._load_definition()
        self.cli = self.definition["cli"]

    def test_every_command_is_declared_as_the_parser_table_reads_it(self):
        parsed = parsed_commands()
        declared = declared_commands(self.cli)
        self.assertEqual(list(declared), list(parsed), "the commands, in order")
        for name, command in parsed.items():
            with self.subTest(command=name):
                self.assertEqual(declared[name], command)

    def test_the_global_flags_and_output_formats_are_the_parsers(self):
        rust = source("arguments.rs")
        parse = rust[rust.index("fn parse(") : rust.index("#[cfg(test)]")]
        # `--format` and `--version` are arms of the parser's match, and
        # `--help` is looked for before it.
        flags = re.findall(r'"(--[a-z][a-z-]*)" =>', parse)
        flags += re.findall(r'argument == "(--[a-z][a-z-]*)"', parse)
        variants = dict(re.findall(r'Some\("([a-z]+)"\) => Format::(\w+)', parse))
        default = re.search(r"format\.unwrap_or\(Format::(\w+)\)", parse)[1]
        declared = {flag["name"]: flag for flag in self.cli["global_flags"]}
        vocabulary = declared["--format"]["values"]
        self.assertEqual(
            {
                "flags": sorted(declared),
                "formats": self.cli["values"][vocabulary],
                "default": declared["--format"]["default"],
            },
            {
                "flags": sorted(flags),
                "formats": list(variants),
                "default": next(
                    name for name, variant in variants.items() if variant == default
                ),
            },
        )

    def test_the_synopses_are_the_usage_the_binary_prints(self):
        rust = source("main.rs")
        usage = text(
            re.search(r"fn help\(\).*?String::from\(\s*" + STRING, rust, re.S)[1]
        )
        usage = usage.removeprefix("usage:").split("\n\ncommands:\n")[0]
        self.assertEqual(
            [line.strip() for line in usage.split("\n")], self.cli["synopsis"]
        )

    def test_the_exit_statuses_are_the_binarys(self):
        statuses = re.findall(r"const [A-Z_]+: u8 = (\d+);", source("main.rs"))
        self.assertEqual(sorted(map(int, statuses)), sorted(self.cli["exit_status"]))

    def test_the_json_objects_hold_the_columns_and_fields_the_hosts_return(self):
        def columns(table):
            return [
                column["name"] for column in self.definition["tables"][table]["columns"]
            ]

        info = self.definition["records"]["info"]["fields"]
        json = self.cli["json"]
        self.assertEqual(
            {
                key: json[key]
                for key in (
                    "issue_keys",
                    "finding_keys",
                    "case_keys",
                    "condition_keys",
                    "info_keys",
                )
            },
            {
                "issue_keys": columns("issues"),
                "finding_keys": columns("lint"),
                "case_keys": columns("selftest_cases"),
                "condition_keys": columns("conditions"),
                "info_keys": [field["name"] for field in info],
            },
        )


if __name__ == "__main__":
    unittest.main()
