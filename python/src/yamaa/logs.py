"""The warning and verification logs (execution/verification): the sidecar
declarations, each declared check's outcome, and the two governed datasets
a run renders from them (storage/publication)."""

import json
import os

from . import expr as _expr
from . import pred as _pred
from . import validate as _validate
from .csv_io import write_csv_text, write_parquet_bytes
from .errors import YamaaError
from .values import compare, float_text

LOG_VERSION = "1.0"

# REQ-0392: the warning log's columns, in order, with their types.
WARNING_LOG_COLUMNS = (
    ("LOG_VERSION", "str"),
    ("ARTIFACT", "str"),
    ("SEVERITY", "str"),
    ("CONDITION", "str"),
    ("REQUIREMENT", "str"),
    ("SPEC_PATH", "str"),
    ("VERIFICATION_ID", "str"),
    ("FAILURE_COUNT", "int"),
    ("OFFENDING_KEYS", "str"),
    ("DETAILS", "str"),
)

# REQ-1174: the verification log's columns, in order, with their types.
VERIFICATION_LOG_COLUMNS = (
    ("REPORT_VERSION", "str"),
    ("ARTIFACT", "str"),
    ("SPEC_PATH", "str"),
    ("VERIFICATION_ID", "str"),
    ("CHECK", "str"),
    ("TARGET", "str"),
    ("REQUIREMENT", "str"),
    ("SEVERITY", "str"),
    ("OUTCOME", "str"),
    ("CONDITION", "str"),
    ("EVALUATED_COUNT", "int"),
    ("FAILURE_COUNT", "int"),
    ("DETAILS", "str"),
)

# Each registered check's failed condition and the requirement defining it.
_CHECKS = {
    "not_missing": ("not_missing_failed", "REQ-0375"),
    "allowed_values": ("allowed_values_failed", "REQ-0376"),
    "range": ("range_failed", "REQ-0377"),
    "max_length": ("length_failed", "REQ-0378"),
    "matches": ("matches_failed", "REQ-0379"),
    "unique": ("unique_failed", "REQ-0381"),
    "all_or_none": ("all_or_none_failed", "REQ-0382"),
    "implies": ("implication_failed", "REQ-0383"),
    "assert": ("assert_failed", "REQ-0384"),
    "row_count": ("row_count_failed", "REQ-0385"),
}

_SEVERITIES = ["error", "warning"]
_PROFILES = [".csv", ".parquet"]


def _fail(where, condition, requirement, context):
    raise YamaaError(
        phase="validation",
        condition=condition,
        requirement=requirement,
        spec_paths=[where] if isinstance(where, str) else list(where),
        context=context,
    )


def _declarations(e):
    """Every declared column and dataset check as (path, kind, payload)."""
    decls = []
    for name in e.col_order:
        for i, v in enumerate(_validate._vlist(e.colspecs[name].get("verifications"))):
            decls.append((f"columns.{name}.verifications[{i}]", v))
    decls += [(f"verifications[{i}]", v) for i, v in enumerate(e.verifications)]
    for where, v in decls:
        if isinstance(v, dict) and len(v) == 1:
            kind, payload = next(iter(v.items()))
            yield f"{where}.{kind}", kind, payload


