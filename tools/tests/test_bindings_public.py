"""The public section of the host interface and the host layers it generates."""

import ast
import copy
import unittest
from unittest.mock import patch

from tools.packaging import bindings_public as public
from tools.packaging import generate_bindings as generator


def member(definition, cls, name):
    members = definition["public"]["classes"][cls]["members"]
    return next(entry for entry in members if entry["name"] == name)


def function(definition, name):
    return next(f for f in definition["public"]["functions"] if f["name"] == name)


class RenderingTests(unittest.TestCase):
    def setUp(self):
        self.definition = generator._load_definition()
        self.public = public.Public(self.definition)

    def test_an_optional_argument_follows_its_hosts_type_projection(self):
        run = member(self.definition, "Spec", "run")
        python = ast.parse(self.public.render_python([]))
        spec = next(
            node
            for node in python.body
            if isinstance(node, ast.ClassDef) and node.name == "Spec"
        )
        method = next(
            node
            for node in spec.body
            if isinstance(node, ast.FunctionDef) and node.name == "run"
        )
        self.assertIn("inputs", [arg.arg for arg in method.args.kwonlyargs])
        self.public.public["types"]["input_frames"].pop("r")
        self.assertEqual(
            self.public._r_signature(run),
            "stream = FALSE, progress = NULL, threads = NULL",
        )
        self.public.public["types"]["input_frames"]["r"] = "named list or NULL"
        self.assertEqual(
            self.public._r_signature(run),
            "stream = FALSE, progress = NULL, inputs = NULL, threads = NULL",
        )

    def test_python_exports_the_declared_names(self):
        names = [
            "Error",
            "StateError",
            "Table",
            "__version__",
            "conditions",
            "info",
            "lint",
            "project",
            "read_odm",
            "schema",
            "selftest",
            "spec",
        ]
        self.assertEqual(self.public.python_names(), names)
        module = ast.parse(self.public.render_python([]))
        exported = next(
            node.value
            for node in module.body
            if isinstance(node, ast.Assign) and node.targets[0].id == "__all__"
        )
        self.assertEqual(ast.literal_eval(exported), names)
        classes = [node.name for node in module.body if isinstance(node, ast.ClassDef)]
        self.assertEqual(
            classes,
            ["Check", "Run", "MemberRun", "ProjectRun", "Submission"]
            + ["Spec", "Project", "Lint", "SelfTest"],
        )

    def test_python_delegates_each_member_to_its_operation(self):
        text = self.public.render_python([])
        for expected in (
            '            _native._spec_run(\n                self._path,\n                self._project,\n                self._plans,\n                _flag(stream, "stream"),\n                progress,\n                inputs,\n                threads,\n            )\n',
            'return Spec(_native._project_spec(self._path, _text(name, "name")), self._path)',
            "return self._native.output(self._name)",
            "        return self._native.save()\n",
            '            self._native.member_issues(\n                _text(name, "name")\n            )  # raises',
            "        return MemberRun(self._native, name)",
            "        return Check(_native._project_check(self._path, progress, threads))",
            "class Spec(_Lazy):",
            "    def __init__(self, path, project=None):",
            "    def __init__(self, native, name: str):",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, text)
        # The private qualification accessor is reached through the handle.
        self.assertNotIn("observations", text)

    def test_r_exports_and_s3_methods_follow_the_declaration(self):
        namespace = self.public.render_namespace([]).splitlines()
        self.assertEqual(
            namespace,
            [
                "useDynLib(yamaa, .registration = TRUE)",
                "export(yamaa_spec)",
                "export(yamaa_project)",
                "export(yamaa_read_odm)",
                "export(yamaa_lint)",
                "export(yamaa_schema)",
                "export(yamaa_conditions)",
                "export(yamaa_info)",
                "export(yamaa_selftest)",
                "S3method(names, yamaa_check)",
                "S3method(length, yamaa_check)",
                "S3method(print, yamaa_check)",
                "S3method(format, yamaa_check)",
                "S3method(names, yamaa_run)",
                "S3method(length, yamaa_run)",
                "S3method(print, yamaa_run)",
                "S3method(format, yamaa_run)",
                "S3method(names, yamaa_member_run)",
                "S3method(length, yamaa_member_run)",
                "S3method(print, yamaa_member_run)",
                "S3method(format, yamaa_member_run)",
                "S3method(names, yamaa_project_run)",
                "S3method(length, yamaa_project_run)",
                'S3method("[[", yamaa_project_run)',
                "S3method(print, yamaa_project_run)",
                "S3method(format, yamaa_project_run)",
                "S3method(names, yamaa_submission)",
                "S3method(length, yamaa_submission)",
                "S3method(print, yamaa_submission)",
                "S3method(format, yamaa_submission)",
                "S3method(print, yamaa_spec)",
                "S3method(format, yamaa_spec)",
                "S3method(print, yamaa_project)",
                "S3method(format, yamaa_project)",
                "S3method(names, yamaa_lint_result)",
                "S3method(length, yamaa_lint_result)",
                "S3method(print, yamaa_lint_result)",
                "S3method(format, yamaa_lint_result)",
                "S3method(names, yamaa_selftest)",
                "S3method(length, yamaa_selftest)",
                "S3method(print, yamaa_selftest)",
                "S3method(format, yamaa_selftest)",
            ],
        )
        text = self.public.render_r([])
        for expected in (
            'yamaa_spec <- function(path) .spec(.path(path, "path"), NULL)',
            "`[[.yamaa_project_run` <- function(x, i, ...) {",
            "    plans <<- .reply(.native_plans(plans))",
            "  object$save <- function() "
            "invisible(.changing(memo, .reply(.native_run_save(native))))",
            ".run_observations <- function(run, include_measurements = FALSE) {",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, text)
        # Every result prints, so every result keeps its handle for format().
        self.assertEqual(text.count("object$.handle <- native"), 7)

    def test_a_printing_member_returns_the_text_rust_renders(self):
        python = self.public.render_python([])
        self.assertIn(
            "    def __repr__(self) -> str:\n        return self._native.summary()\n",
            python,
        )
        r = self.public.render_r([])
        self.assertIn(
            "print.yamaa_selftest <- function(x, ...) {\n"
            '  cat(format(x), sep = "\\n")\n'
            "  invisible(x)\n"
            "}\n",
            r,
        )
        self.assertIn(
            "format.yamaa_selftest <- function(x, ...) {\n"
            '  native <- get(".handle", envir = x, inherits = FALSE)\n'
            "  .reply(.native_selftest_summary(native))\n"
            "}\n",
            r,
        )

    def test_a_lazy_object_prints_the_path_it_holds(self):
        python = self.public.render_python([])
        for name in ("spec", "project"):
            with self.subTest(name=name):
                self.assertIn(
                    "    def __repr__(self) -> str:\n"
                    f"        return _native._{name}_summary(self._path)\n",
                    python,
                )
        r = self.public.render_r([])
        # The object keeps the path an S3 method reads in a hidden binding.
        self.assertEqual(r.count("  object$.path <- path\n"), 2)
        self.assertIn(
            "format.yamaa_spec <- function(x, ...) {\n"
            '  path <- get(".path", envir = x, inherits = FALSE)\n'
            "  .reply(.native_spec_summary(path))\n"
            "}\n",
            r,
        )

    def test_an_s3_method_reads_every_held_value_its_call_takes(self):
        r = self.public.render_r([])
        self.assertIn("  object$.handle <- native\n  object$.name <- name\n", r)
        self.assertIn(
            "format.yamaa_member_run <- function(x, ...) {\n"
            '  native <- get(".handle", envir = x, inherits = FALSE)\n'
            '  name <- get(".name", envir = x, inherits = FALSE)\n'
            "  .reply(.native_project_run_member_summary(native, name))\n"
            "}\n",
            r,
        )
        self.assertIn(
            "    def __repr__(self) -> str:\n"
            "        return self._native.member_summary(self._name)\n",
            self.public.render_python([]),
        )

    def test_every_result_names_its_public_members_in_r(self):
        r = self.public.render_r([])
        # Its properties and methods in declaration order, never the handle.
        self.assertIn(
            'names.yamaa_run <- function(x) c("status", "output", "issues", '
            '"report", "timings", "save")\n\nlength.yamaa_run <- function(x) 6L\n',
            r,
        )
        self.assertIn(
            'names.yamaa_submission <- function(x) c("status", "created", '
            '"issues", "timings", "save")\n',
            r,
        )
        # A container names its keys, as its names member gives them.
        self.assertIn(
            "names.yamaa_project_run <- function(x) {\n"
            '  native <- get(".handle", envir = x, inherits = FALSE)\n'
            "  .reply(.native_project_run_names(native))\n"
            "}\n\n"
            "length.yamaa_project_run <- function(x) length(names(x))\n",
            r,
        )
        # A lazy object is no result and keeps R's own names() and length().
        self.assertNotIn("names.yamaa_spec", r)
        self.assertNotIn("length.yamaa_project <-", r)
        self.assertIn(
            "    def __len__(self) -> int:\n"
            '        """The number of names."""\n'
            "        return len(self._native.names())\n",
            self.public.render_python([]),
        )

    def test_selftest_delegates_to_its_native_result(self):
        python = self.public.render_python([])
        for expected in (
            "def selftest() -> SelfTest:",
            "    return SelfTest(_native._selftest())",
            "    def status(self) -> str:\n        "
            '"""`failed` when a case failed, and `succeeded` otherwise."""\n'
            "        return self._native.status()",
            "        return self._native.cases()",
            "    def info(self) -> dict:",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, python)
        r = self.public.render_r([])
        for expected in (
            "yamaa_selftest <- function() .self_test(.reply(.native_selftest(.version())))",
            '  .read_only(object, "status", '
            "function() .reply(.native_selftest_status(native)))",
            '  .kept(object, "cases", memo, function() .reply(.native_selftest_cases(native)))',
            '  .read_only(object, "info", function() .reply(.native_selftest_info(native)))',
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, r)

    def test_a_result_keeps_its_tables_until_a_method_runs_in_r(self):
        r = self.public.render_r([])
        # #4071: a table property of a result reads through its memo, which
        # every method starts again; any other property reads each time, and
        # a lazy object, which reads no table, has no memo.
        self.assertIn(
            ".run <- function(native, memo = .memo()) {\n"
            "  force(native)\n"
            "  force(memo)\n"
            "  object <- new.env(parent = emptyenv())\n"
            "  object$.handle <- native\n"
            '  .read_only(object, "status", function() .reply(.native_run_status(native)))\n'
            '  .kept(object, "output", memo, function() .reply(.native_run_output(native)))\n'
            '  .kept(object, "issues", memo, function() .reply(.native_run_issues(native)))\n'
            '  .kept(object, "report", memo, function() .reply(.native_run_report(native)))\n'
            '  .kept(object, "timings", memo, function() .reply(.native_run_timings(native)))\n'
            "  object$save <- function() "
            "invisible(.changing(memo, .reply(.native_run_save(native))))\n"
            '  .object(object, "yamaa_run")\n'
            "}\n",
            r,
        )
        self.assertIn(
            '  .read_only(object, "created", '
            "function() .reply(.native_submission_created(native)))\n",
            r,
        )
        self.assertIn(".spec <- function(path, project) {\n", r)
        self.assertIn(
            '  .read_only(object, "names", '
            "function() .reply(.native_project_names(path, .version())))\n",
            r,
        )
        # A member that the index makes shares its container's memo, so the
        # container's save() starts the member's tables again.
        self.assertIn("  object$.handle <- native\n  object$.memo <- memo\n", r)
        self.assertEqual(r.count("object$.memo <- memo"), 1)
        self.assertIn(
            '  .member_run(native, name, get(".memo", envir = x, inherits = FALSE))\n}\n',
            r,
        )

    def test_a_path_list_converts_each_path_in_both_hosts(self):
        python = self.public.render_python([])
        self.assertIn("def lint(paths, fix: bool = False) -> Lint:", python)
        self.assertIn(
            'return Lint(_native._lint(_fspaths(paths), _flag(fix, "fix")))', python
        )
        self.assertIn(
            'yamaa_lint <- function(paths, fix = FALSE) {\n  .lint(.reply(.native_lint(.paths(paths, "paths"), .flag(fix, "fix"))))\n}',
            self.public.render_r([]),
        )

    def test_an_r_path_argument_expands_and_a_text_argument_is_taken_as_written(self):
        # #4091: each argument declared `path`, `optional_path` or `path_list`
        # reaches api.R's path conversion, which expands a leading `~`; a
        # `text` or `optional_text` argument, such as a name or a TAR member,
        # and the path a native call returns keep the text conversion.
        r = self.public.render_r([])
        for expected in (
            'yamaa_spec <- function(path) .spec(.path(path, "path"), NULL)',
            'yamaa_project <- function(path) .project(.path(path, "path"))',
            '    .path(source, "source"),\n'
            '    .optional_text(archive_member, "archive_member"),\n',
            '.reply(.native_lint(.paths(paths, "paths"), .flag(fix, "fix")))',
            '      .optional_path(parent, "parent"),\n'
            "      .version(),\n"
            '      .progress(progress, "progress"),\n',
            '    .spec(.text(.reply(.native_project_spec(path, .text(name, "name"), '
            '.version())), "path"), path)\n',
            '      .optional_text(section, "section"),\n'
            '      .optional_text(created, "created"),\n',
            'yamaa_schema <- function(file_type) .reply(.native_schema(.text(file_type, "file_type")))',
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, r)
        for removed in (
            ".texts(",
            ".text(path, ",
            ".text(source, ",
            ".optional_text(parent, ",
        ):
            with self.subTest(removed=removed):
                self.assertNotIn(removed, r)

    def test_a_file_type_argument_gives_the_text_its_native_call_returns(self):
        python = self.public.render_python([])
        self.assertIn(
            "def schema(file_type: str) -> str:\n"
            '    """The JSON Schema of a file type, spec, project, codelists or '
            'function, as canonical JSON text."""\n'
            '    return _native._schema(_text(file_type, "file_type"))\n',
            python,
        )
        self.assertIn(
            'yamaa_schema <- function(file_type) .reply(.native_schema(.text(file_type, "file_type")))',
            self.public.render_r([]),
        )

    def test_a_long_export_list_is_split_one_name_per_line(self):
        text = self.public.render_python([])
        self.assertIn(
            '__all__ = [\n    "Error",\n    "StateError",\n    "Table",\n    "__version__",\n',
            text,
        )
        self.assertIn('    "spec",\n]\n', text)
        candidate = copy.deepcopy(self.definition)
        for name in ("lint", "schema", "conditions", "info", "selftest"):
            candidate["public"]["functions"].remove(function(candidate, name))
        short = public.Public(candidate).render_python([])
        self.assertIn(
            '__all__ = ["Error", "StateError", "Table", "__version__", "project", '
            '"read_odm", "spec"]\n',
            short,
        )

    def test_the_helper_import_names_the_helpers_used_and_wraps_when_long(self):
        def imported(text):
            [node] = [
                node
                for node in ast.parse(text).body
                if isinstance(node, ast.ImportFrom) and node.module == "yamaa._support"
            ]
            return [item.name for item in node.names]

        expected = [
            "__version__",
            "_flag",
            "_fspath",
            "_fspaths",
            "_Lazy",
            "_optional_fspath",
            "_optional_text",
            "_text",
        ]
        self.assertEqual(imported(self.public.render_python([])), expected)
        candidate = copy.deepcopy(self.definition)
        candidate["public"]["functions"].remove(function(candidate, "lint"))
        self.assertEqual(
            imported(public.Public(candidate).render_python([])),
            [name for name in expected if name != "_fspaths"],
        )
        with patch.object(public, "_PY_WIDTH", 60):
            narrow = public.Public(self.definition).render_python([])
        self.assertIn(
            "from yamaa._support import (\n    __version__,\n    _flag,\n    _fspath,\n",
            narrow,
        )

    def test_a_removed_function_leaves_both_hosts_and_the_namespace(self):
        candidate = copy.deepcopy(self.definition)
        functions = candidate["public"]["functions"]
        functions.remove(function(candidate, "read_odm"))
        rendered = public.Public(candidate)
        self.assertNotIn("def read_odm(", rendered.render_python([]))
        self.assertNotIn("yamaa_read_odm", rendered.render_r([]))
        self.assertNotIn("export(yamaa_read_odm)", rendered.render_namespace([]))
        self.assertNotIn("read_odm", rendered.python_names())

    def test_declared_refusals_use_the_same_native_call_without_remapping(self):
        candidate = copy.deepcopy(self.definition)
        member(candidate, "Spec", "check").pop("raises")
        text = public.Public(candidate).render_python([])
        self.assertIn(
            "return Check(\n"
            "            _native._spec_check(\n"
            "                self._path, self._project, _optional_fspath(parent), progress, threads\n"
            "            )\n"
            "        )",
            text,
        )

    def test_a_python_only_class_is_the_native_class_exported_as_it_is(self):
        python = self.public.render_python([])
        self.assertIn(
            "from yamaa import _native\nfrom yamaa._native import Error, StateError, Table\n",
            python,
        )
        self.assertNotIn("class Table", python)
        self.assertNotIn("Table", self.public.render_r([]))
        self.assertNotIn("Table", self.public.render_namespace([]))
        self.assertEqual(self.public.kind("Table"), "native")
        self.assertEqual(
            list(self.public.delegating_classes()),
            [name for name in self.definition["public"]["classes"] if name != "Table"],
        )
        # Every table is that class: annotations name it, and nothing wraps
        # the table that a native call returns.
        self.assertIn("    def issues(self) -> Table:\n", python)
        self.assertIn("    def output(self) -> Table | None:\n", python)
        self.assertIn(
            "def _error_init(self, message: str, issues: Table, paths: list[str]):",
            python,
        )
        self.assertIn('setattr(Error, "__reduce__", _error_reduce)', python)

    def test_generated_python_is_valid_and_ascii(self):
        text = self.public.render_python(generator._PYI_HEADER)
        compile(text, "__init__.py", "exec")
        self.assertTrue(text.isascii())
        self.assertTrue(self.public.render_r(generator._PYI_HEADER).isascii())


