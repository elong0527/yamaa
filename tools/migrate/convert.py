"""Convert one YAML document's text from the 0.5 surface to 1.0.

A document is converted in three passes, each repeated until it plans no
more edits; the text is composed again before every round, so marks are
always fresh:

1. removals: a YAML null in a child layer becomes an entry of a root `clear`
   list (P-CLR-1), and one in a single-layer specification, which today
   leaves its field absent, is deleted (P-NULL-1);
2. rewrites: every rename table entry and transform that applies to a node
   the schema walker attributes to it, innermost first, so a structural
   transform (a list wrapped, a binding list made a map) copies text whose
   expressions are already converted;
3. insertions: the header, `model`, declared join keys and the D8 project
   fields.

Every edit replaces an exact range of the text; nothing is re-serialized,
so comments and layout outside the ranges stay as written, and a comment
inside a replaced range moves to its own line.
"""

import re
from collections import Counter
from dataclasses import dataclass, field

import yaml
from tools.migrate import schema
from tools.migrate import table as table_module
from tools.migrate.text import (
    LINE_WIDTH,
    Edit,
    EditPlan,
    Text,
    comments,
    compose,
    core_type,
    double_quoted,
    end,
    is_null,
    member,
    member_pair,
    pair_end,
    path_segment,
    plain,
    start,
)

MAX_ROUNDS = 60

# Handlers whose omission already gives the missing value: there `{literal:
# null}` restates the default and is deleted in a single-layer
# specification (P-HND-3, the handler table of the RFC's spelling page).
# Every other substitute-value field defaults to failing, so its null stays.
DEFAULT_MISSING = (
    "spec.intermediate_class.no_match",
    "spec.source_binding_class.absent",
    "spec.first_available.missing",
    "spec.mapping.missing",
    "spec.cut.missing",
    "spec.str_extract.missing",
    "spec.str_replace.missing",
    "spec.str_case.missing",
    # Missing by default, but required beside `false_value`: a restatement
    # only when the flag declares no `false_value`.
    "spec.flag_class.missing",
)

# Keyed members are addressed by identity in a clear path (P-CLR-1).
IDENTITY = {"columns": "name", "intermediates": "id", "rows": "id"}

_SUPPRESSION = re.compile(r"#(\s*)yamaa-style:")
NULL_DELETED = "YAML nulls deleted (P-NULL-1)"


@dataclass
class Note:
    """One item of the manual follow-up list."""

    file: str
    line: int
    kind: str
    message: str

    def render(self):
        where = f"{self.file}:{self.line}" if self.line else self.file
        return f"{where}: [{self.kind}] {self.message}"


@dataclass
class Plan:
    """What the unit decided for one document before its text is edited."""

    bundle: str = "spec"
    anchor: object = None
    layered: bool = False
    header_type: str = "spec"
    model: str = None
    clears: set = field(default_factory=set)
    intermediate_keys: dict = field(default_factory=dict)
    input_keys: dict = field(default_factory=dict)
    functions: list = field(default_factory=list)
    locks: dict = field(default_factory=dict)
    d8: bool = False
    label: str = ""
    line_offset: int = 0
    changes: Counter = field(default_factory=Counter)


