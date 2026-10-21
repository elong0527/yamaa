"""The embedded converter (tools.migrate.embedded): YAML inside test sources and fixtures.

Each format's tests convert a small source and read the result back. The
corpus tests are #3166's acceptance, on a copy of the repository's own test
sources: every converted document has the YAML values the file converter
gives for the same document, a second run changes nothing, and every string
built by interpolation whose YAML the conversion would change is listed.
"""

import io
import json
import re
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock

import yaml
from tools.migrate import convert as convert_files
from tools.migrate import embedded, fixtures, sources
from tools.migrate import table as table_module
from tools.migrate.text import compose, value_of
from tools.paths import ROOT

SPEC = (
    "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\n"
    "input: {SRC: {path: input.csv, types: {V: int}}}\n"
    "output: {path: result.csv, columns: [ID, VALUE]}\n"
    "columns:\n  - {name: ID, type: str, derivation: SRC.ID}\n"
    "  - {name: VALUE, type: int, derivation: {literal: 7}}\n"
)
CONVERTED = SPEC.replace(
    "schema_version: '1.0'\n", 'yamaa: "1.0"\ntype: spec\n'
).replace("{literal: 7}", "7")
LOOKUP = (
    "schema_version: '1.0'\ndomain: TEST\nkeys: [ID]\ninput: {SRC: src.csv}\n"
    "output: {path: out.csv, columns: [ID]}\n"
    "intermediates:\n  - {id: FIRST, dataset: SRC, no_match: {literal: null}}\n"
    "columns:\n  - {name: ID, type: str, derivation: SRC.ID}\n"
)


def run(name, text):
    """Convert one source; return its converted text and the Result."""
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / name
        path.write_text(text, encoding="utf-8")
        result = embedded.convert_embedded([path], base=directory)
    return result.files.get(name, text), result


def notes(result, kind=None):
    return [note for note in result.notes if kind is None or note.kind == kind]


def rust_string(text):
    return (
        '"' + text.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n") + '"'
    )


def values(text):
    return value_of(compose(text))


