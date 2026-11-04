"""Installed `python -m yamaa._worker`: the worker the standalone `yamaa`
command line runs its Python project functions through (`cli.worker` of
rust/interface/bindings.yaml).

The binary's own tests cover its side of the protocol, the codec of
rust/crates/yamaa-cli/src/worker.rs; this drives the worker as the binary
does, over its standard input and output, and checks what it answers: the
installed engine's version, the installation's observations for the uv lock
verification, a bound callable's results row by row and as a batch, and
the failures it reports in the engine's own terms. The worker resolves and
calls with the helpers the package's `run()` uses, so a function behaves as
it does there.
"""

import datetime
import platform
import subprocess
import sys
import unittest
from urllib.parse import quote, unquote

import yamaa
from yamaa import _locked_functions as locked

SAFE = "-._~"


def enc(text):
    return quote(text, safe=SAFE)


def zero_division():
    """The message this interpreter gives a float division by zero, which
    Python 3.14 shortened."""
    try:
        1.0 / 0.0
    except ZeroDivisionError as error:
        return str(error)
    raise AssertionError("division by zero raises")


def worker(lines):
    """Send `lines`, then `exit`, and return the reply lines."""
    completed = subprocess.run(
        [sys.executable, "-m", "yamaa._worker"],
        input="".join(line + "\n" for line in [*lines, "exit"]),
        capture_output=True,
        text=True,
        check=False,
    )
    if completed.returncode:
        raise AssertionError(completed.stderr)
    replies = completed.stdout.split("\n")
    assert replies[-1] == "" and replies[-2] == "ok", completed.stdout
    return replies[:-2]