class Converter:
    """Applies the rename table to one document under its plan."""

    def __init__(self, plan, table=None, notes=None):
        self.table = table or table_module.load()
        self.plan = plan
        self.bundle = schema.bundle(plan.bundle)
        self.anchor = plan.anchor or self.bundle.root
        self.notes = notes if notes is not None else []
        self.changes = Counter(plan.changes)
        self.markers = 0
        self.cleared = []
        self._noted = set()

    # -- driver ------------------------------------------------------------

    def convert(self, text):
        for stage in (self._removals, self._rewrites, self._insertions):
            for _ in range(MAX_ROUNDS):
                self.text = Text(text)
                try:
                    self.root = compose(text)
                except yaml.YAMLError as error:
                    message = str(error).splitlines()[0]
                    self.note(-1, "yaml", f"not parsed, left as written: {message}")
                    return text
                self.edits = EditPlan()
                self._comments = None
                if self.root is not None:
                    stage()
                if not self.edits:
                    break
                changed = self.edits.apply(text)
                if changed == text:
                    break
                text = changed
            else:
                self.note(
                    -1, "internal", "the conversion did not settle; check the result"
                )
        self._residue(text)
        return text

    def _residue(self, text):
        """Note every 0.5 spelling the table changes that the passes left in place."""
        self.text = Text(text)
        try:
            self.root = compose(text)
        except yaml.YAMLError:
            self.note(
                -1, "internal", "the converted text does not parse; check the result"
            )
            return
        if self.root is None:
            return
        for visit in self.visits():
            entry = self.entry_of(visit)
            convert = (entry or {}).get("convert") or {}
            action = convert.get("action")
            transform = convert.get("transform")
            if action in ("rename_key", "delete_key"):
                pass
            elif action != "transform" or transform not in RESIDUE:
                continue
            elif transform == "source_wrapper_to_text" and (
                visit.kind != "keyword" or core_type(visit.value) != "str"
            ):
                continue
            elif transform == "compute_to_text" and visit.kind != "keyword":
                continue
            elif transform == "derive_to_derivations" and visit.name != "derive":
                continue
            elif (
                transform == "cut_right_to_closed" and core_type(visit.value) != "bool"
            ):
                continue
            line = self.line_of(visit.key) if visit.key is not None else -1
            self.note(
                line,
                "residue",
                f"`{visit.name}` ({entry['id']}) is left in its 0.5 spelling; convert it by hand",
            )
            self.count(entry["id"], marker=False)
            self.count("0.5 spellings left for manual work", marker=False)

    def note(self, line, kind, message):
        key = (line, kind, message)
        if key in self._noted:
            return
        self._noted.add(key)
        number = line + self.plan.line_offset + 1 if line >= 0 else 0
        self.notes.append(Note(self.plan.label, number, kind, message))

    def count(self, name, amount=1, marker=True):
        self.changes[name] += amount
        if marker:
            self.markers += amount

    def visits(self):
        return list(self.bundle.walk(self.root, self.anchor))

    def comment_ranges(self):
        if self._comments is None:
            self._comments = comments(self.text.text, self.root)
        return self._comments

    def comments_in(self, a, b):
        return [(x, y) for x, y in self.comment_ranges() if a <= x < b]

    def line_of(self, node):
        return self.text.line(start(node)) if node is not None else -1

    def entry_of(self, visit):
        location = visit.location() if visit is not None else None
        return self.table.entry(self.plan.bundle, location) if location else None

    # -- pass 1: YAML nulls --------------------------------------------------

    def _removals(self):
        """Remove one YAML null per round, so a removal sees its container as
        the earlier removals left it (an entry emptied by two nulls drops)."""
        if self.plan.bundle != "spec":
            return
        visits = self.visits()
        parents = {id(visit.value): visit for visit in visits}
        for visit in visits:
            if visit.key is None or visit.kind not in ("field", "entry"):
                continue
            if not plain(visit.value) or not is_null(visit.value):
                continue
            target = None
            if self.plan.layered and self.plan.clears:
                chain = []
                current = visit
                while current is not None:
                    chain.append(current)
                    current = current.holder
                target = clear_path(self.table, self.plan.bundle, list(reversed(chain)))
                target = target if target in self.plan.clears else None
            removal = self._removal_root(visit, parents)
            edits, moved = self._remove(removal, keep_comments=target is None)
            if edits is None:
                where = target or ".".join(str(item) for item in visit.path)
                self.note(
                    self.line_of(visit.key),
                    "null",
                    f"the YAML null at {where} could not be removed; rewrite it by hand",
                )
                continue
            if not self.edits.add(*edits):
                continue
            entry = self.entry_of(visit)
            if target is None:
                # A null deleted under a spelling 1.0 renames or removes marks
                # its document as 0.5; one under an unchanged name does not.
                convert = (entry or {}).get("convert") or {}
                marker = convert.get("action") in ("rename_key", "delete_key") or bool(
                    convert.get("new_name")
                )
                self.count(NULL_DELETED, marker=marker)
                if entry is not None:
                    self.count(entry["id"], marker=False)
                return
            self.count("new.spec.clear")
            if entry is not None:
                self.count(entry["id"])
            self.cleared.append(target)
            for _, comment in moved:
                self.note(
                    self.line_of(visit.key),
                    "comment",
                    f"comment moved to the clear entry {target}; reword it: {comment}",
                )
            self._write_clear([*moved, ("entry", target)])
            return

    def _removal_root(self, visit, parents):
        """Climb from a null pair while removing it would empty its container.

        A keyed member left with only its identity field is empty too (an
        emptied column entry is dropped, then an emptied `columns` list)."""
        current = visit
        while True:
            container = current.parent
            holder = parents.get(id(container))
            if holder is None:
                return current
            if isinstance(container, yaml.MappingNode):
                remaining = [
                    k.value for k, _ in container.value if k is not current.key
                ]
                identity = None
                if (
                    holder.kind == "item"
                    and holder.holder is not None
                    and len(holder.holder.path) == 1
                ):
                    identity = IDENTITY.get(holder.holder.name)
                if remaining and remaining != [identity]:
                    return current
            elif isinstance(container, yaml.SequenceNode):
                if len(container.value) > 1:
                    return current
            else:
                return current
            current = holder

    def _remove(self, visit, keep_comments):
        """Edits deleting a visit's pair or list item, and the comments it carried.

        With `keep_comments` a comment in the deleted lines stays as a line of
        its own; without, the comments, those written directly above
        included, are returned to move with a clear entry."""
        text = self.text
        container = visit.parent
        if visit.kind == "item":
            if container.flow_style:
                return self._flow_remove_item(container, visit.value)
            node = visit.value
            a = text.line_start(text.line(start(node)))
            b = text.next_line_start(text.line(end(node, text.text)))
        else:
            if container.flow_style:
                return self._flow_remove_pair(container, visit.key, visit.value)
            first = text.line(start(visit.key))
            if text.text[text.line_start(first) : start(visit.key)].strip():
                return self._remove_dash_pair(container, visit.key)
            a = text.line_start(first)
            last = text.line(pair_end(visit.key, visit.value, text.text))
            b = text.next_line_start(last)
        lead = a
        if not keep_comments:
            line = text.line(a)
            while line > 0 and text.line_text(line - 1).lstrip().startswith("#"):
                line -= 1
            lead = text.line_start(line)
        found = [text.text[x:y].strip() for x, y in self.comments_in(lead, b)]
        if keep_comments:
            if not found:
                return self._collapse_blank(a, b), []
            indent = " " * text.indent(text.line(a))
            replacement = "".join(indent + item + text.newline for item in found)
            return [Edit(a, b, replacement)], []
        return self._collapse_blank(lead, b), [("comment", item) for item in found]

    def _collapse_blank(self, a, b):
        """Delete [a, b), with one of the two blank lines it would leave adjacent."""
        text = self.text
        before = text.line(a) - 1
        if before >= 0 and not text.line_text(before).strip():
            if b >= len(text.text):
                # Removing the last field leaves no blank line at the end.
                return [Edit(text.line_start(before), b, "")]
            after = text.line(b)
            if not text.line_text(after).strip():
                return [Edit(a, text.next_line_start(after), "")]
        return [Edit(a, b, "")]

    def _remove_dash_pair(self, mapping, key):
        pairs = mapping.value
        index = next(i for i, (k, _) in enumerate(pairs) if k is key)
        if index != 0 or len(pairs) < 2:
            return None, []
        a, b = start(key), start(pairs[1][0])
        if self.comments_in(a, b):
            return None, []
        return [Edit(a, b, "")], []

    def _flow_remove_pair(self, mapping, key, value):
        pairs = mapping.value
        index = next(i for i, (k, _) in enumerate(pairs) if k is key)
        if len(pairs) == 1:
            return [Edit(start(mapping), end(mapping, self.text.text), "{}")], []
        if index > 0:
            previous = pairs[index - 1]
            a = pair_end(previous[0], previous[1], self.text.text)
            b = pair_end(key, value, self.text.text)
        else:
            a, b = start(key), start(pairs[1][0])
        if self.comments_in(a, b):
            return None, []
        return [Edit(a, b, "")], []

    def _flow_remove_item(self, sequence, node):
        items = sequence.value
        index = next(i for i, item in enumerate(items) if item is node)
        if len(items) == 1:
            return [Edit(start(sequence), end(sequence, self.text.text), "[]")], []
        if index > 0:
            a = end(items[index - 1], self.text.text)
            b = end(node, self.text.text)
        else:
            a, b = start(node), start(items[1])
        if self.comments_in(a, b):
            return None, []
        return [Edit(a, b, "")], []

    def _write_clear(self, lines):
        """Write clear entries into the root `clear` list, after `parent` (P-CLR-1)."""
        text = self.text
        style = self.sequence_style()
        indent = " " * style
        body = []
        for kind, value in lines:
            body.append(
                f"{indent}{value}" if kind == "comment" else f"{indent}- {value}"
            )
        existing = member_pair(self.root, "clear")
        if existing is not None:
            at = text.next_line_start(
                text.line(pair_end(existing[0], existing[1], text.text))
            )
            content = text.newline.join(body) + text.newline
        else:
            pair = member_pair(self.root, "parent") or member_pair(
                self.root, "schema_version"
            )
            pair = pair or self.root.value[0]
            at = text.next_line_start(text.line(pair_end(pair[0], pair[1], text.text)))
            content = text.newline.join(["clear:", *body]) + text.newline
        if at == len(text.text) and not text.text.endswith(("\n", "\r")):
            content = text.newline + content
        self.edits.add(Edit(at, at, content))

    # -- pass 2: rewrites ------------------------------------------------------

    def _rewrites(self):
        for visit in reversed(self.visits()):
            entry = self.entry_of(visit)
            if entry is None:
                continue
            convert = entry.get("convert") or {}
            action = convert.get("action")
            if action == "rename_key":
                self._rename(visit, entry, convert["new_name"])
            elif action == "delete_key":
                edits, _ = self._remove(visit, keep_comments=True)
                if edits and self.edits.add(*edits):
                    self.count(entry["id"])
            elif action == "transform":
                transform = TRANSFORMS[convert["transform"]]
                if transform.stage == "rewrite":
                    transform.function(self, visit, entry)
        self._suppression_comments()

    def _rename(self, visit, entry, new_name):
        edits = []
        if visit.key.value != new_name:
            edits.append(
                Edit(start(visit.key), end(visit.key, self.text.text), new_name)
            )
        value = visit.value
        if isinstance(value, yaml.ScalarNode):
            renamed = self.table.value_entry(
                self.plan.bundle, visit.location(), value.value
            )
            convert = (renamed or {}).get("convert") or {}
            new_value = convert.get("new_value")
            if (
                convert.get("action") == "rename_value"
                and new_value
                and new_value != value.value
            ):
                edits.append(Edit(start(value), end(value, self.text.text), new_value))
        if edits and self.edits.add(*edits):
            self.count(entry["id"])

    def _suppression_comments(self):
        entry = self.table.get("new.any.suppression_comment")
        for a, b in self.comment_ranges():
            match = _SUPPRESSION.match(self.text.text, a)
            if match and self.edits.add(
                Edit(a, match.end(), f"#{match.group(1)}yamaa-lint:")
            ):
                self.count(entry["id"])

    # -- pass 3: insertions ----------------------------------------------------

    def _insertions(self):
        for visit in self.visits():
            entry = self.entry_of(visit)
            convert = (entry or {}).get("convert") or {}
            if convert.get("action") == "transform":
                transform = TRANSFORMS[convert["transform"]]
                if transform.stage == "insert":
                    transform.function(self, visit, entry)
        if self.plan.bundle == "spec":
            TRANSFORMS["write_inferred_intermediate_key"].function(
                self, None, self.table.get("spec.intermediate_class.key")
            )
            TRANSFORMS["declare_input_key"].function(
                self, None, self.table.get("new.spec.input.key")
            )

    # -- text helpers ------------------------------------------------------------

    def sequence_style(self):
        """The dash offset the file's block sequences take under their key (0 or 2)."""
        offsets = Counter()
        text = self.text
        stack = [self.root]
        while stack:
            node = stack.pop()
            if isinstance(node, yaml.MappingNode):
                for key, value in node.value:
                    if (
                        isinstance(value, yaml.SequenceNode)
                        and not value.flow_style
                        and value.value
                    ):
                        dash_line = text.line(start(value.value[0]))
                        if dash_line != text.line(start(key)):
                            column = text.line_text(dash_line).find("-")
                            offsets[column - text.column(start(key))] += 1
                    stack.append(value)
            elif isinstance(node, yaml.SequenceNode):
                stack.extend(node.value)
        return offsets.most_common(1)[0][0] if offsets else 2

    def replace_value(self, key, value, new_text):
        """Edits writing `new_text` as the inline value of `key`.

        A value that starts on a later line than its key is replaced from the
        colon on. A comment on the key's own line stays at the end of it; one
        on a later line of the replaced range moves above the key."""
        text = self.text
        a, b = start(value), end(value, text.text)
        key_line = text.line(start(key)) if key is not None else text.line(a)
        if key is not None and text.line(a) != key_line:
            a = text.text.find(":", end(key, text.text)) + 1
            new_text = " " + new_text
        found = self.comments_in(a, b)
        trailing = [
            text.text[x:y].strip() for x, y in found if text.line(x) == key_line
        ]
        above = [text.text[x:y].strip() for x, y in found if text.line(x) != key_line]
        if trailing:
            new_text += "  " + "  ".join(trailing)
        edits = [Edit(a, b, new_text)]
        if above:
            indent = " " * text.indent(key_line)
            at = text.line_start(key_line)
            edits.insert(
                0, Edit(at, at, "".join(indent + item + text.newline for item in above))
            )
            for item in above:
                self.note(key_line, "comment", f"comment moved above its line: {item}")
        return edits

    def replace_wrapper(self, visit, new_text):
        """Edits replacing the mapping that holds `visit` with `new_text`."""
        holder = visit.holder
        wrapper = visit.parent
        if holder is not None and holder.value is wrapper:
            return self.replace_value(holder.key, wrapper, new_text)
        return [Edit(start(wrapper), end(wrapper, self.text.text), new_text)]

    def insert_pair(self, mapping, key, value_text, after=None, before=None):
        """Edits inserting `key: value_text` into a mapping, or None."""
        text = self.text
        if mapping.flow_style:
            return self._flow_insert(mapping, key, value_text, after)
        if not mapping.value:
            return None
        column = text.column(start(mapping.value[0][0]))
        line_text = f"{' ' * column}{key}: {value_text}"
        if before is not None:
            line = text.line(start(before))
            if text.text[text.line_start(line) : start(before)].strip():
                return None
            while (
                line > 0
                and text.line_text(line - 1).lstrip().startswith("#")
                and text.indent(line - 1) == column
            ):
                line -= 1
            at = text.line_start(line)
            return [Edit(at, at, line_text + text.newline)]
        if after is None:
            after = mapping.value[-1][0]
        value = next(v for k, v in mapping.value if k is after)
        at = text.next_line_start(text.line(pair_end(after, value, text.text)))
        body = line_text + text.newline
        if at == len(text.text) and not text.text.endswith(("\n", "\r")):
            body = text.newline + line_text
        return [Edit(at, at, body)]

    def _flow_insert(self, mapping, key, value_text, after=None):
        text = self.text
        pairs = mapping.value
        if not pairs:
            return [
                Edit(
                    start(mapping), end(mapping, text.text), f"{{{key}: {value_text}}}"
                )
            ]
        anchor_key, anchor_value = pairs[-1]
        if after is not None:
            anchor_key, anchor_value = next((k, v) for k, v in pairs if k is after)
        at = pair_end(anchor_key, anchor_value, text.text)
        piece = f", {key}: {value_text}"
        line = text.line(at)
        if line == text.line(end(mapping, text.text)) == text.line(start(mapping)):
            if len(text.line_text(line)) + len(piece) > LINE_WIDTH:
                column = text.column(start(mapping)) + 1
                piece = f",{text.newline}{' ' * column}{key}: {value_text}"
        return [Edit(at, at, piece)]

    def source(self, node):
        return self.text.text[start(node) : end(node, self.text.text)]


