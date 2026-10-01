"""Grade one agent submission against a benchmark's golden datasets.

Runs in Harbor's separate verifier container as `/tests/grade.py` and also
standalone, so it uses the Python standard library only. It is a pure
function of the submitted CSV files, the required script (`result.R` or
`result.py`), the task's `contract.json`, the golden files, and the agent
trajectory; with `--rerun` it also runs that script.

The reward is 1 only when all of these hold:

- every requested dataset matches its golden file: the same columns, keys,
  and cell values once each value is read as its column's type;
- with `--rerun` (the verifier), the script, run from a clean state by its
  language's interpreter, writes datasets that match the golden too;
- the trajectory and the script use no other language and no web tool.

Column order and row order are reported but not graded. A grader failure
raises instead of writing `reward.json`, so Harbor records a verifier error
rather than a score.
"""

from __future__ import annotations

import argparse
import csv
import json
import math
import re
import subprocess
from collections.abc import Callable
from datetime import date, datetime
from pathlib import Path

NULL_TOKENS = frozenset({"", "NA", "NaN", "nan", ".", "NULL", "None"})
WEB_TOOLS = frozenset({"webfetch", "websearch", "web_fetch", "web_search"})
RELATIVE_TOLERANCE = 1e-9
MAX_REPORTED = 200
RERUN_TIMEOUT_SEC = 300.0
# How each track's script is run, and what counts as reaching for the other
# language: running one of its programs in a shell call, or a bridge in the
# script. Only a call counts: the program's name in a grep pattern, a quoted
# string, a comment, or a heredoc body is not one.
INTERPRETERS = {"r": "Rscript", "python": "python3"}
OTHER_LANGUAGE_PROGRAMS = {
    "r": re.compile(r"python[\d.]*|pip[\d.]*|ipython[\d.]*|jupyter|uvx?"),
    # `r` is littler, the R front end rocker images install.
    "python": re.compile(r"R|Rscript|r"),
}
OTHER_LANGUAGE_IN_SCRIPT = {
    "r": re.compile(
        r"\b(?:library|require|requireNamespace)\s*\(\s*['\"]?reticulate\b"
        r"|\breticulate::"
        r"|\bsystem2?\s*\(\s*['\"][^'\"]*\b(?:python[\d.]*|pip[\d.]*)\b"
    ),
    "python": re.compile(
        r"\b(?:import|from)\s+rpy2\b"
        r"|\b(?:subprocess\.\w+|os\.(?:system|popen|exec\w*|spawn\w*))\s*\("
        r"\s*\[?\s*['\"](?:[^'\"]*/)?(?:Rscript|R)\b"
    ),
}
SHELL_TOOLS = ("bash", "shell", "exec", "terminal", "command")
# Programs that run the command after them, and shells that run a `-c` script.
WRAPPERS = {"sudo", "env", "time", "exec", "nohup", "command", "nice", "timeout"}
WRAPPERS |= {"xargs", "stdbuf", "builtin"}
# Wrapper options followed by a value: `sudo -u agent`, `nice -n 5`,
# `timeout -s KILL 60`, `env -u NAME`.
WRAPPER_VALUE_OPTIONS = {"-u", "-g", "-n", "-s", "-k", "-C", "-p"}
SHELLS = {"bash", "sh", "dash", "zsh"}
HEREDOC = re.compile(r"<<(-?)\s*(['\"]?)([\w.-]+)\2")


class SubmissionError(ValueError):
    """The submitted file cannot be read as a dataset."""


def normalize(value: str, kind: str) -> object:
    """A cell as a comparable value of its column type (`None` for no value).

    Text keeps surrounding blanks, so a value of spaces is not missing; a
    number or date cell that is only blanks is.
    """
    text = value.strip()
    if kind == "str":
        if value == "" or (text and text in NULL_TOKENS):
            return None
        return value
    if text in NULL_TOKENS:
        return None
    if kind in ("int", "float"):
        try:
            number = float(text)
        except ValueError:
            return ("unparsed", text)
        return number if math.isfinite(number) else ("unparsed", text)
    if kind == "date":
        try:
            parsed = datetime.fromisoformat(text.replace("Z", "+00:00"))
        except ValueError:
            return ("unparsed", text)
        if isinstance(parsed, datetime) and parsed.time() != datetime.min.time():
            return ("unparsed", text)
        return parsed.date() if isinstance(parsed, datetime) else parsed
    if kind == "datetime":
        try:
            return datetime.fromisoformat(text.replace("Z", "+00:00"))
        except ValueError:
            return ("unparsed", text)
    return ("unparsed", text)