class Worker(unittest.TestCase):
    def test_the_worker_names_the_installed_engine(self):
        self.assertEqual(worker(["info"]), ["ok\t" + enc(yamaa.info()["version"])])

    def test_the_worker_observes_the_installation_as_the_lock_verification_asks(self):
        replies = worker(
            [
                "version\tyamaa",
                "version\t" + enc("yamaa-no-such-distribution"),
                "standard_library\tjson",
                "standard_library\t" + enc("yamaa_benchmarks"),
                "providers\t" + enc("yamaa_benchmarks") + "\tjson",
                "environment",
                "integer_digits",
            ]
        )
        self.assertEqual(
            replies[0], "ok\tversion\t" + enc(locked.installed_version("yamaa"))
        )
        self.assertEqual(replies[1:4], ["ok\tmissing", "ok\ttrue", "ok\tfalse"])
        providers = locked._installed_providers(["yamaa_benchmarks", "json"])
        self.assertEqual(
            replies[4],
            "ok\t"
            + enc("yamaa_benchmarks")
            + "="
            + ",".join(map(enc, providers["yamaa_benchmarks"]))
            + "\tjson=",
        )
        environment = dict(
            (unquote(key), unquote(value))
            for key, value in (pair.split("=") for pair in replies[5].split("\t")[1:])
        )
        self.assertEqual(environment, locked.marker_environment())
        self.assertEqual(environment["python_version"], "%d.%d" % sys.version_info[:2])
        self.assertEqual(replies[6], f"ok\t{sys.get_int_max_str_digits()}")

    def test_a_bound_function_answers_each_call_and_each_row_of_a_batch(self):
        call = enc("yamaa_benchmarks.project_ratio")
        parameters = "\t".join(
            ["numerator", "denominator", "decimals", "adjust", "as_percent"]
        )
        replies = worker(
            [
                f"bind\t{call}\tfloat\t{parameters}",
                "invoke\t1\tnumerator=f:5.0\tdenominator=f:2.0\tdecimals=i:2\tadjust=f:0.0\tas_percent=b:false",
                "batch\t1\t2\t5",
                "numerator\tf:1.0\tf:2.0",
                "denominator\tf:3.0\tf:3.0",
                "decimals\ti:2\ti:2",
                "adjust\tf:0.0\tf:0.5",
                "as_percent\tb:false\tb:true",
                "invoke\t1\tnumerator=f:1.0\tdenominator=f:0.0\tdecimals=i:2\tadjust=f:0.0\tas_percent=b:false",
                "batch\t1\t0\t5",
                *[f"{name}" for name in parameters.split("\t")],
            ]
        )
        self.assertEqual(
            replies,
            [
                "ok\t1",
                "ok\tf:2.5",
                "ok\tf:0.33\tf:67.17",
                "error\tZeroDivisionError\t" + enc(zero_division()),
                "ok",
            ],
        )

    def test_a_result_is_spelled_as_the_in_process_port_admits_it(self):
        from yamaa import _worker

        for value, spelled in [
            (None, "-"),
            (True, "b:true"),
            (2**63 - 1, f"i:{2**63 - 1}"),
            (0.1, "f:0.1"),
            (float("inf"), "f:inf"),
            ("a b/\u00e9", "s:a%20b%2F%C3%A9"),
            (datetime.date(2024, 2, 29), "d:2024-02-29"),
            (datetime.datetime(999, 12, 31, 23, 59, 58), "t:0999-12-31T23:59:58"),
        ]:
            with self.subTest(value=value):
                self.assertEqual(_worker.encode_value(value), spelled)
                if value is not None and not isinstance(value, float):
                    self.assertEqual(_worker.decode_value(spelled), value)
        for value, reason, returned in [
            (2**63, "a returned integer exceeds 64 bits", None),
            (
                datetime.datetime(2024, 1, 1, tzinfo=datetime.timezone.utc),
                "a returned datetime carries a zone or a fraction of a second",
                None,
            ),
            ([1], "a binding returned a value of no scalar type", "list"),
        ]:
            with self.subTest(value=value):
                with self.assertRaises(_worker.Rejected) as caught:
                    _worker.encode_value(value)
                self.assertEqual(
                    (caught.exception.reason, caught.exception.returned),
                    (reason, returned),
                )

    def test_a_callable_that_cannot_be_bound_or_a_result_that_is_no_scalar_is_reported(
        self,
    ):
        replies = worker(
            [
                "bind\t" + enc("yamaa_benchmarks.no_such_function") + "\tstr\tx",
                "bind\t" + enc("json.dumps") + "\tstr\tobj",
                "bind\t" + enc("os.listdir") + "\tstr\tpath",
                "invoke\t1\tpath=s:.",
                "invoke\t7\tpath=s:.",
            ]
        )
        self.assertTrue(replies[0].startswith("error\tAttributeError\t"), replies[0])
        self.assertEqual(
            replies[1],
            "error\tInvalidBinding\t"
            + enc("installed callable signature differs from params"),
        )
        self.assertEqual(replies[2], "ok\t1")
        # os.listdir returns a list: no scalar.
        self.assertEqual(
            replies[3],
            "rejected\t"
            + enc("a binding returned a value of no scalar type")
            + "\tlist",
        )
        self.assertEqual(replies[4], "error\tKeyError\t7")

    def test_project_code_that_prints_reaches_standard_error_not_the_protocol(
        self,
    ):
        # A callable the worker binds from the json module, planted in the
        # worker's own process before the worker starts: it prints, as a
        # project function must not, and its print reaches standard error.
        code = (
            "import json, runpy, sys; "
            "json.shout = lambda x: (print('shouting ' + x), x.upper())[1]; "
            "sys.argv = ['python', '-m', 'yamaa._worker']; "
            "runpy.run_module('yamaa._worker', run_name='__main__')"
        )
        completed = subprocess.run(
            [sys.executable, "-c", code],
            input="bind\t"
            + enc("json.shout")
            + "\tstr\tx\ninvoke\t1\tx=s:hello\nexit\n",
            capture_output=True,
            text=True,
            check=True,
        )
        self.assertEqual(
            completed.stdout.split("\n"), ["ok\t1", "ok\ts:HELLO", "ok", ""]
        )
        self.assertEqual(completed.stderr, "shouting hello\n")
        self.assertEqual(platform.python_version(), yamaa.info()["host_version"])


if __name__ == "__main__":
    unittest.main()