@dataclass(frozen=True)
class Transform:
    stage: str
    function: object


def flow_list(items):
    return "[" + ", ".join(items) + "]"


# -- transforms named by the table --------------------------------------------


def _literal_text(node):
    """The 1.0 expression text of a `literal` payload (D18)."""
    kind = core_type(node)
    if kind == "null":
        return "null"
    if kind in ("bool", "int", "float"):
        return node.value
    return double_quoted("'" + node.value.replace("'", "''") + "'")


def literal_to_expression(converter, visit, entry):
    payload = visit.value
    if not isinstance(payload, yaml.ScalarNode):
        return
    if converter.edits.add(*converter.replace_wrapper(visit, _literal_text(payload))):
        converter.count(entry["id"])


def compute_to_text(converter, visit, entry):
    if visit.kind != "keyword":
        return
    expr = member(visit.value, "expr")
    if not isinstance(expr, yaml.ScalarNode) or len(visit.value.value) != 1:
        return
    if converter.edits.add(*converter.replace_wrapper(visit, converter.source(expr))):
        converter.count(entry["id"])


def source_wrapper_to_text(converter, visit, entry):
    # The handled wrapper's `value` field names this transform too; its
    # `{source: X}` is the keyword visit inside it, converted here.
    if visit.kind != "keyword" or not isinstance(visit.value, yaml.ScalarNode):
        return
    if core_type(visit.value) != "str":
        return
    if converter.edits.add(
        *converter.replace_wrapper(visit, converter.source(visit.value))
    ):
        converter.count(entry["id"])