def same(expected: object, actual: object) -> bool:
    if isinstance(expected, float) and isinstance(actual, float):
        scale = max(1.0, abs(expected), abs(actual))
        return abs(expected - actual) <= RELATIVE_TOLERANCE * scale
    return expected == actual


def read_rows(path: Path) -> tuple[list[str], list[dict[str, str]]]:
    try:
        with path.open(newline="", encoding="utf-8-sig") as handle:
            reader = csv.reader(handle)
            header = next(reader, None)
            if header is None:
                raise SubmissionError(f"{path.name} is empty")
            header = [name.strip() for name in header]
            rows = []
            for number, record in enumerate(reader, start=2):
                if not record:
                    continue
                if len(record) != len(header):
                    raise SubmissionError(
                        f"{path.name} line {number} has {len(record)} fields, "
                        f"header has {len(header)}"
                    )
                rows.append(dict(zip(header, record, strict=True)))
    except UnicodeDecodeError as exc:
        raise SubmissionError(f"{path.name} is not UTF-8: {exc}") from exc
    except csv.Error as exc:
        raise SubmissionError(f"{path.name} is not valid CSV: {exc}") from exc
    return header, rows


def index_rows(
    rows: list[dict[str, str]], keys: list[str], types: dict[str, str]
) -> tuple[dict[tuple, dict[str, str]], list[str]]:
    """Rows by normalized key, and problems with the keys themselves."""
    indexed: dict[tuple, dict[str, str]] = {}
    problems = []
    for row in rows:
        key = tuple(normalize(row[k], types[k]) for k in keys)
        if any(part is None for part in key):
            problems.append(f"row with a missing key value: {display(key)}")
        elif key in indexed:
            problems.append(f"duplicate key: {display(key)}")
        else:
            indexed[key] = row
    return indexed, problems


def display(value: object) -> str:
    if isinstance(value, tuple) and value[:1] == ("unparsed",):
        return str(value[1])
    if isinstance(value, tuple):
        return "|".join(display(part) for part in value)
    if value is None:
        return ""
    if isinstance(value, float) and value.is_integer():
        return str(int(value))
    if isinstance(value, (date, datetime)):
        return value.isoformat()
    return str(value)


def grade_output(spec: dict, expected_dir: Path, output_dir: Path) -> dict:
    """Compare one requested dataset with its golden file."""
    name, keys, types = spec["file"], spec["keys"], spec["types"]
    columns = spec["columns"]
    result: dict = {"file": name, "passed": False, "problems": [], "diffs": []}
    golden_header, golden_rows = read_rows(expected_dir / name)
    if golden_header != columns:
        raise RuntimeError(f"golden {name} header differs from the contract")
    golden, golden_problems = index_rows(golden_rows, keys, types)
    if golden_problems:
        raise RuntimeError(f"golden {name}: {golden_problems[0]}")
    cells = len(golden) * (len(columns) - len(keys))
    result.update(expected_rows=len(golden), expected_cells=cells)
    result.update(matched_rows=0, matched_cells=0)

    path = output_dir / name
    if not path.is_file():
        result["problems"].append(f"{name} was not written")
        return result
    try:
        header, rows = read_rows(path)
    except SubmissionError as exc:
        result["problems"].append(str(exc))
        return result

    missing = [c for c in columns if c not in header]
    extra = [c for c in header if c not in columns]
    duplicated = sorted({c for c in header if header.count(c) > 1})
    result["column_order_matches"] = header == columns
    if missing:
        result["problems"].append(f"missing columns: {', '.join(missing)}")
    if extra:
        result["problems"].append(f"unexpected columns: {', '.join(extra)}")
    if duplicated:
        result["problems"].append(f"repeated columns: {', '.join(duplicated)}")
    if any(k in missing for k in keys) or duplicated:
        return result

    actual, key_problems = index_rows(rows, keys, types)
    result["problems"].extend(key_problems[:MAX_REPORTED])
    absent = [k for k in golden if k not in actual]
    unexpected = [k for k in actual if k not in golden]
    if absent:
        result["problems"].append(
            f"{len(absent)} expected row(s) missing, first: {display(absent[0])}"
        )
    if unexpected:
        result["problems"].append(
            f"{len(unexpected)} unexpected row(s), first: {display(unexpected[0])}"
        )

    compared = [c for c in columns if c not in keys and c not in missing]
    matched_rows = matched_cells = 0
    for key, want in golden.items():
        got = actual.get(key)
        if got is None:
            continue
        row_ok = True
        for column in compared:
            expected = normalize(want[column], types[column])
            value = normalize(got[column], types[column])
            if same(expected, value):
                matched_cells += 1
                continue
            row_ok = False
            if len(result["diffs"]) < MAX_REPORTED:
                result["diffs"].append(
                    {
                        "key": display(key),
                        "column": column,
                        "expected": want[column],
                        "actual": got[column],
                    }
                )
        matched_rows += row_ok
    result.update(matched_rows=matched_rows, matched_cells=matched_cells)
    mismatched = cells - matched_cells
    if mismatched:
        result["problems"].append(f"{mismatched} cell(s) differ from the golden")
    result["passed"] = not result["problems"]
    return result


