"""The public section of the host interface and the host layers it generates."""

import ast
import copy
import unittest

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

    def test_python_exports_the_declared_names(self):
        names = [
            "Error",
            "__version__",
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
            ["Error", "Check", "Run", "MemberRun", "ProjectRun", "Submission"]
            + ["Spec", "Project", "SelfTest"],
        )

    def test_python_delegates_each_member_to_its_operation(self):
        text = self.public.render_python([])
        for expected in (
            "return Run(_call(_native._spec_run, self._path, self._project, self._plans))",
            "return Spec(_call(_native._project_spec, self._path, name), self._path)",
            "return _optional_table(_call(self._native.output, self._name))",
            "        return _call(self._native.save)\n",
            "            _call(self._native.member_issues, name)  # raises",
            "        return MemberRun(self._native, name)",
            "        return Check(_call(_native._project_check, self._path))",
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
                "export(yamaa_info)",
                "export(yamaa_selftest)",
                "S3method(print, yamaa_check)",
                "S3method(format, yamaa_check)",
                "S3method(print, yamaa_run)",
                "S3method(format, yamaa_run)",
                "S3method(print, yamaa_member_run)",
                "S3method(format, yamaa_member_run)",
                'S3method("[[", yamaa_project_run)',
                "S3method(print, yamaa_project_run)",
                "S3method(format, yamaa_project_run)",
                "S3method(print, yamaa_submission)",
                "S3method(format, yamaa_submission)",
                "S3method(print, yamaa_spec)",
                "S3method(format, yamaa_spec)",
                "S3method(print, yamaa_project)",
                "S3method(format, yamaa_project)",
                "S3method(print, yamaa_selftest)",
                "S3method(format, yamaa_selftest)",
            ],
        )
        text = self.public.render_r([])
        for expected in (
            'yamaa_spec <- function(path) .spec(.text(path, "path"), NULL)',
            "`[[.yamaa_project_run` <- function(x, i, ...) {",
            "  .rethrow_interrupt(.native_run_interrupt(native))",
            "    plans <<- .reply(.native_plans(plans))",
            "  object$save <- function() invisible(.reply(.native_run_save(native)))",
            ".run_observations <- function(run, include_measurements = FALSE) {",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, text)
        # Every result prints, so every result keeps its handle for format().
        self.assertEqual(text.count("object$.handle <- native"), 6)

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

    def test_selftest_delegates_to_its_native_result(self):
        python = self.public.render_python([])
        for expected in (
            "def selftest() -> SelfTest:",
            "    return SelfTest(_native._selftest())",
            "    def status(self) -> str:\n        "
            '"""`failed` when a case failed, and `succeeded` otherwise."""\n'
            "        return self._native.status()",
            "        return _table(self._native.cases())",
            "    def info(self) -> dict:",
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, python)
        r = self.public.render_r([])
        for expected in (
            "yamaa_selftest <- function() .self_test(.reply(.native_selftest(.version())))",
            '  .read_only(object, "status", '
            "function() .reply(.native_selftest_status(native)))",
            '  .read_only(object, "info", function() .reply(.native_selftest_info(native)))',
        ):
            with self.subTest(expected=expected):
                self.assertIn(expected, r)

    def test_a_path_list_converts_each_path_in_both_hosts(self):
        python = self.public.render_python([])
        self.assertIn("def lint(paths, fix: bool = False) -> _pl.DataFrame:", python)
        self.assertIn(
            "return _table(_call(_native._lint, _fspaths(paths), fix))", python
        )
        self.assertIn(
            'yamaa_lint <- function(paths, fix = FALSE) .reply(.native_lint(.texts(paths, "paths"), fix))',
            self.public.render_r([]),
        )

    def test_a_file_type_argument_gives_the_text_its_native_call_returns(self):
        python = self.public.render_python([])
        self.assertIn(
            "def schema(file_type: str) -> str:\n"
            '    """The JSON Schema of a file type, spec, project, codelists or '
            'function, as canonical JSON text."""\n'
            "    return _call(_native._schema, file_type)\n",
            python,
        )
        self.assertIn(
            'yamaa_schema <- function(file_type) .reply(.native_schema(.text(file_type, "file_type")))',
            self.public.render_r([]),
        )

    def test_a_long_export_list_is_split_one_name_per_line(self):
        text = self.public.render_python([])
        self.assertIn('__all__ = [\n    "Error",\n    "__version__",\n', text)
        self.assertIn('    "spec",\n]\n', text)
        candidate = copy.deepcopy(self.definition)
        for name in ("lint", "schema", "info"):
            candidate["public"]["functions"].remove(function(candidate, name))
        short = public.Public(candidate).render_python([])
        self.assertIn(
            '__all__ = ["Error", "__version__", "project", "read_odm", "selftest", '
            '"spec"]\n',
            short,
        )

    def test_a_long_helper_import_is_split_one_name_per_line(self):
        text = self.public.render_python([])
        self.assertIn("from yamaa._support import (\n    __version__,\n", text)
        candidate = copy.deepcopy(self.definition)
        candidate["public"]["functions"].remove(function(candidate, "lint"))
        short = public.Public(candidate).render_python([])
        self.assertIn(
            "from yamaa._support import __version__, _fspath, _Lazy, _optional_table, "
            "_pl, _table\n",
            short,
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

    def test_raises_decides_whether_a_call_maps_the_native_refusal(self):
        candidate = copy.deepcopy(self.definition)
        member(candidate, "Spec", "check").pop("raises")
        text = public.Public(candidate).render_python([])
        self.assertIn(
            "return Check(_native._spec_check(self._path, self._project))", text
        )

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
            ("host-only", lambda d: classes(d)["Run"].update(interrupt="run_output")),
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
                lambda d: member(d, "Spec", "run").update(calls="spec_check"),
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
            "public.types.table: python polars.DataFrame; r data.frame",
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
            "public.classes.SelfTest.summary: printing; python __repr__; r print, format",
            "public.r.s3methods.yamaa_selftest.print",
            "public.r.s3methods.yamaa_selftest.format",
            "public.classes.Check.status: property; returns status",
            "public.classes.MemberRun.status: property; returns status; raises refusal",
            "public.classes.Spec.summary: printing; python __repr__; r print, format",
            "public.r.s3methods.yamaa_spec.format",
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