def null_handler(converter, visit, entry):
    if not isinstance(visit.value, yaml.ScalarNode) or not is_null(visit.value):
        return
    handler = visit.holder
    handler_entry = None
    if (
        handler is not None
        and handler.kind == "field"
        and handler.value is visit.parent
    ):
        handler_entry = converter.entry_of(handler)
    restates = (
        handler_entry is not None
        and handler_entry["id"] in DEFAULT_MISSING
        and not converter.plan.layered
    )
    if restates and handler_entry["id"] == "spec.flag_class.missing":
        restates = member(handler.parent, "false_value") is None
    if restates:
        edits, _ = converter._remove(handler, keep_comments=True)
        if edits and converter.edits.add(*edits):
            converter.count(entry["id"])
            converter.count(handler_entry["id"])
            converter.count("restated null handlers deleted (P-HND-3)")
        return
    if converter.edits.add(*converter.replace_wrapper(visit, "null")):
        converter.count(entry["id"])


def tagged_literal_to_text(converter, visit, entry):
    payload = visit.value
    if not isinstance(payload, yaml.ScalarNode) or len(visit.parent.value) != 1:
        return
    if visit.name == "literal":
        new_text = _literal_text(payload)
    elif visit.name == "date":
        new_text = double_quoted(f"DATE '{payload.value}'")
    else:
        new_text = double_quoted(f"DATETIME '{payload.value}'")
    if converter.edits.add(*converter.replace_wrapper(visit, new_text)):
        converter.count(entry["id"])