def check_declarations(e):
    """REQ-0389: a severity is `error` or `warning`. REQ-0391: a warning
    needs `output.warning_log`. REQ-0756/REQ-1180: the primary artifact and
    its sidecars name three different files, each of whose extensions
    selects a profile (REQ-0716)."""
    warnings = []
    for where, _kind, payload in _declarations(e):
        if not isinstance(payload, dict) or "severity" not in payload:
            continue
        severity = payload["severity"]
        if severity not in _SEVERITIES:
            _fail(
                f"{where}.severity",
                "value_not_permitted",
                "REQ-0389",
                {"value": severity, "permitted": list(_SEVERITIES)},
            )
        if severity == "warning":
            warnings.append(f"{where}.severity")
    out = e.output
    if warnings and out.get("warning_log") is None:
        _fail(
            "output.warning_log",
            "missing_warning_log",
            "REQ-0391",
            {"warnings": warnings},
        )
    for field in ("warning_log", "verification_log"):
        path = out.get(field)
        if path is not None and _profile(path) is None:
            _fail(
                f"output.{field}",
                "unknown_artifact_profile",
                "REQ-0760",
                {"path": path, "permitted": list(_PROFILES)},
            )
    for left, right, requirement in (
        ("path", "warning_log", "REQ-0756"),
        ("path", "verification_log", "REQ-1180"),
        ("warning_log", "verification_log", "REQ-1180"),
    ):
        a, b = out.get(left), out.get(right)
        if isinstance(a, str) and b is not None and _same_file(a, b):
            _fail(
                [f"output.{left}", f"output.{right}"],
                "artifact_path_collision",
                requirement,
                {"path": b},
            )


def _profile(path):
    """REQ-0716: the closed, case-insensitive extension mapping."""
    if not isinstance(path, str):
        return None
    ext = os.path.splitext(path)[1].lower()
    return ext[1:] if ext in _PROFILES else None


def _same_file(a, b):
    """Two declared paths collide when they name the same file."""
    return os.path.normpath(a) == os.path.normpath(b)


class Outcome:
    """One declared check the run evaluated (REQ-1173), held or violated:
    the executor's condition and the complete evidence the logs carry."""

    def __init__(self, spec_path, check, target, payload, condition, evidence):
        self.spec_path = spec_path
        self.check = check
        self.target = target
        self.verification_id = payload.get("id")
        self.severity = payload.get("severity", "error")
        self.requirement = _CHECKS[check][1]
        self.condition = condition
        self.evaluated, self.failure_count, self.keys, self.details = evidence


def record(e, where, kind, payload, col, verdict):
    """Collect the outcome of the check at `where`, just decided by the
    executor, for the logs the specification declares. `verdict` is the
    executor's failed condition (alone or with its report), or None when
    the check held. A check no declared log reports is not examined again."""
    out = e.output
    condition = verdict[0] if isinstance(verdict, tuple) else verdict
    severity = payload.get("severity", "error")
    if out.get("verification_log") is None and (
        condition is None or severity != "warning"
    ):
        return
    evidence = _evidence(e, kind, payload, col, where)
    # REQ-0395/REQ-1178: the logs correspond one-to-one with the executor's
    # findings, so evidence the executor did not find is a defect.
    found = _CHECKS[kind][0] if evidence[1] else None
    if found != condition:
        raise RuntimeError(
            f"verification log defect at {where}: the executor found "
            f"{condition!r}, the log evidence {found!r}"
        )
    e._outcomes.append(Outcome(where, kind, col, payload, condition, evidence))