def _strings(value: object) -> list[str]:
    if isinstance(value, str):
        return [value]
    if isinstance(value, dict):
        return [s for v in value.values() for s in _strings(v)]
    if isinstance(value, list):
        return [s for v in value for s in _strings(v)]
    return []


def _matching(text: str, start: int, opening: str, closing: str) -> int:
    """The index just past the bracket that closes the one before `start`."""
    depth, i = 1, start
    while i < len(text) and depth:
        depth += {opening: 1, closing: -1}.get(text[i], 0)
        i += 1
    return i


def shell_commands(script: str) -> list[list[str]]:
    """The simple commands of a shell script, each as its words.

    Quotes keep their contents in one word, so a separator or a program name
    inside them is text. A comment and a heredoc body are not commands.
    `$(...)` and backticks are commands of their own, and so is the script a
    shell runs with `-c`.
    """
    commands: list[list[str]] = []
    words: list[str] = []
    word: list[str] = []
    in_word = False
    heredocs: list[tuple[bool, str]] = []
    quote = ""
    i = 0

    def end_word() -> None:
        nonlocal word, in_word
        if in_word:
            words.append("".join(word))
        word, in_word = [], False

    def end_command() -> None:
        nonlocal words
        end_word()
        if words:
            commands.append(words)
        words = []

    while i < len(script):
        char = script[i]
        if quote == "'":
            if char == "'":
                quote = ""
            else:
                word.append(char)
            i += 1
            continue
        if char == "\\" and i + 1 < len(script):
            if script[i + 1] != "\n":
                word.append(script[i + 1])
                in_word = True
            i += 2
            continue
        if script.startswith("$(", i) and not script.startswith("$((", i):
            end = _matching(script, i + 2, "(", ")")
            commands.extend(shell_commands(script[i + 2 : end - 1]))
            in_word, i = True, end
            continue
        if char == "`":
            end = script.find("`", i + 1)
            end = len(script) if end < 0 else end
            commands.extend(shell_commands(script[i + 1 : end]))
            in_word, i = True, end + 1
            continue
        if quote == '"':
            if char == '"':
                quote = ""
            else:
                word.append(char)
            i += 1
            continue
        if char in "'\"":
            quote, in_word = char, True
            i += 1
            continue
        if char == "#" and not in_word:
            newline = script.find("\n", i)
            i = len(script) if newline < 0 else newline
            continue
        heredoc = HEREDOC.match(script, i) if char == "<" else None
        if heredoc:
            end_word()
            heredocs.append((heredoc.group(1) == "-", heredoc.group(3)))
            i = heredoc.end()
            continue
        if char == "\n":
            end_command()
            i += 1
            for strip_tabs, delimiter in heredocs:
                while i < len(script):
                    newline = script.find("\n", i)
                    newline = len(script) if newline < 0 else newline
                    line = script[i:newline]
                    i = newline + 1
                    if (line.lstrip("\t") if strip_tabs else line) == delimiter:
                        break
            heredocs = []
            continue
        if char in ";&|()":
            end_command()
        elif char in "<> \t\r":
            end_word()
        else:
            word.append(char)
            in_word = True
        i += 1
    end_command()
    nested = []
    for command in commands:
        program, arguments = _program(command)
        if program in SHELLS and "-c" in arguments:
            position = arguments.index("-c") + 1
            if position < len(arguments):
                nested.extend(shell_commands(arguments[position]))
    return commands + nested