def untag_function_value(converter, visit, entry):
    payload = visit.value
    if not isinstance(payload, yaml.ScalarNode) or len(visit.parent.value) != 1:
        return
    if converter.edits.add(
        *converter.replace_wrapper(visit, converter.source(payload))
    ):
        converter.count(entry["id"])


def unwrap_value(converter, visit, entry):
    if len(visit.parent.value) != 1:
        return
    inner = visit.value
    if (
        isinstance(inner, (yaml.MappingNode, yaml.SequenceNode))
        and not inner.flow_style
    ):
        converter.note(
            converter.line_of(visit.key),
            "unwrap",
            "an intermediate `{value: E}` with a block E; unwrap it by hand",
        )
        return
    if converter.edits.add(*converter.replace_wrapper(visit, converter.source(inner))):
        converter.count(entry["id"])


def _wrapped(converter, visit, new_name):
    """Edits renaming a field and writing its one mapping as a one-entry list."""
    text = converter.text
    key, value = visit.key, visit.value
    if isinstance(value, yaml.SequenceNode):
        return None
    edits = [Edit(start(key), end(key, text.text), new_name)]
    if isinstance(value, yaml.MappingNode) and not value.flow_style and value.value:
        first = text.line(start(value))
        last = text.line(end(value, text.text))
        child = text.column(start(value))
        dash = text.column(start(key)) + converter.sequence_style()
        shift = dash + 2 - child
        out = []
        for line in range(first, last + 1):
            raw = text.line_text(line)
            if line == first:
                out.append(" " * dash + "- " + raw[child:])
            elif not raw.strip():
                out.append(raw)
            elif shift >= 0:
                out.append(" " * shift + raw)
            else:
                out.append(raw[-shift:])
        edits.append(
            Edit(text.line_start(first), text.line_end(last), text.newline.join(out))
        )
        return edits
    edits.append(
        Edit(start(value), end(value, text.text), "[" + converter.source(value) + "]")
    )
    return edits