class RustTests(unittest.TestCase):
    def test_concat_pieces_are_listed_without_converting_partial_documents(self):
        header, columns = SPEC.split("columns:", 1)
        source = (
            "fn t() {\n    let text = concat!(\n"
            f"        {rust_string(header)},\n"
            f"        {rust_string('columns:' + columns)},\n"
            "    );\n}\n"
        )
        converted, result = run("t.rs", source)
        self.assertEqual(converted, source)
        self.assertEqual([note.line for note in notes(result, "interpolated")], [3, 4])

    def test_a_document_converts_in_its_own_literal_style(self):
        raw = SPEC.replace("'1.0'", '"1.0"')
        doubled = SPEC.replace("{", "{{").replace("}", "}}")
        source = (
            "fn t() {\n"
            f"    let a = {rust_string(SPEC)};\n"
            f'    let b = r#"{raw}"#;\n'
            f"    let c = b{rust_string(SPEC)};\n"
            f"    let d = format!({rust_string(doubled)});\n"
            "}\n"
        )
        converted, result = run("t.rs", source)
        found = [group.text for group in sources.rust_extraction(converted).groups]
        self.assertEqual(
            found[:3], [CONVERTED, CONVERTED.replace("'1.0'", '"1.0"'), CONVERTED]
        )
        self.assertIn('let a = "yamaa: \\"1.0\\"\\ntype: spec\\n', converted)
        self.assertIn('let b = r#"yamaa: "1.0"\ntype: spec\n', converted)
        self.assertIn('let c = b"yamaa:', converted)
        self.assertEqual(result.stats["documents converted"], 3)
        # A format string holding braces is a template, even with no placeholder.
        self.assertIn(f"let d = format!({rust_string(doubled)});", converted)
        line = next(
            number
            for number, text in enumerate(converted.splitlines(), start=1)
            if "let d" in text
        )
        self.assertEqual([note.line for note in notes(result, "interpolated")], [line])

    def test_an_interpolated_string_is_listed_and_left(self):
        source = (
            "fn t(v: &str) -> String {\n"
            f"    let a = format!({rust_string(SPEC + '  - {{name: X, derivation: {v}}}\\n')});\n"
            f"    let b = text + {rust_string('derivation: {compute: {expr: X}}')};\n"
            f"    let mut c = String::from({rust_string(SPEC)});\n"
            "    c.push_str(v);\n"
            f'    format!("{{v}}/environment.yaml")\n'
            "}\n"
        )
        converted, result = run("t.rs", source)
        self.assertEqual(converted, source)
        listed = [note.line for note in notes(result, "interpolated")]
        self.assertEqual(listed, [2, 3, 4])

    def test_a_file_name_beside_a_bare_header_gives_its_type(self):
        source = (
            "fn t() {\n"
            '    study.write("project.yaml", "schema_version: \'1.0\'\\n");\n'
            '    study.write("python/environment.yaml", "schema_version: \'1.0\'\\n");\n'
            "    run(\"schema_version: '1.0'\\n\");\n"
            "}\n"
        )
        converted, result = run("t.rs", source)
        lines = converted.splitlines()
        self.assertIn('"yamaa: \\"1.0\\"\\ntype: project\\n"', lines[1])
        self.assertEqual(lines[2], source.splitlines()[2])
        self.assertIn('"yamaa: \\"1.0\\"\\ntype: spec\\n"', lines[3])
        self.assertEqual([note.line for note in notes(result, "header")], [4])
        self.assertEqual([note.line for note in notes(result, "D8")], [3])

    def test_a_pattern_is_listed_and_a_constant_converts(self):
        source = (
            f"const VALID: &str = {rust_string(SPEC)};\n"
            "fn t() {\n"
            '    let text = VALID.replace("{literal: 7}", "{literal: 8}");\n'
            "}\n"
        )
        converted, result = run("t.rs", source)
        self.assertIn(rust_string(CONVERTED), converted)
        self.assertIn('VALID.replace("{literal: 7}", "{literal: 8}")', converted)
        self.assertEqual([note.line for note in notes(result, "pattern")], [3, 3])

    def test_spec_paths_follow_the_specification_beside_them(self):
        source = (
            f"const BODY: &str = {rust_string(LOOKUP)};\n"
            "fn one() {\n"
            '    check(BODY, &["intermediates[0].no_match", "columns.ID.derivation.compute.expr"]);\n'
            "}\n"
            "fn two() {\n"
            '    check(OTHER, &["rows[0].filter"]);\n'
            "}\n"
        )
        converted, result = run("t.rs", source)
        self.assertIn('"intermediates.FIRST.absent"', converted)
        self.assertIn('"columns.ID.derivation"', converted)
        self.assertIn('"rows[0].filter"', converted)
        self.assertEqual([note.line for note in notes(result, "path")], [6])

    def test_a_path_inside_a_longer_string_is_listed(self):
        source = (
            "fn t(i: usize) {\n"
            '    let report = "out.csv,check,columns.X.derivation.compute.expr,,";\n'
            '    let path = format!("rows[{i}].filter");\n'
            '    let name = "SRC.literal_count";\n'
            "}\n"
        )
        converted, result = run("t.rs", source)
        self.assertEqual(converted, source)
        self.assertEqual([note.line for note in notes(result, "path")], [2, 3])


class GuardTests(unittest.TestCase):
    def test_an_unsupported_source_reports_a_cli_error(self):
        from tools.migrate.__main__ import main

        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "spec.yaml"
            path.write_text(SPEC, encoding="utf-8")
            errors = io.StringIO()
            with mock.patch("sys.stderr", errors):
                status = main(["--embedded", str(path), "--output", directory + "-out"])
            self.assertEqual(status, 2)
            self.assertEqual(
                errors.getvalue(),
                f"error: {path} is not a Rust, Python, R, JSON or TSV file\n",
            )

    def test_a_string_the_file_converter_fails_on_is_listed_and_left(self):
        source = f"fn t() {{\n    let a = {rust_string(SPEC)};\n}}\n"
        with mock.patch.object(embedded, "convert_files", side_effect=TypeError("odd")):
            converted, result = run("t.rs", source)
        self.assertEqual(converted, source)
        self.assertEqual(
            [(note.kind, note.line) for note in result.notes], [("internal", 2)]
        )


