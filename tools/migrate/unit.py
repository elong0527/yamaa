"""Convert a benchmark directory, a project, or a list of files.

A unit is the set of files whose conversion needs each other: a benchmark
directory (its specifications, parent layers, schema producers, project and
environment files, metadata, negative contract and README), or a directory
holding a project file. The unit decides what no single document can:

- which specifications are entries, and their `model` (P-MODEL-1): from the
  project section that lists them, else from `benchmark.yaml`'s `standard`;
- today's inferred join keys, written on the layer that declares each input
  or intermediate (D23, D63, P-JOIN-1, P-KEY-1);
- the `clear` entries of a child layer's YAML nulls (P-CLR-1);
- the D8 merge of each project's environment files into `type: function`
  files at `functions/<name>.yaml` beside the project file;
- the spec paths and condition of a negative `expected/error.yaml`.

Every other file is copied unchanged; `benchmark.yaml` is read, never
rewritten. Nothing is written in place: the caller writes the result into a
separate target directory.
"""

import os
import re
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath

import yaml
from tools.migrate import contracts, markdown, schema
from tools.migrate import keys as keys_module
from tools.migrate import table as table_module
from tools.migrate.convert import (
    DEFAULT_MISSING,
    IDENTITY,
    TRANSFORMS,
    Converter,
    Note,
    Plan,
    clear_path,
)
from tools.migrate.text import (
    LINE_BREAK,
    Edit,
    EditPlan,
    Text,
    comment_texts,
    comments,
    compose,
    end,
    is_null,
    member,
    member_pair,
    pair_end,
    plain,
    start,
    value_of,
)

DRIVER_KEYLESS = markdown.DRIVER_KEYLESS
KEY_UNINFERABLE = markdown.KEY_UNINFERABLE
PROJECT_FIELDS = frozenset(
    {"environments", "codelists", "study", "sdtm", "adam", "send"}
)
ENVIRONMENT_FIELDS = frozenset({"functions", "lock"})
SECTION_MODELS = {"sdtm": "SDTM", "adam": "ADaM", "send": "SEND"}
STANDARD_MODELS = {"SDTM": "SDTM", "ADaM": "ADaM", "SEND": "SEND"}
HOSTS = ("python", "r")
SUFFIXES = (".yaml", ".yml")
RETIRED_COVERS = ("literal", "compute")

# Comment tokens that name a construct the table changes.
COMMENT_SPELLINGS = re.compile(
    r"\{\s*literal\s*:|\bliteral:|\bcompute\b|\{\s*source\s*:|\bschema_version\b"
    r"|\btie_break\b|\bminimum_source_precision\b|\bderive\b|\bbounds\b"
    r"|\breference_date\b|not_missing:\s*\{\}|environment\.yaml|\bno_match\b"
)

# The converter footprint rows of the RFC's method page, each the sum of
# the table entries (or the counts in COUNTERS) it names.
ROWS = {
    "header (D4)": (
        "spec.root_class.schema_version",
        "project.project_class.schema_version",
        "environment.environment_class.schema_version",
    ),
    "literal (D18)": ("spec.expressions.literal",),
    "{literal: null} in value fields (D18)": ("spec.missing_values.literal",),
    "tagged function arguments (D18)": (
        "spec.function_string_literal_class.literal",
        "spec.function_date_literal_class.date",
        "spec.function_datetime_literal_class.datetime",
    ),
    "intermediate no_match (P-HND-1, P-HND-3)": ("spec.intermediate_class.no_match",),
    "origins (P-ORIGIN-1)": ("spec.submission_column_class.origin",),
    "value-level origins (P-ORIGIN-1)": ("spec.submission_value_class.origin",),
    "compute (D18)": ("spec.expressions.compute",),
    "intermediates without key (D23)": (
        "spec.intermediate_class.key",
        DRIVER_KEYLESS,
        KEY_UNINFERABLE,
    ),
    "project environments (D8)": ("project.project_class.environments",),
    "environment functions (D8)": ("environment.environment_class.functions",),
    "not_missing: {} (P-HND-4)": ("spec.column_verifications.not_missing",),
    "rows[].derivations: {} (P-HND-4)": ("spec.row_class.derivations",),
    "baseline_flag operands (P-OP-2, P-OP-3)": (
        "spec.baseline_flag.value",
        "spec.baseline_flag.reference_date",
    ),
    "section standard (D16)": ("project.submission_section_class.standard",),
    "minimum_source_precision (P-IMP-1)": ("spec.impute.minimum_source_precision",),
    "{source: X} in wrappers (D18)": ("spec.expressions.source",),
    "aggregate derive (P-BIND-1)": ("spec.aggregate_class.derive",),
    "study_day.date (P-OP-1)": ("spec.study_day.date",),
    "datasets (P-PRJ-1)": ("project.submission_section_class.datasets",),
    "diff.bounds and cut.right (P-ORD-2)": ("spec.diff.bounds", "spec.cut.right"),
    "tie_break (P-ORD-1)": ("spec.baseline_flag.tie_break",),
}
COUNTERS = frozenset({DRIVER_KEYLESS, KEY_UNINFERABLE})