def wrap_in_list(converter, visit, entry):
    edits = _wrapped(converter, visit, entry["convert"]["new_name"])
    if edits and converter.edits.add(*edits):
        converter.count(entry["id"])


def origin_to_list(converter, visit, entry):
    edits = _wrapped(converter, visit, entry["convert"]["new_name"])
    if edits and converter.edits.add(*edits):
        converter.count(entry["id"])


def derive_to_derivations(converter, visit, entry):
    # The binding's `name` field names this transform too; the list is
    # rewritten whole from the `derive` field.
    if visit.owner != "aggregate_class" or visit.name != "derive":
        return
    text = converter.text
    sequence = visit.value
    if not isinstance(sequence, yaml.SequenceNode):
        return
    new_name = str(entry.get("proposed", "derivations")).split(":", 1)[0]
    items = []
    for item in sequence.value:
        name = member(item, "name")
        if not isinstance(item, yaml.MappingNode) or not isinstance(
            name, yaml.ScalarNode
        ):
            converter.note(
                converter.line_of(visit.key),
                "derive",
                "a derive binding without a name; rewrite it as derivations by hand",
            )
            return
        items.append((item, name))

    def flow_item(item, name):
        rest = [
            text.text[start(k) : pair_end(k, v, text.text)]
            for k, v in item.value
            if k.value != "name"
        ]
        return f"{name.value}: {{{', '.join(rest)}}}"

    edits = [Edit(start(visit.key), end(visit.key, text.text), new_name)]
    if sequence.flow_style or not items:
        if any(not item.flow_style for item, _ in items):
            return
        edits.append(
            Edit(
                start(sequence),
                end(sequence, text.text),
                "{" + ", ".join(flow_item(*pair) for pair in items) + "}",
            )
        )
    else:
        name_column = text.column(start(visit.key)) + 2
        out = []
        first_line = text.line(start(sequence))
        last_line = text.line(end(sequence, text.text))
        owners = {}
        for item, name in items:
            for line in range(
                text.line(start(item)), text.line(end(item, text.text)) + 1
            ):
                owners[line] = (item, name)
        written = set()
        for line in range(first_line, last_line + 1):
            raw = text.line_text(line)
            owner = owners.get(line)
            if owner is None:
                # A comment or blank line between bindings stays, at the
                # bindings' new column.
                out.append(" " * name_column + raw.strip() if raw.strip() else raw)
                continue
            item, name = owner
            if item.flow_style:
                if id(item) not in written:
                    out.append(" " * name_column + flow_item(item, name))
                    written.add(id(item))
                continue
            field_column = text.column(start(item))
            shift = name_column + 2 - field_column
            name_key = member_pair(item, "name")[0]
            name_lines = range(
                text.line(start(name_key)),
                text.line(pair_end(name_key, name, text.text)) + 1,
            )
            if id(item) not in written:
                trailing = [
                    text.text[x:y].strip()
                    for x, y in converter.comments_in(
                        text.line_start(name_lines[0]), text.line_end(name_lines[-1])
                    )
                ]
                out.append(
                    f"{' ' * name_column}{name.value}:"
                    + "".join(f"  {item_}" for item_ in trailing)
                )
                written.add(id(item))
            if line in name_lines:
                continue
            if line == text.line(start(item)):
                raw = " " * field_column + raw[field_column:]
            if not raw.strip():
                out.append(raw)
            elif shift >= 0:
                out.append(" " * shift + raw)
            else:
                out.append(raw[-shift:])
        a = text.line_start(text.line(start(sequence)))
        b = text.line_end(text.line(end(sequence, text.text)))
        edits.append(Edit(a, b, text.newline.join(out)))
    if converter.edits.add(*edits):
        converter.count(entry["id"])
        converter.count("aggregate derive bindings (P-BIND-1)", len(items))


def drop_empty_row_derivations(converter, visit, entry):
    value = visit.value
    if not (isinstance(value, yaml.MappingNode) and not value.value):
        return
    edits, _ = converter._remove(visit, keep_comments=True)
    if edits and converter.edits.add(*edits):
        converter.count(entry["id"])


def bare_empty_verification(converter, visit, entry):
    payload = visit.value
    if not (isinstance(payload, yaml.MappingNode) and not payload.value):
        return
    wrapper = visit.parent
    if converter.edits.add(
        Edit(start(wrapper), end(wrapper, converter.text.text), visit.name)
    ):
        converter.count(entry["id"])