class PieceTests(unittest.TestCase):
    """Writing a converted text back over the pieces it was spelt with."""

    def test_a_boundary_in_unchanged_text_keeps_its_pieces(self):
        original = "columns: [{name: A, derivation: {literal: 1}}, {name: B}]\n"
        converted = "columns: [{name: A, derivation: 1}, {name: B}]\n"
        cut = original.index("{name: B}")
        (mapped,) = sources.map_offsets(original, converted, [cut])
        self.assertEqual(converted[mapped:], "{name: B}]\n")

    def test_a_boundary_in_changed_text_joins_its_pieces(self):
        original = "a: {literal: 1}\nb: 2\n"
        style = sources.PythonStyle()
        pieces, start = [], 0
        for part in ("a: {lit", "eral: 1}\n", "b: 2\n"):
            pieces.append(
                sources.Piece(start, start + 1, part, style, [start] * len(part))
            )
            start += 2
        group = sources.Group("python", pieces, 0, start)
        self.assertEqual(group.text, original)
        edits = group.rewrite("a: 1\nb: 2\n")
        self.assertEqual(edits, [(0, 3, '"a: 1\\n"')])

    def test_an_inserted_line_joins_the_piece_before_it(self):
        original = "yamaa_header\nrest\n"
        (mapped,) = sources.map_offsets(original, "yamaa_header\nnew\nrest\n", [13])
        self.assertEqual(mapped, len("yamaa_header\nnew\n"))


class PythonTests(unittest.TestCase):
    def test_only_the_pieces_that_change_are_rewritten(self):
        source = (
            "def test():\n"
            "    (root / 'spec.yaml').write_text(\n"
            "        \"schema_version: '1.0'\\ndomain: TEST\\nkeys: [ID]\\n\"\n"
            '        "input: {SRC: {path: input.csv, types: {V: int}}}\\n"\n'
            '        "output: {path: result.csv, columns: [ID, VALUE]}\\n"\n'
            '        "columns:\\n  - {name: ID, type: str, derivation: SRC.ID}\\n"\n'
            "        '  - {name: VALUE, type: int, derivation: {literal: 7}}\\n'\n"
            "    )\n"
        )
        converted, _ = run("t.py", source)
        before, after = source.splitlines(), converted.splitlines()
        self.assertEqual(
            after[2],
            "        'yamaa: \"1.0\"\\ntype: spec\\ndomain: TEST\\nkeys: [ID]\\n'",
        )
        self.assertEqual(after[3:6], before[3:6])
        self.assertEqual(
            after[6], '        "  - {name: VALUE, type: int, derivation: 7}\\n"'
        )
        (group,) = [
            group
            for group in sources.python_extraction(converted).groups
            if "yamaa" in group.text
        ]
        self.assertEqual(group.text, CONVERTED)

    def test_templates_are_listed(self):
        source = (
            "def test(v):\n"
            f"    a = f{json.dumps(SPEC)}\n"
            f"    b = {json.dumps(SPEC + '{}')}.format(v)\n"
            f"    c = {json.dumps(SPEC + '%s')} % v\n"
            f"    d = {json.dumps(SPEC)} + v\n"
            "    e = f'{v}.csv'\n"
        )
        converted, result = run("t.py", source)
        self.assertEqual(converted, source)
        self.assertEqual(
            [note.line for note in notes(result, "interpolated")], [2, 3, 4, 5]
        )