class ValidationTests(unittest.TestCase):
    def setUp(self):
        self.definition = generator._load_definition()

    def refuses(self, message, mutate):
        candidate = copy.deepcopy(self.definition)
        mutate(candidate)
        with self.assertRaisesRegex(ValueError, message):
            public.Public(candidate)

    def test_malformed_declarations_are_refused(self):
        classes = lambda d: d["public"]["classes"]  # noqa: E731
        cases = [
            ("unknown keys", lambda d: d["public"].update(metadata={})),
            (
                "unknown token",
                lambda d: d["public"]["types"].update(widget={"python": "x", "r": "y"}),
            ),
            ("undeclared type token", lambda d: d["public"]["types"].pop("count")),
            (
                "public.types.file_type names an undeclared vocabulary",
                lambda d: d["values"].pop("file_type"),
            ),
            (
                "public.types.file_type.r must name values.file_type",
                lambda d: d["public"]["types"]["file_type"].update(r="character(1)"),
            ),
            ("unknown kind", lambda d: member(d, "Run", "save").update(kind="action")),
            (
                "unknown operation",
                lambda d: member(d, "Run", "save").update(calls="run_publish"),
            ),
            (
                "takes no arguments",
                lambda d: member(d, "Run", "issues").update(args={"name": "text"}),
            ),
            (
                "must be spelled",
                lambda d: member(d, "ProjectRun", "member").update(r="["),
            ),
            (
                "only index and printing do",
                lambda d: member(d, "Run", "save").update(python="publish"),
            ),
            (
                "names a record exactly when it returns one",
                lambda d: member(d, "SelfTest", "info").pop("record"),
            ),
            (
                "names a record exactly when it returns one",
                lambda d: member(d, "SelfTest", "status").update(record="info"),
            ),
            (
                "undeclared record",
                lambda d: member(d, "SelfTest", "info").update(record="engine"),
            ),
            (
                "must be spelled __repr__ and \\[print, format\\]",
                lambda d: member(d, "SelfTest", "summary").update(r=["print"]),
            ),
            (
                "must be spelled __repr__ and \\[print, format\\]",
                lambda d: member(d, "SelfTest", "summary").update(python="__str__"),
            ),
            (
                "must be spelled __repr__ and \\[print, format\\]",
                lambda d: member(d, "SelfTest", "summary").update(returns="text"),
            ),
            (
                "must be spelled __repr__ and \\[print, format\\]",
                lambda d: member(d, "SelfTest", "summary").pop("calls"),
            ),
            (
                "unknown operation",
                lambda d: member(d, "SelfTest", "summary").update(
                    calls="selftest_report"
                ),
            ),
            (
                "Spec.summary calls _SelfTest.summary without holding a handle",
                lambda d: member(d, "Spec", "summary").update(calls="selftest_summary"),
            ),
            (
                "nothing fills the Python argument specification",
                lambda d: classes(d)["Spec"].update(holds={"path": "project"}),
            ),
            (
                "keyword_only",
                lambda d: member(d, "Project", "submission").update(
                    keyword_only=["section"]
                ),
            ),
            (
                "cannot be written as a docstring",
                lambda d: member(d, "Run", "save").update(doc='Say """x"""'),
            ),
            (
                "unsupported default",
                lambda d: member(d, "Project", "spec").update(
                    args={"name": "text = None"}
                ),
            ),
            (
                "exactly the refusal",
                lambda d: d["public"]["errors"].update(argument={}),
            ),
            (
                "unknown keys",
                lambda d: classes(d)["Run"].update(interrupt="run_output"),
            ),
            (
                "must hold its handle first",
                lambda d: classes(d)["MemberRun"].update(
                    holds={"name": "name", "native": "handle"}
                ),
            ),
            # A held value is a handle or a path; `field` is no member kind.
            (
                "unknown kind",
                lambda d: member(d, "Check", "issues").update(kind="field"),
            ),
            (
                "defined later",
                lambda d: classes(d).update(Check=classes(d).pop("Check")),
            ),
            (
                "must be yamaa.read_odm",
                lambda d: function(d, "read_odm").update(python="yamaa.odm"),
            ),
            (
                "must return a class",
                lambda d: function(d, "spec").update(returns="tree"),
            ),
            (
                "needs a names member",
                lambda d: classes(d)["ProjectRun"]["members"].remove(
                    member(d, "ProjectRun", "names")
                ),
            ),
            (
                "reaches a handle",
                lambda d: classes(d)["Spec"]["members"].append(
                    {
                        "name": "observations",
                        "kind": "private",
                        "returns": "optional_text",
                        "calls": "spec_check",
                    }
                ),
            ),
        ]
        for message, mutate in cases:
            with self.subTest(message=message):
                self.refuses(message, mutate)

    def test_a_python_only_class_declares_each_native_method_once(self):
        table = lambda d: d["public"]["classes"]["Table"]  # noqa: E731
        cases = [
            # A class for any other host is an ordinary class, which holds values.
            ("is missing \\['holds', 'r'\\]", lambda d: table(d).update(hosts=["r"])),
            ("unknown keys", lambda d: table(d).update(r="yamaa_table")),
            ("must be yamaa.Table", lambda d: table(d).update(python="yamaa.Frame")),
            (
                "is no class of yamaa._native",
                lambda d: d["public"]["classes"].update(
                    Frame={**copy.deepcopy(table(d)), "python": "yamaa.Frame"}
                ),
            ),
            (
                "must declare every native method: to_pandas",
                lambda d: table(d)["members"].pop(),
            ),
            (
                "is no method of yamaa._native.Table",
                lambda d: member(d, "Table", "to_arrow").update(name="to_numpy"),
            ),
            (
                "declares the native __len__ twice",
                lambda d: table(d)["members"].append(
                    {**member(d, "Table", "length"), "name": "size"}
                ),
            ),
            (
                "a property exactly when its native method is a getter",
                lambda d: member(d, "Table", "num_rows").update(kind="method"),
            ),
            (
                "a property exactly when its native method is a getter",
                lambda d: member(d, "Table", "to_pylist").update(kind="property"),
            ),
            (
                "must spell its special method",
                lambda d: member(d, "Table", "length").pop("python"),
            ),
            (
                "only protocol and printing do",
                lambda d: member(d, "Table", "to_arrow").update(python="to_arrow"),
            ),
            (
                "must be __repr__",
                lambda d: member(d, "Table", "preview").update(python="__str__"),
            ),
            (
                "must declare what it returns",
                lambda d: member(d, "Table", "to_pylist").pop("returns"),
            ),
            (
                "must take its native method's arguments",
                lambda d: member(d, "Table", "arrow_stream").pop("args"),
            ),
            (
                "imports must list the modules",
                lambda d: member(d, "Table", "num_rows").update(imports=["polars"]),
            ),
            (
                "imports must list the modules",
                lambda d: member(d, "Table", "to_pandas").update(
                    imports=["pandas", "pandas"]
                ),
            ),
            (
                "needs host_only entries: Table.to_pylist",
                lambda d: d["host_only"]["python"].remove("Table.to_pylist"),
            ),
            # A Python-only token or class stays out of both hosts' surface.
            (
                "uses the Python-only type token 'rows'",
                lambda d: member(d, "Run", "issues").update(returns="rows"),
            ),
            (
                "returns the Python-only class Table",
                lambda d: function(d, "read_odm").update(returns="Table"),
            ),
            (
                "invalid R refusal field",
                lambda d: d["public"]["errors"]["refusal"]["r"]["fields"].update(
                    issues="rows"
                ),
            ),
            # Every column type projects into both hosts.
            (
                "must project str, int, float, date, datetime, list",
                lambda d: d["public"]["value_types"].pop("list"),
            ),
            (
                "public.value_types.date must be a mapping|is missing",
                lambda d: d["public"]["value_types"].update(date={"python": "date"}),
            ),
            (
                "tables.lint.line has a type",
                lambda d: d["tables"]["lint"]["columns"][1].update(type="decimal"),
            ),
            # The table token annotates the class every table is.
            (
                "public.classes.Table, the class every table is, is missing",
                lambda d: d["public"]["classes"].pop("Table"),
            ),
        ]
        for message, mutate in cases:
            with self.subTest(message=message):
                self.refuses(message, mutate)

    def test_every_native_argument_and_member_argument_must_bind(self):
        cases = [
            # A member argument that no native argument takes.
            (
                "fill no Python argument",
                lambda d: member(d, "Run", "save").update(args={"path": "path"}),
            ),
            # A lazy object that stops holding a path its natives take.
            (
                "nothing fills the Python argument project",
                lambda d: d["public"]["classes"]["Spec"].update(
                    holds={"path": "specification"}
                ),
            ),
            # A handle class calling a module function, and the reverse.
            (
                "holds a handle but calls _spec_check",
                lambda d: member(d, "Run", "issues").update(calls="spec_check"),
            ),
            (
                "without holding a handle",
                lambda d: member(d, "Spec", "resolve").update(calls="run_report"),
            ),
            # A result made from the wrong native type.
            (
                "returns _Check, not Run",
                lambda d: member(d, "Spec", "run").update(
                    calls="spec_check",
                    args={
                        "parent": "optional_path = None",
                        "progress": "progress = None",
                        "threads": "threads = None",
                    },
                    keyword_only=[],
                ),
            ),
            # Members of one handle class call one native class.
            (
                "one native class",
                lambda d: member(d, "MemberRun", "output").update(calls="run_output"),
            ),
        ]
        for message, mutate in cases:
            with self.subTest(message=message):
                self.refuses(message, mutate)