def _program(command: list[str]) -> tuple[str, list[str]]:
    """The program a simple command runs, past variable assignments and
    wrappers such as `env` or `timeout 60`, and its arguments."""
    words = list(command)
    while words:
        name = words[0].rsplit("/", 1)[-1]
        if re.fullmatch(r"[A-Za-z_]\w*=.*", words[0]):
            words.pop(0)
        elif name in WRAPPERS:
            words.pop(0)
            while words and re.fullmatch(r"-\S*|\d+[smhd]?", words[0]):
                option = words.pop(0)
                if option in WRAPPER_VALUE_OPTIONS and words:
                    words.pop(0)
        else:
            return name, words[1:]
    return "", []


def other_language_calls(script: str, language: str | None) -> list[str]:
    """The commands in a shell script that run the other track's language."""
    pattern = OTHER_LANGUAGE_PROGRAMS.get(language or "")
    if pattern is None:
        return []
    calls = []
    for command in shell_commands(script):
        program, _ = _program(command)
        if pattern.fullmatch(program):
            calls.append(" ".join(command))
    return calls


def scan_trajectory(path: Path, language: str | None) -> dict:
    """Web tool calls, and shell calls to the other track's language, in the
    agent's ATIF trajectory, when there is one."""
    if not path.is_file():
        return {"checked": False, "violations": [], "language_violations": []}
    trajectory = json.loads(path.read_text(encoding="utf-8"))
    violations, language_violations = [], []
    for index, step in enumerate(trajectory.get("steps") or []):
        for call in (step or {}).get("tool_calls") or []:
            tool = str(call.get("function_name") or call.get("name") or "")
            if tool.lower() in WEB_TOOLS:
                violations.append({"step": index, "tool": tool})
            if not any(h in tool.lower() for h in SHELL_TOOLS):
                continue
            # The command a shell tool ran, not its free-text description.
            arguments = call.get("arguments")
            if isinstance(arguments, dict) and "command" in arguments:
                scripts = _strings(arguments["command"])
            else:
                scripts = _strings(arguments)
            for script in scripts:
                calls = other_language_calls(script, language)
                if calls:
                    language_violations.append(
                        {"step": index, "excerpt": calls[0][:200]}
                    )
                    break
    return {
        "checked": True,
        "violations": violations,
        "language_violations": language_violations,
    }


def grade_script(contract: dict, output_dir: Path) -> dict:
    """The required `result.R`/`result.py` exists and is not empty."""
    name = contract.get("script")
    result: dict = {"file": name, "passed": False, "problems": []}
    if not name:
        return {**result, "passed": True}
    path = output_dir / name
    if not path.is_file():
        result["problems"].append(f"{name} was not written")
        return result
    try:
        text = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError) as exc:
        result["problems"].append(f"{name} cannot be read: {exc}")
        return result
    if not text.strip():
        result["problems"].append(f"{name} is empty")
        return result
    pattern = OTHER_LANGUAGE_IN_SCRIPT.get(contract.get("language") or "")
    match = pattern.search(text) if pattern else None
    if match:
        result["problems"].append(f"{name} calls another language: {match.group(0)!r}")
        return result
    result["passed"] = True
    return result


RunScript = Callable[[list[str], Path, float], tuple[int | None, str]]
"""`run(command, cwd, timeout) -> (returncode or None on timeout, output)`."""