class RTests(unittest.TestCase):
    def test_raw_strings_stay_whole_and_do_not_hide_later_literals(self):
        for prefix, opening, closing in (
            ('r"(', 'r"(', ')"'),
            ('R"--[', 'R"--[', ']--"'),
            ("r'{", "r'{", "}'"),
        ):
            raw = opening + SPEC + '# "quoted" \\n\n' + closing
            ordinary = sources.RStyle().encode(SPEC)
            source = f"raw <- {raw}\nordinary <- {ordinary}\n"
            with self.subTest(prefix=prefix):
                converted, result = run("t.R", source)
                self.assertIn(f"raw <- {raw}\n", converted)
                self.assertEqual(result.stats["documents converted"], 1)
                self.assertEqual(
                    [group.text for group in sources.r_extraction(converted).groups],
                    [CONVERTED],
                )

    def test_a_vector_of_lines_gains_and_loses_elements(self):
        source = (
            'writeLines(c("schema_version: \'1.0\'", "domain: TEST", "keys: [ID]",\n'
            '  "intermediates:", "  - {id: FIRST, dataset: SRC, no_match: {literal: null}}"), "spec.yaml")\n'
            'writeLines(c("schema_version: \'1.0\'", paste0("domain: ", name)), path)\n'
            'writeLines(c("a", paste0("{literal: 1}", x)), path)\n'
        )
        converted, result = run("t.R", source)
        lines = converted.splitlines()
        self.assertTrue(
            lines[0].startswith(
                'writeLines(c(\'yamaa: "1.0"\', "type: spec", "domain: TEST"'
            )
        )
        self.assertIn('"  - {id: FIRST, dataset: SRC}"', lines[1])
        self.assertEqual(lines[2:], source.splitlines()[2:])
        self.assertEqual([note.line for note in notes(result, "interpolated")], [3, 4])
        (group,) = [
            group
            for group in sources.r_extraction(converted).groups
            if group.lines and "yamaa" in group.text
        ]
        self.assertEqual(values(group.text)["type"], "spec")


class FixtureTests(unittest.TestCase):
    def test_a_json_case_converts_with_the_files_it_names(self):
        parent = (
            "schema_version: '1.0'\ndomain: NC\nkeys: [ID]\nbase: OBS\n"
            "input:\n  OBS: {path: obs.csv, types: {ID: int}}\n  REF: ref.csv\n"
            "output: {path: result.csv, columns: [ID, LABEL]}\n"
            "intermediates:\n  - id: LOOK\n    dataset: REF\n    no_match: {literal: null}\n"
            "columns:\n  - {name: ID, type: int, derivation: OBS.ID}\n"
            "  - {name: LABEL, type: str, derivation: LOOK.LABEL}\n"
        )
        child = "schema_version: '1.0'\nparent: parent.yaml\nintermediates: [{id: LOOK, no_match: null}]\n"
        case = {
            "spec": child,
            "parents": {"parent.yaml": parent},
            "inputs": {"obs.csv": "ID,CODE\n1,A\n", "ref.csv": "ID,LABEL\n1,Alpha\n"},
            "spec_paths": ["intermediates[0].no_match"],
        }
        source = json.dumps({"cases": [case]}, indent=2) + "\n"
        converted, _ = run("cases.json", source)
        (written,) = json.loads(converted)["cases"]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name, text in {
                "spec.yaml": child,
                "parent.yaml": parent,
                **case["inputs"],
            }.items():
                (root / name).write_text(text, encoding="utf-8")
            reference = convert_files(
                [root / "spec.yaml", root / "parent.yaml"], base=root
            )
        self.assertEqual(values(written["spec"]), values(reference.files["spec.yaml"]))
        self.assertEqual(
            values(written["parents"]["parent.yaml"]),
            values(reference.files["parent.yaml"]),
        )
        self.assertIn("clear:\n  - intermediates.LOOK.absent", written["spec"])
        self.assertEqual(written["spec_paths"], ["intermediates.LOOK.absent"])
        self.assertEqual(converted, json.dumps({"cases": [written]}, indent=2) + "\n")

    def test_an_environment_in_a_case_converts_alone(self):
        environment = "schema_version: '1.0'\nlock: uv.lock\nfunctions: {}\n"
        case = {
            "spec": LOOKUP,
            "inputs": {"src.csv": "ID,V\n1,2\n"},
            "files": {"environment.yaml": environment},
        }
        converted, result = run(
            "cases.json", json.dumps({"case": case}, indent=2) + "\n"
        )
        written = json.loads(converted)["case"]
        self.assertIn("key: [ID]", written["spec"])
        self.assertEqual(written["files"]["environment.yaml"], environment)
        self.assertEqual({note.kind for note in result.notes}, {"D8"})

    def test_a_json_specification_keeps_its_layout(self):
        spec = values(SPEC)
        source = (
            json.dumps({"case": {"specification": spec, "csv": "ID\n"}}, indent=2)
            + "\n"
        )
        converted, _ = run("cases.json", source)
        written = json.loads(converted)["case"]["specification"]
        self.assertEqual(written, values(CONVERTED))
        self.assertEqual(list(written)[:3], ["yamaa", "type", "domain"])
        self.assertEqual(
            converted,
            json.dumps({"case": {"specification": written, "csv": "ID\n"}}, indent=2)
            + "\n",
        )

    def test_a_layout_json_dumps_does_not_write_is_listed(self):
        spec = json.dumps(values(SPEC), indent=2)
        spec = re.sub(r'\{\s*"literal": 7\s*\}', '{"literal": 7}', spec)
        self.assertIn('{"literal": 7}', spec)
        source = json.dumps({"specification": spec}, indent=2) + "\n"
        converted, result = run("cases.json", source)
        self.assertEqual(converted, source)
        self.assertEqual(len(notes(result, "json")), 1)

    def test_a_tsv_row_converts_its_specification_and_its_expected_paths(self):
        spec = json.dumps(
            values(SPEC.replace("{literal: 7}", "{compute: {expr: '1 +'}}")),
            separators=(",", ":"),
        )
        expected = json.dumps(
            {"spec_paths": ["columns.VALUE.derivation.compute.expr"]},
            separators=(",", ":"),
        )
        source = f"case\tsource\texpected\nbad\t{spec}\t{expected}\n"
        converted, _ = run("cases.tsv", source)
        _, cell, report = converted.splitlines()[1].split("\t")
        self.assertEqual(json.loads(cell)["columns"][1]["derivation"], "1 +")
        self.assertTrue(cell.startswith('{"yamaa":"1.0","type":"spec",'))
        self.assertEqual(
            json.loads(report), {"spec_paths": ["columns.VALUE.derivation"]}
        )

    def test_block_rendering_keeps_every_json_type(self):
        value = {
            "a": "1.0",
            "b": 1.0,
            "c": [True, None, "null"],
            "d": {},
            "e": [[1, "x"]],
            "0o7": "0o7",
        }
        self.assertEqual(values(fixtures.block(value)), value)