class ManifestTests(unittest.TestCase):
    def test_public_lines_leave_out_private_and_native_details(self):
        lines = public.Public(generator._load_definition()).manifest_lines()
        for expected in (
            "public.types.table: python yamaa.Table; r data.frame",
            "public.functions.read_odm: python yamaa.read_odm; r yamaa_read_odm; "
            "returns table; table odm_items; raises refusal",
            "public.functions.read_odm.args[2]: max_archive_members: count = 10000; "
            "keyword-only",
            "public.classes.ProjectRun.member: index; python __getitem__; r [[; "
            "returns MemberRun; raises refusal",
            "public.classes.Project.submission.args[1]: created: optional_text = None; "
            "keyword-only",
            "public.python.exports.__version__",
            "public.r.exports.yamaa_read_odm",
            "public.r.s3methods.yamaa_project_run.[[",
            "public.classes.SelfTest.info: property; returns info; record info",
            "public.classes.SelfTest.summary: printing; python __repr__; r print, format; raises refusal",
            "public.r.s3methods.yamaa_selftest.print",
            "public.r.s3methods.yamaa_selftest.format",
            "public.classes.Check.status: property; returns status",
            "public.classes.MemberRun.status: property; returns status; raises refusal",
            "public.classes.Spec.summary: printing; python __repr__; r print, format; raises refusal",
            "public.r.s3methods.yamaa_spec.format",
            "public.classes.ProjectRun.length: protocol; python __len__",
            "public.r.s3methods.yamaa_check.names: status, issues, timings",
            "public.r.s3methods.yamaa_check.length",
            "public.r.s3methods.yamaa_project_run.names: keys of names",
            "public.types.polars_frame: python polars.DataFrame",
            "public.value_types.date: python Arrow Date32, datetime.date; r Date",
            "public.classes.Table: python yamaa.Table",
            "public.classes.Table.num_rows: property; returns count; raises refusal",
            "public.classes.Table.length: protocol; python __len__; returns count; raises refusal",
            "public.classes.Table.preview: printing; python __repr__; raises refusal",
            "public.classes.Table.arrow_stream.args[0]: "
            "requested_schema: schema_request = None",
            "public.classes.Table.to_pylist: method; returns rows; raises refusal",
            "public.classes.Table.to_pandas: method; returns pandas_frame; raises refusal; "
            "imports pandas, pyarrow",
            "public.python.exports.Table",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, lines)
        text = "\n".join(lines)
        for private in ("observations", "_native", "holds", "calls", "plans", "doc"):
            with self.subTest(private=private):
                self.assertNotIn(private, text)
        self.assertEqual(len(lines), len(set(lines)))