def _evidence(e, kind, payload, col, where):
    """(evaluated count, failure count, offending keys, details) of one check
    over the completed rows. The keys are every offending row's output keys,
    or every offending group, rather than an error report's sample
    (REQ-0393); the counts count the unit the check counts (REQ-1176)."""
    from .engine import _ColCtx, _hashable  # the executor's own row reads

    rows = e.rows
    if kind == "row_count":
        return _row_count(e, payload, where, _hashable)
    if kind == "unique":
        cols = payload["columns"]
        parts = {}
        for i, r in enumerate(rows):
            parts.setdefault(tuple(_hashable(r.get(c)) for c in cols), []).append(i)
        repeated = [p for p in parts.values() if len(p) > 1]
        # A repeated combination is one failure; every row carrying it is
        # offending, since those rows differ in their own keys.
        keys = [_expr.keys_of(e, rows[i]) for p in repeated for i in p]
        details = {"columns": list(cols)} if repeated else {}
        return len(parts), len(repeated), keys, details
    details = {}
    if kind == "all_or_none":
        cols = payload["columns"]
        bad = [r for r in rows if len({r.get(c) is None for c in cols}) > 1]
    elif kind == "assert":
        node, _ = _pred.parse(payload["expr"], where)
        bad = [
            rows[i]
            for i in range(len(rows))
            if _pred.evaluate(node, _ColCtx(e, i, where).value, where) is not True
        ]
    elif kind == "implies":
        wn, _ = _pred.parse(payload["when"], where)
        tn, _ = _pred.parse(payload["then"], where)
        bad = []
        for i in range(len(rows)):
            ctx = _ColCtx(e, i, where)
            if (
                _pred.evaluate(wn, ctx.value, where) is True
                and _pred.evaluate(tn, ctx.value, where) is not True
            ):
                bad.append(rows[i])
    else:
        col = col or payload.get("column")
        details["column"] = col
        if kind == "max_length":
            details["max"] = payload["max"]
        offends = _value_test(kind, payload)
        bad = [r for r in rows if offends(r.get(col))]
    # REQ-1176: a held check carries no details.
    details = details if bad else {}
    return len(rows), len(bad), [_expr.keys_of(e, r) for r in bad], details


def _value_test(kind, payload):
    """Whether one column value violates a column check. Every check but
    `not_missing` passes a missing value (REQ-0375)."""
    if kind == "not_missing":
        return lambda v: v is None
    if kind == "allowed_values":
        allowed = set(payload["values"])
        return lambda v: v is not None and v not in allowed
    if kind == "range":
        low, high = payload.get("min"), payload.get("max")
        return lambda v: (
            v is not None
            and (
                (low is not None and compare(v, low) < 0)
                or (high is not None and compare(v, high) > 0)
            )
        )
    if kind == "max_length":
        return lambda v: v is not None and len(v) > payload["max"]
    rx = _pred.normalize_pattern(payload["pattern"])
    return lambda v: v is not None and not rx.search(v)


def _row_count(e, payload, where, hashable):
    """REQ-0385/0386/0387/1154: every bound group whose filtered count
    breaks a bound, with its count and, under a fraction bound, the
    unfiltered group size it divides by."""
    rows = e.rows

    def predicate(text):
        if text is None:
            return None
        node, _ = _pred.parse(text, where)
        return lambda i: _pred.evaluate(node, rows[i].get, where) is True

    admits, binds = predicate(payload.get("filter")), predicate(payload.get("when"))
    group_by = payload.get("group_by") or []
    groups = [list(range(len(rows)))]
    if group_by:
        parts = {}
        for i, r in enumerate(rows):
            parts.setdefault(tuple(hashable(r.get(g)) for g in group_by), []).append(i)
        groups = list(parts.values())
    low, high = payload.get("min"), payload.get("max")
    low_share, high_share = payload.get("min_fraction"), payload.get("max_fraction")
    fraction = low_share is not None or high_share is not None
    keys, counts, sizes = [], [], []
    for g in groups:
        if binds is not None and not any(binds(i) for i in g):
            continue  # REQ-1154: an exempt group
        n = sum(1 for i in g if admits is None or admits(i))
        share = n / len(g) if g else 0.0
        if (
            (low is not None and n < low)
            or (high is not None and n > high)
            or (low_share is not None and share < low_share)
            or (high_share is not None and share > high_share)
        ):
            keys.append({c: _expr.json_value(rows[g[0]].get(c)) for c in group_by})
            counts.append(n)
            sizes.append(len(g))
    if not keys:
        details = {}
    elif group_by:
        # REQ-0393: the complete counts, aligned with the offending groups.
        details = {"counts": counts}
        if fraction:
            details["denominators"] = sizes
    else:
        details = {"count": counts[0]}
        if fraction:
            details["denominator"] = sizes[0]
    return len(groups), len(keys), keys, details