@dataclass
class Document:
    """One YAML file of a unit and what the unit decided for it."""

    path: Path
    text: str
    root: object
    kind: str
    plan: Plan = None
    output: str = None
    cleared: list = field(default_factory=list)
    comments_used: bool = False
    residue: int = 0
    version: str = "1.0"


@dataclass
class Result:
    """What a conversion writes, what it drops, what it counted and what it could not do."""

    files: dict = field(default_factory=dict)
    removed: list = field(default_factory=list)
    notes: list = field(default_factory=list)
    counts: Counter = field(default_factory=Counter)
    documents: dict = field(default_factory=lambda: defaultdict(set))
    stats: Counter = field(default_factory=Counter)

    def record(self, area, label, changes):
        """Count one document's changes under the footprint rows."""
        for row, entries in ROWS.items():
            amount = sum(changes.get(entry, 0) for entry in entries)
            if amount:
                self.counts[(area, row)] += amount
                self.documents[(area, row)].add(label)
        if any(value for name, value in changes.items() if name not in COUNTERS):
            self.documents[(area, "any change")].add(label)


def relative(path, base):
    return PurePosixPath(Path(os.path.relpath(path, base)).as_posix())


def classify(path, root):
    """The kind of a YAML file: spec, project, environment, converted,
    benchmark (metadata), contract (a negative's expected error) or data."""
    if not isinstance(root, yaml.MappingNode):
        return "data"
    names = {key.value for key, _ in root.value if isinstance(key, yaml.ScalarNode)}
    if "yamaa" in names and "schema_version" not in names:
        return "converted"
    if "schema_version" not in names:
        if path.name == "benchmark.yaml":
            return "benchmark"
        if path.name == "error.yaml" and path.parent.name == "expected":
            return "contract"
        return "data"
    if names & ENVIRONMENT_FIELDS and not names & PROJECT_FIELDS:
        return "environment"
    if names & PROJECT_FIELDS:
        return "project"
    return "spec"