SOURCES = ("rust", "python/tests", "R/yamaa/tests")
HEADER = re.compile(r"(?m)^schema_version\s*:|\"schema_version\"\s*:")


def corpus_files():
    for root in SOURCES:
        for path in embedded.files_under(ROOT / root):
            yield path


def zero_five_tests():
    """True while some test source still writes a 0.5 header."""
    return any(
        HEADER.search(path.read_text(encoding="utf-8")) for path in corpus_files()
    )


@unittest.skipUnless(
    zero_five_tests(), "the test sources are already in the 1.0 surface"
)
class CorpusTests(unittest.TestCase):
    """#3166's acceptance on a copy of the repository's test sources."""

    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.copy = Path(cls.temporary.name) / "copy"
        for path in corpus_files():
            target = cls.copy / path.relative_to(ROOT)
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
        cls.result = embedded.convert_embedded(
            [cls.copy / root for root in SOURCES], base=cls.copy
        )
        for label, text in cls.result.files.items():
            (cls.copy / label).write_text(text, encoding="utf-8")

    @classmethod
    def tearDownClass(cls):
        cls.temporary.cleanup()

    def test_every_converted_document_has_the_file_converter_s_values(self):
        table = table_module.load()
        compared = 0
        for label in sorted(self.result.files):
            original = (ROOT / label).read_text(encoding="utf-8")
            reader = embedded.READERS[Path(label).suffix.lower()]
            source = embedded.Source(label, original, table, embedded.Result(), reader)
            source.run()
            for group in source.extraction.groups:
                root = (
                    embedded.parse(source.readable(group)) if group.converted else None
                )
                if root is None or "schema_version" not in embedded.names_of(root):
                    continue
                expected = self.reference(source, group)
                if expected is None:
                    continue
                with self.subTest(file=label, line=source.extraction.line(group.start)):
                    self.assertEqual(self.written(group), expected)
                compared += 1
        # The corpus shrinks as P2-9 and P2-10 convert the tests themselves.
        if self.result.stats["documents converted"]:
            self.assertGreater(compared, 0)

    @staticmethod
    def written(group):
        if fixtures.is_json(group.converted):
            return json.loads(group.converted)
        return values(group.converted)

    @staticmethod
    def reference(source, group):
        """The file converter's values for a document, written as a file of its
        name; a fixture case's document beside the files the case names."""
        documents, inputs = {}, {}
        json_case = source.reader is fixtures.json_extraction
        if json_case and group.name:
            return None  # a case's named layer converts with the case's entry
        name = embedded.file_name(
            group.name or (f"{group.label}.yaml" if json_case and group.label else None)
        )
        documents[name] = source.readable(group)
        for other in source.extraction.groups if json_case else ():
            if other.name and other.scope[: len(group.scope)] == group.scope:
                if other.name.endswith(".csv"):
                    inputs[embedded.file_name(other.name)] = other.text
                elif (root := embedded.parse(other.text)) is not None:
                    written = embedded.file_name(other.name)
                    if embedded.classify(Path(written), root) != "environment":
                        documents[written] = other.text
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for written, content in {**inputs, **documents}.items():
                path = root / written
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content, encoding="utf-8")
            output = convert_files(
                [root / item for item in documents], base=root
            ).files.get(name)
        return values(output) if isinstance(output, str) else None

    def test_a_second_run_changes_nothing(self):
        again = embedded.convert_embedded(
            [self.copy / root for root in SOURCES], base=self.copy
        )
        self.assertEqual(sorted(again.files), [])

    def test_every_interpolated_string_whose_yaml_changes_is_listed(self):
        listed = {
            (note.file, note.line)
            for note in self.result.notes
            if note.kind == "interpolated"
        }
        missing = []
        for path in corpus_files():
            label = str(path.relative_to(ROOT))
            reader = embedded.READERS[path.suffix.lower()]
            # Notes name lines of the text written, which the copy holds.
            extraction = reader((self.copy / label).read_text(encoding="utf-8"))
            for group in extraction.groups:
                if not group.interpolated:
                    continue
                if not self.converter_changes(group.skeleton):
                    continue
                if (label, extraction.line(group.start)) not in listed:
                    missing.append(f"{label}:{extraction.line(group.start)}")
        self.assertEqual(missing, [])

    @staticmethod
    def converter_changes(skeleton):
        """Whether the file converter changes a template's skeleton written as a file."""
        if re.search(r"(?m)^\s*schema_version\s*:", skeleton):
            return True
        try:
            root = compose(skeleton)
        except yaml.YAMLError:
            return False
        if (
            not isinstance(root, (yaml.MappingNode, yaml.SequenceNode))
            or embedded.parse(skeleton) is None
        ):
            return False
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fragment.yaml"
            path.write_text(skeleton, encoding="utf-8")
            output = convert_files([path], base=directory).files.get("fragment.yaml")
        return isinstance(output, str) and output != skeleton

    def test_no_header_is_left_without_a_note(self):
        noted = {(note.file, note.line) for note in self.result.notes}
        missing = []
        for label, text in sorted(self.result.files.items()):
            extraction = embedded.READERS[Path(label).suffix.lower()](text)
            for group in extraction.groups:
                body = group.template if group.interpolated else group.text
                if (
                    HEADER.search(body)
                    and (label, extraction.line(group.start)) not in noted
                ):
                    missing.append(f"{label}:{extraction.line(group.start)}")
        self.assertEqual(missing, [])

    def test_nothing_failed_internally(self):
        kinds = {note.kind for note in self.result.notes}
        self.assertNotIn("internal", kinds)
        self.assertEqual(
            self.result.stats["source files left after a failed read-back"], 0
        )


if __name__ == "__main__":
    unittest.main()