def canonical_json(v):
    """REQ-0394: compact ASCII JSON, object names in Text-values order,
    numbers in their `str` form, and non-ASCII escaped in lower-case hex
    (a scalar above U+FFFF as its surrogate pair)."""
    if v is None:
        return "null"
    if v is True:
        return "true"
    if v is False:
        return "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        return float_text(v)
    if isinstance(v, str):
        return json.dumps(v, ensure_ascii=True)
    if isinstance(v, list):
        return "[" + ",".join(canonical_json(x) for x in v) + "]"
    if isinstance(v, dict):
        return (
            "{"
            + ",".join(
                canonical_json(k) + ":" + canonical_json(v[k]) for k in sorted(v)
            )
            + "}"
        )
    raise TypeError(f"no canonical JSON for {type(v).__name__}")


def sidecars(e, succeeded=True):
    """{declared path: file} for the logs the specification declares, in the
    order a publisher replaces them (REQ-1181): the verification log, then
    the warning log. A csv file is its text, a parquet file its bytes.

    A successful run renders both, the warning log even with no violated
    warning (REQ-0391) and the verification log even with no check
    (REQ-1173). A failed run renders its verification log alone, of the
    checks it evaluated (REQ-1177). Each log is verified before it is
    rendered (REQ-0395/REQ-1178)."""
    out = e.output
    artifact = out["path"]
    # REQ-0393/REQ-1175: column checks in declaration order, then dataset
    # checks, however the executor interleaved them.
    outcomes = [o for o in e._outcomes if o.target is not None]
    outcomes += [o for o in e._outcomes if o.target is None]
    findings = [o for o in outcomes if o.severity == "warning" and o.failure_count]
    warning_rows = [_warning_row(artifact, o) for o in findings]
    _verify_warning_log(warning_rows, findings)
    files = {}
    if out.get("verification_log") is not None:
        rows = [_verification_row(artifact, o) for o in outcomes]
        _verify_verification_log(rows, outcomes, warning_rows)
        files[out["verification_log"]] = _render(
            out["verification_log"], VERIFICATION_LOG_COLUMNS, rows
        )
    if succeeded and out.get("warning_log") is not None:
        files[out["warning_log"]] = _render(
            out["warning_log"], WARNING_LOG_COLUMNS, warning_rows
        )
    return files


def _warning_row(artifact, o):
    return {
        "LOG_VERSION": LOG_VERSION,
        "ARTIFACT": artifact,
        "SEVERITY": "warning",
        "CONDITION": o.condition,
        "REQUIREMENT": o.requirement,
        "SPEC_PATH": o.spec_path,
        "VERIFICATION_ID": o.verification_id,
        "FAILURE_COUNT": o.failure_count,
        "OFFENDING_KEYS": canonical_json(o.keys),
        "DETAILS": canonical_json(o.details),
    }


def _verification_row(artifact, o):
    # REQ-1175: the offending keys stay in the warning log.
    return {
        "REPORT_VERSION": LOG_VERSION,
        "ARTIFACT": artifact,
        "SPEC_PATH": o.spec_path,
        "VERIFICATION_ID": o.verification_id,
        "CHECK": o.check,
        "TARGET": o.target,
        "REQUIREMENT": o.requirement,
        "SEVERITY": o.severity,
        "OUTCOME": "violated" if o.failure_count else "held",
        "CONDITION": o.condition,
        "EVALUATED_COUNT": o.evaluated,
        "FAILURE_COUNT": o.failure_count,
        "DETAILS": canonical_json(o.details),
    }


def _defect(log, reason):
    """REQ-0395/REQ-1178: a malformed log is an execution defect, not a
    finding that can be recorded inside itself."""
    raise RuntimeError(f"malformed {log}: {reason}")