class Unit:
    """One benchmark directory, project directory or loose file set."""

    def __init__(self, directory, files, base, result, table=None, explicit=False):
        self.directory = Path(directory).resolve() if directory else None
        self.files = [Path(item).resolve() for item in files]
        self.base = Path(base).resolve()
        self.result = result
        self.table = table or table_module.load()
        self.explicit = explicit
        self.cache = {}
        self.relations = keys_module.Relations(self.cache)
        self.documents = {}
        self.metadata = {}
        self.entries = []
        self.sections = {}
        self.projectless = set()

    def label(self, path):
        return str(relative(path, self.base))

    def note(self, path, line, kind, message):
        self.result.notes.append(
            Note(self.label(path) if path else "", line, kind, message)
        )

    def output(self, path, content):
        self.result.files[str(relative(path, self.base))] = content

    # -- run ---------------------------------------------------------------

    def run(self):
        for path in self.files:
            if path.suffix.lower() in SUFFIXES:
                self._load(path)
        self._read_metadata()
        self._find_entries()
        self._plan_models()
        self._plan_keys()
        self._plan_clears()
        functions = self._plan_functions()
        self._plan_codelists()
        # Specifications and projects first: a negative contract reads the
        # clear entries its entry specification's conversion wrote.
        for document in self.documents.values():
            if document.kind in ("spec", "project"):
                self._convert(document)
            elif document.kind == "codelists":
                self._codelists(document)
        for path in self.files:
            document = self.documents.get(path)
            if path.suffix.lower() == ".md":
                self._markdown(path)
            elif document is not None and document.kind in (
                "spec",
                "project",
                "codelists",
            ):
                continue
            elif document is None or document.kind in (
                "converted",
                "benchmark",
                "data",
            ):
                if document is not None and document.kind == "data" and self.explicit:
                    self._fragment_file(document)
                else:
                    self.output(path, path.read_bytes())
            elif document.kind == "environment":
                self._environment(document)
            elif document.kind == "contract":
                self._contract(document)
        for path, text in functions.items():
            self.output(path, text)
        self._review_comments()

    def _load(self, path):
        text = path.read_text(encoding="utf-8")
        try:
            root = compose(text)
        except yaml.YAMLError as error:
            self.note(
                path,
                0,
                "yaml",
                f"not parsed, copied unchanged: {str(error).splitlines()[0]}",
            )
            return
        kind = classify(path, root)
        self.documents[path] = Document(path, text, root, kind)
        if kind == "converted":
            self.result.stats["files already in the 1.0 surface"] += 1

    def _read_metadata(self):
        if self.directory is None:
            return
        path = (self.directory / "benchmark.yaml").resolve()
        document = self.documents.get(path)
        if document is None or not isinstance(document.root, yaml.MappingNode):
            return
        self.metadata = value_of(document.root) or {}
        for item in self.metadata.get("covers") or []:
            if item in RETIRED_COVERS:
                self.note(
                    path,
                    0,
                    "covers",
                    f"covers names `{item}`, an expression keyword 1.0 removes (D18); review the vocabulary coverage",
                )

    def _find_entries(self):
        """Benchmark entries (benchmark.yaml, else the one spec*.yaml) and
        every specification a project section lists."""
        entries = []
        if self.directory is not None:
            declared = self.metadata.get("entries")
            default = (self.directory / "project.yaml").is_file()
            if isinstance(declared, dict):
                for host in HOSTS:
                    for call in declared.get(host) or []:
                        if isinstance(call, dict) and isinstance(
                            call.get("specification"), str
                        ):
                            entry = (self.directory / call["specification"]).resolve()
                            entries.append(entry)
                            if not call.get("environment", default):
                                self.projectless.add(entry)
            else:
                specs = sorted(self.directory.glob("spec*.yaml"))
                if len(specs) == 1:
                    entries.append(specs[0].resolve())
                    if not default:
                        self.projectless.add(specs[0].resolve())
        for document in self.documents.values():
            if document.kind != "project":
                continue
            value = value_of(document.root) or {}
            for section, model in SECTION_MODELS.items():
                listed = (
                    value.get(section) if isinstance(value.get(section), dict) else {}
                )
                for spec in listed.get("specs") or []:
                    if isinstance(spec, str):
                        path = (document.path.parent / spec).resolve()
                        self.sections.setdefault(path, model)
                        entries.append(path)
        if self.explicit:
            # A specification given by itself is an entry, unless another
            # given file names it as its parent.
            parents = set()
            for document in self.documents.values():
                parent = (
                    member(document.root, "parent") if document.kind == "spec" else None
                )
                if isinstance(parent, yaml.ScalarNode):
                    parents.add((document.path.parent / parent.value).resolve())
            entries.extend(
                path
                for path, document in self.documents.items()
                if document.kind == "spec" and path not in parents
            )
        ordered = list(dict.fromkeys(entries))
        for entry in ordered:
            if entry not in self.documents:
                self.note(
                    entry,
                    0,
                    "entry",
                    "an entry specification this conversion cannot read",
                )
        self.entries = [
            entry
            for entry in ordered
            if entry in self.documents and self.documents[entry].kind == "spec"
        ]

    def _plan(self, document):
        if document.plan is None:
            names = {key.value for key, _ in document.root.value}
            document.plan = Plan(
                bundle=document.kind
                if document.kind in ("spec", "project")
                else "spec",
                layered="parent" in names and document.kind == "spec",
                header_type=document.kind,
                label=self.label(document.path),
            )
        return document.plan

    def _plan_models(self):
        standard = self.metadata.get("standard")
        for entry in self.entries:
            plan = self._plan(self.documents[entry])
            plan.model = self.sections.get(entry) or STANDARD_MODELS.get(standard)
            if plan.model:
                self.result.stats["entry specifications with a model (P-MODEL-1)"] += 1
                continue
            self.result.stats[
                "entry specifications without a model source (P-MODEL-1)"
            ] += 1
            source = (
                f"benchmark.yaml standard {standard}"
                if standard
                else "no section or benchmark standard"
            )
            self.note(
                entry,
                0,
                "model",
                f"no model source ({source}); P-MODEL-1 leaves `model` optional here",
            )

    # -- keys ----------------------------------------------------------------

    def _composed(self, entry, kind):
        try:
            return keys_module.composed(entry, self.cache)
        except keys_module.LayerError as error:
            self.note(entry, 0, kind, f"not composed: {error.describe(self.base)}")
        except (OSError, yaml.YAMLError) as error:
            self.note(entry, 0, kind, f"not composed: {type(error).__name__}")
        return None

    def _plan_keys(self):
        assigned = {}
        for entry in self.entries:
            spec = self._composed(entry, "keys")
            if spec is None:
                continue
            inference = keys_module.infer(spec, self.relations)
            self._record_inference(spec, inference)
            self._copy_candidates(spec)
            if entry in self.projectless and entry not in self.sections:
                bindings = sorted(_project_bindings(spec.value))
                if bindings:
                    self.note(
                        entry,
                        0,
                        "project",
                        f"runs without a project but binds {', '.join(bindings)}; D32 makes that fail validation (project_binding_missing), so run it with a project (P1-11)",
                    )
            for kind, members, target in (
                ("input", inference.input_keys, "input_keys"),
                ("intermediates", inference.intermediate_keys, "intermediate_keys"),
            ):
                for name, found in members.items():
                    layer = spec.declarers.get((kind, name), entry)
                    previous = assigned.setdefault((layer, kind, name), found)
                    if previous != found:
                        self.note(
                            layer,
                            0,
                            "keys",
                            f"{kind} {name} infers {previous} in one entry and {found} in another; declare it per entry by hand",
                        )
                        continue
                    document = self.documents.get(layer)
                    if document is None or document.kind != "spec":
                        self.note(
                            layer,
                            0,
                            "keys",
                            f"{kind} {name} needs key {found}, declared in a file outside this conversion",
                        )
                        continue
                    getattr(self._plan(document), target)[name] = found
            for kind, name, problem in inference.problems:
                layer = spec.declarers.get(
                    ("input" if kind == "input" else "intermediates", name), entry
                )
                self.note(
                    layer,
                    0,
                    "keys",
                    f"{kind} {name}: no inferable key ({problem}); declare `key` by hand (D23)",
                )

    def _record_inference(self, spec, inference):
        stats = self.result.stats
        if inference.input_keys or inference.intermediate_keys or inference.problems:
            stats["specifications with keys to write (D23)"] += 1
            if self.directory is not None and self.directory.name.startswith(
                "negative-"
            ):
                stats["negative specifications with keys to write (D23)"] += 1
        stats["input key declarations (P-JOIN-1)"] += len(inference.input_keys)
        stats["intermediate keys (D23)"] += len(inference.intermediate_keys)
        stats["intermediate row-template drivers left keyless (D23-R1)"] += len(
            inference.intermediate_drivers
        )
        joined = set(inference.joins) & set(inference.input_keys)
        drivers = keys_module.drivers_of(spec.value)
        stats["join pairs (P-JOIN-1)"] += len(joined)
        stats["join pairs on rows another template's driver built (NAME-4)"] += len(
            joined & drivers
        )
        aggregated = inference.aggregate_inputs & set(inference.input_keys)
        stats["inputs of keyless qualified aggregates (P-KEY-1)"] += len(aggregated)
        stats["inputs both joined and aggregated"] += len(joined & aggregated)
        for name, _ in inference.aggregate_keys:
            if name in inference.input_keys:
                stats["keyless qualified aggregates (P-KEY-1)"] += 1
                stats[f"aggregate key {inference.input_keys[name]}"] += 1
        for name in joined:
            label = "NAME-4 join key" if name in drivers else "join key"
            stats[f"{label} {inference.input_keys[name]}"] += 1
        for name in inference.intermediate_drivers:
            self._plan_change(spec, ("intermediates", name), DRIVER_KEYLESS)
        for kind, name, _ in inference.problems:
            if kind == "intermediate":
                self._plan_change(spec, ("intermediates", name), KEY_UNINFERABLE)

    def _plan_change(self, spec, member_, change):
        document = self.documents.get(spec.declarers.get(member_, spec.entry))
        if document is not None and document.kind == "spec":
            self._plan(document).changes[change] += 1

    def _copy_candidates(self, spec):
        """Columns a `copy` entry could replace (P-COPY-1), listed, never converted."""
        inputs = spec.value.get("input") or {}
        produced = {}
        for column in spec.value.get("columns") or []:
            if not isinstance(column, dict) or not isinstance(
                column.get("derivation"), str
            ):
                continue
            qualifier, _, name = column["derivation"].partition(".")
            dataset = inputs.get(qualifier)
            if (
                name != column.get("name")
                or not isinstance(dataset, dict)
                or "schema" not in dataset
            ):
                continue
            if qualifier not in produced:
                base = spec.path_bases.get((qualifier, "schema"), spec.entry.parent)
                try:
                    producer = keys_module.composed(
                        (base / dataset["schema"]).resolve(), self.cache
                    )
                except (keys_module.LayerError, OSError, yaml.YAMLError):
                    producer = None
                produced[qualifier] = {
                    item.get("name"): item
                    for item in (
                        producer.value.get("columns") or [] if producer else []
                    )
                    if isinstance(item, dict)
                }
            source = produced[qualifier].get(name)
            if (
                source
                and source.get("type") == column.get("type")
                and source.get("label") == column.get("label")
            ):
                self.result.stats["copy candidates (P-COPY-1)"] += 1
                self.note(
                    spec.entry,
                    0,
                    "copy",
                    f"column {name} copies {qualifier}.{name} with its producer's type and label; a `copy` entry could replace it (P-COPY-1, optional)",
                )

    # -- clears ---------------------------------------------------------------

    def _plan_clears(self):
        bundle = schema.bundle("spec")
        for document in self.documents.values():
            if document.kind != "spec" or not _has_nulls(document.root):
                continue
            parent = member(document.root, "parent")
            if not isinstance(parent, yaml.ScalarNode):
                continue
            plan = self._plan(document)
            try:
                layers = keys_module.chain(
                    (document.path.parent / parent.value).resolve(), self.cache
                )
            except keys_module.LayerError as error:
                self.note(
                    document.path,
                    0,
                    "clear",
                    f"parent not composed ({error.describe(self.base)}); its YAML nulls are deleted, not cleared",
                )
                continue
            except (OSError, yaml.YAMLError) as error:
                self.note(
                    document.path,
                    0,
                    "clear",
                    f"parent not composed ({type(error).__name__}); its YAML nulls are deleted, not cleared",
                )
                continue
            inherited, _, _ = keys_module.compose_layers(layers, document.path)
            child = value_of(document.root)
            for visit in bundle.walk(document.root, bundle.root):
                if visit.key is None or visit.kind not in ("field", "entry"):
                    continue
                if not plain(visit.value) or not is_null(visit.value):
                    continue
                if not _inherits(inherited, child, visit.path):
                    continue
                chain_ = []
                current = visit
                while current is not None:
                    chain_.append(current)
                    current = current.holder
                target = clear_path(self.table, "spec", list(reversed(chain_)))
                if target is None:
                    self.note(
                        document.path,
                        visit.key.start_mark.line + 1,
                        "clear",
                        f"the null at {_dotted(visit.path)} has no clear path (a list without identity lies on the way); write its clear entry by hand",
                    )
                    continue
                plan.clears.add(target)

    # -- D8 -----------------------------------------------------------------------

    def _plan_functions(self):
        """Plan each project's D8 merge; return the function files to write."""
        created = {}
        for document in list(self.documents.values()):
            environments = (
                member(document.root, "environments")
                if document.kind == "project"
                else None
            )
            if not isinstance(environments, yaml.MappingNode):
                continue
            definitions, locks, sources = {}, {}, {}
            complete = True
            for key, value in environments.value:
                host = key.value
                if not isinstance(value, yaml.ScalarNode):
                    self.note(
                        document.path,
                        key.start_mark.line + 1,
                        "D8",
                        f"environments.{host} is not a path; merge it by hand",
                    )
                    complete = False
                    continue
                path = (document.path.parent / value.value).resolve()
                environment = self.documents.get(path)
                if environment is None or environment.kind != "environment":
                    self.note(
                        document.path,
                        key.start_mark.line + 1,
                        "D8",
                        f"environment file {value.value} is not part of this conversion; merge it by hand",
                    )
                    complete = False
                    continue
                environment.plan = environment.plan or Plan(
                    bundle="environment", label=self.label(path)
                )
                sources[host] = environment
                lock = member(environment.root, "lock")
                if isinstance(lock, yaml.ScalarNode) and lock.value:
                    locks[host] = relative(
                        (path.parent / lock.value).resolve(), document.path.parent
                    ).as_posix()
                functions = member(environment.root, "functions")
                if isinstance(functions, yaml.MappingNode):
                    for name_key, definition in functions.value:
                        definitions.setdefault(name_key.value, {})[host] = definition
            if not complete:
                continue
            plan = self._plan(document)
            plan.functions = [f"functions/{name}.yaml" for name in definitions]
            plan.locks = locks
            plan.d8 = True
            for name, by_host in definitions.items():
                target = document.path.parent / "functions" / f"{name}.yaml"
                if target in self.documents:
                    self.note(
                        target,
                        0,
                        "D8",
                        f"{self.label(target)} already exists; merge function {name} by hand",
                    )
                    continue
                text = self._function_file(name, by_host, sources, target)
                if text is None:
                    continue
                if target in created and created[target] != text:
                    self.note(
                        target,
                        0,
                        "D8",
                        f"two projects define function {name} differently; {self.label(target)} keeps the first",
                    )
                elif target not in created:
                    created[target] = text
                    self.result.stats["function files written (D8)"] += 1
        return created

    def _plan_codelists(self):
        """Mark the codelist files a project names; each gains a header (P-TYPE-1)."""
        for document in list(self.documents.values()):
            if document.kind != "project":
                continue
            version = member(document.root, "schema_version")
            sources = member(document.root, "codelists")
            for item in sources.value if isinstance(sources, yaml.SequenceNode) else []:
                if not isinstance(item, yaml.ScalarNode):
                    continue
                path = (document.path.parent / item.value).resolve()
                listed = self.documents.get(path)
                if listed is None:
                    self.note(
                        document.path,
                        item.start_mark.line + 1,
                        "codelists",
                        f"codelist file {item.value} is not part of this conversion; give it a `type: codelists` header by hand",
                    )
                elif listed.kind == "data":
                    listed.kind = "codelists"
                    listed.plan = Plan(
                        bundle="project",
                        anchor="codelist_source_class",
                        header_type="codelists",
                        label=self.label(path),
                    )
                    listed.version = (
                        version.value if isinstance(version, yaml.ScalarNode) else "1.0"
                    )

    def _codelists(self, document):
        converter = Converter(document.plan, self.table, self.result.notes)
        body = converter.convert(document.text)
        root = compose(body)
        text = Text(body)
        first = (
            root.value[0][0]
            if isinstance(root, yaml.MappingNode) and root.value
            else None
        )
        at = text.line_start(text.line(start(first))) if first is not None else 0
        header = (
            f'yamaa: "{document.version}"{text.newline}type: codelists{text.newline}'
        )
        document.output = body[:at] + header + body[at:]
        converter.changes["new.codelists.yamaa"] += 1
        converter.changes["new.codelists.type"] += 1
        self.result.record("files", self.label(document.path), converter.changes)
        self.result.stats["codelist files given a header (P-TYPE-1)"] += 1
        self.output(document.path, document.output)

    def _function_file(self, name, by_host, sources, target):
        hosts = [host for host in HOSTS if host in by_host] + [
            host for host in by_host if host not in HOSTS
        ]
        chosen = hosts[0]
        definition = by_host[chosen]
        environment = sources[chosen]
        if not isinstance(definition, yaml.MappingNode) or definition.flow_style:
            self.note(
                environment.path,
                0,
                "D8",
                f"function {name} is not an inline block definition; write {self.label(target)} by hand",
            )
            return None
        implementations, values = {}, {}
        for host in hosts:
            value = (
                value_of(by_host[host])
                if isinstance(by_host[host], yaml.MappingNode)
                else None
            )
            if not isinstance(value, dict) or not isinstance(
                value.get("function"), str
            ):
                self.note(
                    sources[host].path,
                    0,
                    "D8",
                    f"function {name} names no callable for {host}; write its implementation by hand",
                )
                continue
            implementations[host] = value.pop("function")
            values[host] = value
        if len({repr(sorted(item.items(), key=str)) for item in values.values()}) > 1:
            self.note(
                environment.path,
                0,
                "D8",
                f"the definitions of {name} differ beyond `function`; {self.label(target)} takes the {chosen} one",
            )
        if len(implementations) < 2:
            self.note(
                environment.path,
                0,
                "D8",
                f"function {name} has one implementation ({', '.join(implementations) or 'none'})",
            )
        text = Text(environment.text)
        first = text.line(start(definition))
        last = text.line(end(definition, text.text))
        column = text.column(start(definition))
        lines = [text.line_text(line) for line in range(first, last + 1)]
        body = "".join(
            (line[column:] if not line[:column].strip() else line.lstrip()) + "\n"
            for line in lines
        )
        root = compose(body)
        inner = Text(body)
        edits = EditPlan()
        callable_ = member_pair(root, "function")
        edits.add(
            Edit(
                inner.line_start(inner.line(start(callable_[0]))),
                inner.next_line_start(
                    inner.line(pair_end(callable_[0], callable_[1], body))
                ),
                "",
            )
        )
        tests = member_pair(root, "tests")
        at = (
            inner.line_start(inner.line(start(tests[0])))
            if tests is not None
            else len(body)
        )
        listed = "".join(
            f"  {host}: {item}\n" for host, item in implementations.items()
        )
        edits.add(Edit(at, at, "implementations:\n" + listed))
        body = edits.apply(body)
        mine = set(
            comment_texts(
                environment.text[start(definition) : end(definition, environment.text)]
            )
        )
        carried = []
        for host in hosts[1:]:
            other = sources[host]
            node = by_host[host]
            carried.extend(
                item
                for item in comment_texts(
                    other.text[start(node) : end(node, other.text)]
                )
                if item not in mine and item not in carried
            )
        for host in hosts:
            carried.extend(
                item
                for item in self._comments_outside(sources[host])
                if item not in carried
            )
        version = member(environment.root, "schema_version")
        version = version.value if isinstance(version, yaml.ScalarNode) else "1.0"
        head = [f'yamaa: "{version}"', "type: function", *carried, f"name: {name}"]
        converter = Converter(
            Plan(
                bundle="environment",
                anchor="function_definition_class",
                label=self.label(target),
            ),
            self.table,
            self.result.notes,
        )
        converted = converter.convert("\n".join(head) + "\n" + body)
        converter.changes["new.function.implementations"] += 1
        # A new file: counted apart from the files the footprint compares.
        self.result.record("created", self.label(target), converter.changes)
        return converted

    def _comments_outside(self, environment):
        """An environment file's comments outside its definitions, carried once."""
        if environment.comments_used:
            return []
        environment.comments_used = True
        functions = member(environment.root, "functions")
        spans = []
        if isinstance(functions, yaml.MappingNode):
            spans = [
                (start(definition), end(definition, environment.text))
                for _, definition in functions.value
            ]
        return [
            environment.text[a:b].strip()
            for a, b in comments(environment.text, environment.root)
            if not any(x <= a < y for x, y in spans)
        ]

    def _environment(self, document):
        if document.plan is None:
            self.note(
                document.path,
                0,
                "D8",
                "no project in this conversion links this environment file; merge it into function files by hand",
            )
            self.output(document.path, document.path.read_bytes())
            return
        changes = Counter({"environment.environment_class.schema_version": 1})
        if member(document.root, "functions") is not None:
            changes["environment.environment_class.functions"] += 1
        self.result.record("files", self.label(document.path), changes)
        self.result.removed.append(self.label(document.path))
        self.result.stats["environment files removed (D8)"] += 1

    # -- conversion -------------------------------------------------------------

    def _convert(self, document):
        converter = Converter(self._plan(document), self.table, self.result.notes)
        document.output = converter.convert(document.text)
        document.cleared = converter.cleared
        document.residue = converter.changes["0.5 spellings left for manual work"]
        self.result.record("files", self.label(document.path), converter.changes)
        if document.output != document.text:
            self.result.stats["files converted"] += 1
        within = relative(document.path, self.directory or self.base).parts
        if within and within[0] == "input" and document.output != document.text:
            self.note(
                document.path,
                0,
                "input",
                "a schema producer under input/ was converted; AGENTS.md guards benchmarks/**/input/, so confirm this edit",
            )
        self.output(document.path, document.output)

    def _context(self):
        """The composed primary entry, the context README fragments are read in."""
        if not self.entries:
            return None
        try:
            spec = keys_module.composed(self.entries[-1], self.cache)
        except (keys_module.LayerError, OSError, yaml.YAMLError):
            return None
        return markdown.Context(spec, self.relations)

    def _fragment_file(self, document):
        converted, changes = markdown.convert_block(
            document.text,
            self._context(),
            self.table,
            self.result.notes,
            self.label(document.path),
            0,
        )
        if converted is not None and converted != document.text:
            self.result.record("files", self.label(document.path), changes)
            self.output(document.path, converted)
        else:
            self.output(document.path, document.path.read_bytes())

    def _markdown(self, path):
        text = path.read_text(encoding="utf-8")
        converted, blocks = markdown.convert_markdown(
            text, self._context(), self.table, self.result.notes, self.label(path)
        )
        for label, changes in blocks:
            self.result.record("markdown", label, changes)
        if blocks:
            self.result.stats["Markdown files with converted blocks"] += 1
        self.output(path, converted)

    # -- negative contracts -------------------------------------------------------

    def _contract(self, document):
        text = document.text
        entry = self.entries[-1] if self.entries else None
        spec = (
            keys_module.composed(entry, self.cache)
            if entry and self._composable(entry)
            else None
        )
        cleared = self.documents[entry].cleared if entry in self.documents else []
        edits = EditPlan()
        paths = member(document.root, "spec_paths")
        for item in paths.value if isinstance(paths, yaml.SequenceNode) else []:
            if not isinstance(item, yaml.ScalarNode):
                continue
            self.result.stats["negative spec paths"] += 1
            new = contracts.rewrite_path(
                item.value, spec.value if spec else {}, cleared, self.table
            )
            if new is None:
                self.note(
                    document.path,
                    item.start_mark.line + 1,
                    "path",
                    f"spec path {item.value} names a member this conversion cannot identify; rewrite it by hand",
                )
            elif new != item.value and edits.add(
                Edit(start(item), end(item, text), new)
            ):
                self.result.stats["negative spec paths rewritten"] += 1
        condition = member(document.root, "condition")
        entry_ = (
            self.table.condition(condition.value)
            if isinstance(condition, yaml.ScalarNode)
            else None
        )
        change = (
            contracts.condition_change(
                entry_, self.directory.name if self.directory else ""
            )
            if entry_
            else None
        )
        if (
            change
            and change[0] == "rename"
            and edits.add(Edit(start(condition), end(condition, text), change[1]))
        ):
            self.result.stats["negative conditions renamed"] += 1
        elif change and change[0] == "manual":
            self.note(
                document.path, condition.start_mark.line + 1, "negative", change[1]
            )
        if entry in self.documents and self.documents[entry].residue:
            self.note(
                document.path,
                0,
                "negative",
                "the entry specification keeps a 0.5 spelling the table cannot convert, so this fault changes; re-author expected/error.yaml independently (D60)",
            )
        output = edits.apply(text)
        if output != text:
            self.result.stats["negative contracts rewritten"] += 1
        self.output(document.path, output)

    def _composable(self, entry):
        try:
            keys_module.composed(entry, self.cache)
        except (keys_module.LayerError, OSError, yaml.YAMLError):
            return False
        return True

    # -- comments -----------------------------------------------------------------

    def _review_comments(self):
        for document in self.documents.values():
            if document.output is None or document.output == document.text:
                continue
            for number, line in enumerate(LINE_BREAK.split(document.output), start=1):
                comment = _comment_of(line)
                if comment and COMMENT_SPELLINGS.search(comment):
                    self.note(
                        document.path,
                        number,
                        "comment",
                        f"comment names a converted construct; review its text: {comment}",
                    )