class FormattingTests(unittest.TestCase):
    def test_python_calls_split_at_brackets_as_ruff_does(self):
        short = public._Call("f", ["a", "b"])
        self.assertEqual(
            public._py_lines(short, "    ", "return "), ["    return f(a, b)"]
        )
        inner = public._Call("_call", [f"argument_{n}" for n in range(9)])
        lines = public._py_lines(public._Call("_table", [inner]), "    ", "return ")
        self.assertEqual(lines[:2], ["    return _table(", "        _call("])
        self.assertEqual(lines[2], "            argument_0,")
        self.assertEqual(lines[-2:], ["        )", "    )"])
        self.assertTrue(all(len(line) <= 88 for line in lines))

    def test_python_definitions_wrap_their_parameters(self):
        params = "self, section: str | None = None, *, created: str | None = None"
        self.assertEqual(
            public._py_def("submission", params, "Submission", "    "),
            [
                "    def submission(",
                f"        {params}",
                "    ) -> Submission:",
            ],
        )

    def test_r_calls_hug_single_arguments_and_wrap_the_rest(self):
        call = public._Call(
            ".run",
            [public._Call(".reply", [public._Call(".native_x", ["a" * 40, "b" * 40])])],
        )
        self.assertEqual(
            public._r_lines(call, "  "),
            ["  .run(.reply(.native_x(", f"    {'a' * 40}, {'b' * 40}", "  )))"],
        )
        header = public._r_header(
            "f <- ", ", ".join(f"argument_{n} = NULL" for n in range(6))
        )
        self.assertTrue(all(len(line) <= 100 for line in header))
        self.assertTrue(header[-1].endswith(") {"))
        self.assertTrue(header[1].startswith(" " * len("f <- function(")))


if __name__ == "__main__":
    unittest.main()