def _verify_rows(log, columns, rows, version_column):
    """The fixed schema, version, and SPEC_PATH key both logs share."""
    seen = set()
    for row in rows:
        if list(row) != [name for name, _ in columns]:
            _defect(log, "columns differ from the fixed schema")
        for name, typ in columns:
            v = row[name]
            if typ == "int" and (not isinstance(v, int) or isinstance(v, bool)):
                _defect(log, f"{name} is not an int")
            if typ == "str" and v is not None and not isinstance(v, str):
                _defect(log, f"{name} is not a str")
        if row[version_column] != LOG_VERSION:
            _defect(log, f"{version_column} is not {LOG_VERSION}")
        path = row["SPEC_PATH"]
        if path is None or path in seen:
            _defect(log, f"SPEC_PATH {path!r} is missing or repeated")
        seen.add(path)


def _verify_warning_log(rows, findings):
    """REQ-0395: one row per violated warning, in the executor's order."""
    _verify_rows("warning log", WARNING_LOG_COLUMNS, rows, "LOG_VERSION")
    if [r["SPEC_PATH"] for r in rows] != [o.spec_path for o in findings]:
        _defect("warning log", "rows do not correspond to the warning findings")
    for row, o in zip(rows, findings):
        keys = json.loads(row["OFFENDING_KEYS"])
        # A repeated `unique` combination offends on at least two rows.
        least = 2 * o.failure_count if o.check == "unique" else o.failure_count
        if row["SEVERITY"] != "warning" or row["CONDITION"] != o.condition:
            _defect("warning log", f"{o.spec_path} is not its warning finding")
        if row["FAILURE_COUNT"] < 1 or len(keys) < least:
            _defect("warning log", f"{o.spec_path} lacks its complete evidence")
        if o.check != "unique" and len(keys) != o.failure_count:
            _defect("warning log", f"{o.spec_path} keys disagree with its count")


def _verify_verification_log(rows, outcomes, warning_rows):
    """REQ-1178: one row per evaluated check, the REQ-1176 counts, and
    agreement with the warning log about every warning it carries."""
    _verify_rows("verification log", VERIFICATION_LOG_COLUMNS, rows, "REPORT_VERSION")
    if [r["SPEC_PATH"] for r in rows] != [o.spec_path for o in outcomes]:
        _defect("verification log", "rows do not correspond to the checks evaluated")
    for row in rows:
        held = row["OUTCOME"] == "held"
        if row["OUTCOME"] not in ("held", "violated"):
            _defect("verification log", f"{row['SPEC_PATH']} has no valid outcome")
        if row["SEVERITY"] not in _SEVERITIES:
            _defect("verification log", f"{row['SPEC_PATH']} has no valid severity")
        if not 0 <= row["FAILURE_COUNT"] <= row["EVALUATED_COUNT"]:
            _defect("verification log", f"{row['SPEC_PATH']} miscounts its units")
        if held != (row["FAILURE_COUNT"] == 0) or held != (row["CONDITION"] is None):
            _defect("verification log", f"{row['SPEC_PATH']} contradicts its outcome")
        if held and row["DETAILS"] != "{}":
            _defect("verification log", f"{row['SPEC_PATH']} holds with details")
    by_path = {r["SPEC_PATH"]: r for r in rows}
    for w in warning_rows:
        row = by_path.get(w["SPEC_PATH"])
        shared = ("CONDITION", "REQUIREMENT", "VERIFICATION_ID", "FAILURE_COUNT")
        if (
            row is None
            or row["OUTCOME"] != "violated"
            or any(row[c] != w[c] for c in (*shared, "DETAILS"))
        ):
            _defect("verification log", f"{w['SPEC_PATH']} disagrees with its warning")


def _render(path, columns, rows):
    """A log's file under the profile its own extension selects (REQ-0391,
    REQ-1173); a log with no rows is its header alone."""
    names = [name for name, _ in columns]
    types = dict(columns)
    if _profile(path) == "parquet":
        return write_parquet_bytes(names, rows, types)
    return write_csv_text(names, rows, types)