def _project_bindings(value):
    """The codelists and functions a composed specification takes from a project (D32)."""
    found = set()
    stack = [value]
    while stack:
        node = stack.pop()
        if isinstance(node, dict):
            if isinstance(node.get("codelist"), str):
                found.add(f"codelist {node['codelist']}")
            if node.get("inventory_vocabulary") is True:
                found.add("an inventory vocabulary")
            call = node.get("function")
            if isinstance(call, dict) and isinstance(call.get("name"), str):
                found.add(f"function {call['name']}")
            stack.extend(node.values())
        elif isinstance(node, list):
            stack.extend(node)
    return found


def _has_nulls(root):
    stack = [root]
    while stack:
        node = stack.pop()
        if isinstance(node, yaml.MappingNode):
            for _, value in node.value:
                if plain(value) and is_null(value):
                    return True
                stack.append(value)
        elif isinstance(node, yaml.SequenceNode):
            stack.extend(node.value)
    return False


def _comment_of(line):
    index = line.find("#")
    while index >= 0:
        if index == 0 or line[index - 1] in " \t":
            before = line[:index]
            if before.count('"') % 2 == 0 and before.count("'") % 2 == 0:
                return line[index:].strip()
        index = line.find("#", index + 1)
    return None


def _dotted(path):
    out = ""
    for segment in path:
        out += (
            f"[{segment}]"
            if isinstance(segment, int)
            else (f".{segment}" if out else str(segment))
        )
    return out