def tie_break_to_window_order_by(converter, visit, entry):
    text = converter.text
    flag = visit.parent
    date = member(flag, "date")
    terms = visit.value
    if not isinstance(terms, yaml.SequenceNode) or date is None:
        return
    items = []
    for term in terms.value:
        if not isinstance(term, yaml.ScalarNode) and not term.flow_style:
            converter.note(
                converter.line_of(visit.key),
                "tie_break",
                "a block order term in tie_break; fold it into window.order_by by hand",
            )
            return
        items.append(converter.source(term))
    order = flow_list([converter.source(date), *items])
    removal, _ = converter._remove(visit, keep_comments=True)
    if not removal:
        return
    window_pair = member_pair(flag, "window")
    if window_pair is None:
        line = (
            " " * text.column(start(visit.key))
            + f"window: {{order_by: {order}}}"
            + text.newline
        )
        removal[-1] = Edit(removal[-1].start, removal[-1].end, removal[-1].text + line)
        edits = removal
    else:
        window = window_pair[1]
        if not isinstance(window, yaml.MappingNode):
            converter.note(
                converter.line_of(visit.key),
                "tie_break",
                "baseline_flag names a window; fold tie_break into that window's order_by by hand",
            )
            return
        group = member_pair(window, "group_by")
        if group is not None:
            insertion = converter.insert_pair(window, "order_by", order, after=group[0])
        elif window.value:
            insertion = converter.insert_pair(
                window, "order_by", order, before=window.value[0][0]
            )
        else:
            insertion = [
                Edit(start(window), end(window, text.text), f"{{order_by: {order}}}")
            ]
        if not insertion:
            return
        edits = removal + insertion
    if converter.edits.add(*edits):
        converter.count(entry["id"])


def cut_right_to_closed(converter, visit, entry):
    value = visit.value
    if core_type(value) != "bool":
        return
    if value.value.lower() == "true":
        edits = [
            Edit(
                start(visit.key),
                end(visit.key, converter.text.text),
                str(entry["proposed"]),
            ),
            Edit(start(value), end(value, converter.text.text), "right"),
        ]
    else:
        edits, _ = converter._remove(visit, keep_comments=True)
    if edits and converter.edits.add(*edits):
        converter.count(entry["id"])


def header(converter, visit, entry):
    if visit.path != ("schema_version",):
        return
    version = visit.value.value if isinstance(visit.value, yaml.ScalarNode) else "1.0"
    lines = [
        f"yamaa: {double_quoted(version)}",
        f"type: {entry['convert'].get('type') or converter.plan.header_type}",
    ]
    if converter.plan.model:
        lines.append(f"model: {converter.plan.model}")
    text = converter.text
    a = start(visit.key)
    b = pair_end(visit.key, visit.value, text.text)
    # The first line keeps any comment written after the version.
    at = text.next_line_start(text.line(b))
    rest = "".join(line + text.newline for line in lines[1:])
    if at == len(text.text) and not text.text.endswith(("\n", "\r")):
        rest = text.newline + rest.rstrip("\r\n")
    if converter.edits.add(Edit(a, b, lines[0]), Edit(at, at, rest)):
        converter.count(entry["id"])
        if converter.plan.model:
            write_new_field(converter, visit, converter.table.get("new.spec.model"))


def write_new_field(converter, visit, entry):
    """Count a field the old surface implied, written by the transform that places it.

    `model` is written with the header, `clear` with the removals, the
    project's `functions` and locks with the D8 merge, and `cut.closed` by
    `cut_right_to_closed`. `copy`, an ODM input and `submission.standard`
    have no source in a 0.5 file, so the unit lists candidates instead."""
    if entry is not None:
        converter.count(entry["id"])


def _intermediates(converter):
    anchor = converter.anchor
    if isinstance(converter.root, yaml.MappingNode) and anchor == converter.bundle.root:
        found = member(converter.root, "intermediates")
        return found.value if isinstance(found, yaml.SequenceNode) else []
    if anchor == ("list", "intermediate_class") and isinstance(
        converter.root, yaml.SequenceNode
    ):
        return converter.root.value
    if anchor == "intermediate_class":
        return [converter.root]
    return []


def write_inferred_intermediate_key(converter, visit, entry):
    if not converter.plan.intermediate_keys:
        return
    for item in _intermediates(converter):
        if not isinstance(item, yaml.MappingNode):
            continue
        identifier = member(item, "id")
        found = converter.plan.intermediate_keys.get(getattr(identifier, "value", None))
        if not found or member(item, "key") is not None:
            continue
        anchor = member_pair(item, "dataset") or member_pair(item, "id")
        edits = converter.insert_pair(item, "key", flow_list(found), after=anchor[0])
        if edits and converter.edits.add(*edits):
            converter.count(entry["id"])


def declare_input_key(converter, visit, entry):
    if not converter.plan.input_keys:
        return
    datasets = member(converter.root, "input")
    if not isinstance(datasets, yaml.MappingNode):
        return
    for key, value in datasets.value:
        found = converter.plan.input_keys.get(key.value)
        if not found:
            continue
        listed = flow_list(found)
        if isinstance(value, yaml.ScalarNode):
            edits = [
                Edit(
                    start(value),
                    end(value, converter.text.text),
                    f"{{path: {converter.source(value)}, key: {listed}}}",
                )
            ]
        elif isinstance(value, yaml.MappingNode) and member(value, "key") is None:
            edits = converter.insert_pair(value, "key", listed)
        else:
            continue
        if edits and converter.edits.add(*edits):
            converter.count(entry["id"])