def run_script(command: list[str], cwd: Path, timeout: float) -> tuple[int | None, str]:
    try:
        done = subprocess.run(
            command,
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return None, f"timed out after {timeout:.0f}s"
    except OSError as exc:
        return 127, str(exc)
    return done.returncode, (done.stdout + done.stderr)[-4000:]


def rerun_script(
    contract: dict,
    expected_dir: Path,
    output_dir: Path,
    run: RunScript = run_script,
    timeout: float = RERUN_TIMEOUT_SEC,
) -> dict:
    """Delete the submitted datasets, run the script by its track's
    interpreter from the app directory, and grade what it writes.

    Destructive by design: it runs in the throwaway verifier container.
    """
    name = contract.get("script")
    language = contract.get("language")
    result: dict = {"passed": False, "problems": [], "outputs": []}
    if not name or language not in INTERPRETERS:
        return {**result, "passed": True, "skipped": True}
    script = output_dir / name
    if not script.is_file():
        result["problems"].append(f"{name} was not written")
        return result
    for spec in contract["outputs"]:
        (output_dir / spec["file"]).unlink(missing_ok=True)
    returncode, log = run(
        [INTERPRETERS[language], str(script)], output_dir.parent, timeout
    )
    result.update(returncode=returncode, log=log)
    if returncode != 0:
        result["problems"].append(
            f"{INTERPRETERS[language]} {name} failed"
            + (
                f" with exit code {returncode}"
                if returncode is not None
                else f": {log}"
            )
        )
    outputs = [grade_output(s, expected_dir, output_dir) for s in contract["outputs"]]
    result["outputs"] = outputs
    for output in outputs:
        result["problems"].extend(
            f"rerun {output['file']}: {p}" for p in output["problems"]
        )
    result["passed"] = returncode == 0 and all(o["passed"] for o in outputs)
    return result


def grade(
    contract: dict,
    expected_dir: Path,
    output_dir: Path,
    trajectory: Path,
    *,
    rerun: bool = False,
    run: RunScript = run_script,
) -> dict:
    outputs = [grade_output(s, expected_dir, output_dir) for s in contract["outputs"]]
    script = grade_script(contract, output_dir)
    network = scan_trajectory(trajectory, contract.get("language"))
    reproduced = (
        rerun_script(contract, expected_dir, output_dir, run)
        if rerun and script["passed"]
        else {"passed": not rerun, "skipped": not rerun, "problems": [], "outputs": []}
    )
    if rerun and not script["passed"]:
        reproduced["problems"].append("no script to rerun")
    cells = sum(o["expected_cells"] for o in outputs)
    rows = sum(o["expected_rows"] for o in outputs)
    passed = (
        all(o["passed"] for o in outputs)
        and script["passed"]
        and reproduced["passed"]
        and not network["violations"]
        and not network["language_violations"]
    )
    return {
        "benchmark": contract["benchmark"],
        "language": contract.get("language"),
        "passed": passed,
        "reward": {
            "reward": 1.0 if passed else 0.0,
            "cell_accuracy": sum(o["matched_cells"] for o in outputs) / cells
            if cells
            else 1.0,
            "row_accuracy": sum(o["matched_rows"] for o in outputs) / rows
            if rows
            else 1.0,
            "web_tool_calls": float(len(network["violations"])),
            "reproduced": 1.0
            if reproduced["passed"] and not reproduced.get("skipped")
            else 0.0,
            "language_violations": float(len(network["language_violations"])),
        },
        "outputs": outputs,
        "script": script,
        "rerun": reproduced,
        "network": network,
    }


def write_results(result: dict, out: Path) -> None:
    out.mkdir(parents=True, exist_ok=True)
    (out / "grade.json").write_text(json.dumps(result, indent=2) + "\n")
    for output in result["outputs"]:
        if output["diffs"]:
            with (out / f"diff-{output['file']}").open("w", newline="") as handle:
                writer = csv.DictWriter(
                    handle, fieldnames=["key", "column", "expected", "actual"]
                )
                writer.writeheader()
                writer.writerows(output["diffs"])
    (out / "reward.json").write_text(json.dumps(result["reward"], indent=2) + "\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--contract", type=Path, default=Path("/tests/contract.json"))
    parser.add_argument("--expected", type=Path, default=Path("/tests/expected"))
    parser.add_argument("--output", type=Path, default=Path("/app/output"))
    parser.add_argument(
        "--trajectory", type=Path, default=Path("/logs/agent/trajectory.json")
    )
    parser.add_argument("--out", type=Path, default=Path("/logs/verifier"))
    parser.add_argument(
        "--rerun",
        action="store_true",
        help="delete the datasets, rerun the script, and grade its output "
        "(destructive: for the verifier container)",
    )
    args = parser.parse_args()
    contract = json.loads(args.contract.read_text(encoding="utf-8"))
    result = grade(
        contract, args.expected, args.output, args.trajectory, rerun=args.rerun
    )
    write_results(result, args.out)
    status = "PASS" if result["passed"] else "FAIL"
    print(f"{status} {contract['benchmark']}: {json.dumps(result['reward'])}")
    for output in result["outputs"]:
        for problem in output["problems"]:
            print(f"  {output['file']}: {problem}")
    for problem in result["script"]["problems"] + result["rerun"]["problems"]:
        print(f"  {problem}")
    for violation in result["network"].get("language_violations", []):
        print(f"  other language at step {violation['step']}: {violation['excerpt']}")


if __name__ == "__main__":
    main()