def _inherits(parent, child, path):
    """True when the composed parent value holds a value at the child's `path`."""
    inherited, written = parent, child
    for position, segment in enumerate(path):
        if isinstance(segment, int):
            identity = IDENTITY.get(path[position - 1]) if position == 1 else None
            if (
                identity is None
                or not isinstance(written, list)
                or not isinstance(inherited, list)
            ):
                return False
            item = written[segment] if segment < len(written) else None
            name = item.get(identity) if isinstance(item, dict) else None
            inherited = next(
                (
                    candidate
                    for candidate in inherited
                    if isinstance(candidate, dict) and candidate.get(identity) == name
                ),
                None,
            )
            written = item
        else:
            if position == 2 and path[0] == "input" and isinstance(inherited, str):
                inherited = {"path": inherited}
            if not isinstance(inherited, dict) or segment not in inherited:
                return False
            inherited = inherited[segment]
            written = written.get(segment) if isinstance(written, dict) else None
        if inherited is None:
            return False
    return True


def check_table(table):
    """Refuse a table the converter does not implement exactly."""
    table.check_implemented(TRANSFORMS, contracts.RULES)
    named = {
        entry for entries in ROWS.values() for entry in entries if entry not in COUNTERS
    }
    for entry_id in sorted(
        named
        | set(DEFAULT_MISSING)
        | {
            "spec.intermediate_class.no_match",
            "spec.study_day.date",
            "spec.baseline_flag.reference_date",
            "spec.baseline_flag.value",
            "spec.diff.bounds",
            "project.submission_section_class.datasets",
            "new.spec.model",
            "new.spec.clear",
            "new.spec.input.key",
            "new.any.suppression_comment",
        }
    ):
        table.get(entry_id)