def environment_to_function_files(converter, visit, entry):
    if not converter.plan.d8 or visit.path != ("environments",):
        return
    value = visit.value
    if isinstance(value, yaml.MappingNode) and not any(
        isinstance(path, yaml.ScalarNode) for _, path in value.value
    ):
        return
    text = converter.text
    edits = []
    if converter.plan.functions and member(converter.root, "functions") is None:
        indent = " " * converter.sequence_style()
        body = ["functions:"] + [
            f"{indent}- {path}" for path in converter.plan.functions
        ]
        line = text.line(start(visit.key))
        while line > 0 and text.line_text(line - 1).lstrip().startswith("#"):
            line -= 1
        at = text.line_start(line)
        edits.append(Edit(at, at, text.newline.join(body) + text.newline))
    if converter.plan.locks:
        column = text.column(start(visit.key)) + 2
        out = []
        for host, lock in converter.plan.locks.items():
            out.append(f"{' ' * column}{host}:")
            out.append(f"{' ' * (column + 2)}lock: {lock}")
        a = text.text.find(":", end(visit.key, text.text)) + 1
        edits.append(
            Edit(a, end(visit.value, text.text), text.newline + text.newline.join(out))
        )
    else:
        removal, _ = converter._remove(visit, keep_comments=True)
        if not removal:
            return
        edits.extend(removal)
    if converter.edits.add(*edits):
        converter.count(entry["id"])
        if converter.plan.functions:
            converter.count("new.project.functions")


def suppression_comment(converter, visit, entry):
    """Applied to every comment of a document by `Converter._suppression_comments`."""


def _outside_the_walk(converter, visit, entry):
    """Applied outside the node walk: the D8 function files, the YAML-null
    clears, and the negative contracts' conditions and paths (see `unit`)."""


TRANSFORMS = {
    "header": Transform("insert", header),
    "environment_to_function_files": Transform("insert", environment_to_function_files),
    "literal_to_expression": Transform("rewrite", literal_to_expression),
    "compute_to_text": Transform("rewrite", compute_to_text),
    "source_wrapper_to_text": Transform("rewrite", source_wrapper_to_text),
    "null_handler": Transform("rewrite", null_handler),
    "tagged_literal_to_text": Transform("rewrite", tagged_literal_to_text),
    "untag_function_value": Transform("rewrite", untag_function_value),
    "unwrap_value": Transform("rewrite", unwrap_value),
    "clear_from_null": Transform("remove", _outside_the_walk),
    "write_inferred_intermediate_key": Transform(
        "plan", write_inferred_intermediate_key
    ),
    "declare_input_key": Transform("plan", declare_input_key),
    "wrap_in_list": Transform("rewrite", wrap_in_list),
    "origin_to_list": Transform("rewrite", origin_to_list),
    "derive_to_derivations": Transform("rewrite", derive_to_derivations),
    "drop_empty_row_derivations": Transform("rewrite", drop_empty_row_derivations),
    "bare_empty_verification": Transform("rewrite", bare_empty_verification),
    "tie_break_to_window_order_by": Transform("rewrite", tie_break_to_window_order_by),
    "cut_right_to_closed": Transform("rewrite", cut_right_to_closed),
    "write_new_field": Transform("plan", write_new_field),
    "rename_condition": Transform("contract", _outside_the_walk),
    "rewrite_path": Transform("contract", _outside_the_walk),
    "suppression_comment": Transform("comment", suppression_comment),
}


# Transforms after which the old spelling never remains in a 1.0 document.
RESIDUE = frozenset(
    {
        "header",
        "literal_to_expression",
        "compute_to_text",
        "source_wrapper_to_text",
        "null_handler",
        "tagged_literal_to_text",
        "untag_function_value",
        "unwrap_value",
        "wrap_in_list",
        "origin_to_list",
        "derive_to_derivations",
        "tie_break_to_window_order_by",
        "cut_right_to_closed",
    }
)


def clear_path(table, bundle, visits):
    """The P-CLR-1 path of a node, in 1.0 spelling, from its visits, root first;
    None when a list without identity lies on the way."""
    segments = []
    for visit in visits:
        if visit.kind == "item":
            container = visit.holder
            identity = (
                IDENTITY.get(container.name)
                if container is not None and len(container.path) == 1
                else None
            )
            value = member(visit.value, identity) if identity else None
            if not isinstance(value, yaml.ScalarNode):
                return None
            segments.append(path_segment(value.value))
        elif visit.kind == "entry":
            segments.append(path_segment(str(visit.name)))
        else:
            name = visit.name
            entry = (
                table.entry(bundle, visit.location()) if visit.kind == "field" else None
            )
            convert = (entry or {}).get("convert") or {}
            if convert.get("transform") in (
                "origin_to_list",
                "wrap_in_list",
                "derive_to_derivations",
            ):
                # The field becomes a list or map: no index addresses it.
                return None
            if convert.get("new_name"):
                name = convert["new_name"]
            segments.append(name)
    path = ""
    for segment in segments:
        path += segment if segment.startswith("[") else ("." if path else "") + segment
    return path
