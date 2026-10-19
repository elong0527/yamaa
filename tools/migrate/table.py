"""The approved rename table, indexed by today's schema positions.

`tools/migrate/renames.yaml` is the single source of what changes: every
field, registry keyword and enumerated value of today's schemas with its
1.0 spelling and the converter action, the named transforms, the condition
renames and the diagnostic path rewrites. The converter keeps no second copy
of the mapping: renames, deletions and value changes are read from the
entries, and every transform, condition and path rule the table names must
have exactly one implementation here (`check_implemented`).
"""

from pathlib import Path

import yaml

TABLE = Path(__file__).with_name("renames.yaml")

# The file types each bundle's positions belong to, in lookup order.
FILE_TYPES = {
    "spec": ("spec",),
    "project": ("project", "codelists"),
    "environment": ("environment", "function"),
}


class TableError(ValueError):
    pass


class RenameTable:
    """Entries by (file type, owner.field) and by enumerated value."""

    def __init__(self, path=TABLE):
        document = yaml.safe_load(Path(path).read_text(encoding="utf-8"))
        if (
            document.get("format") != "yamaa.rename-table"
            or document.get("format_version") != 1
        ):
            raise TableError(f"{path}: not a format 1 rename table")
        if document.get("status") != "approved":
            raise TableError(f"{path}: the rename table is not approved")
        self.path = Path(path)
        self.transforms = dict(document["transforms"])
        self.entries = list(document["entries"])
        self.conditions = list(document.get("conditions") or [])
        self.paths = list(document.get("paths") or [])
        self.by_id = {}
        self.fields = {}
        self.values = {}
        for entry in self.entries:
            if entry["id"] in self.by_id:
                raise TableError(f"duplicate entry id {entry['id']}")
            self.by_id[entry["id"]] = entry
            location = entry.get("schema")
            if not location:
                continue
            fragment = location.split("#", 1)[1].split(" (", 1)[0]
            if "=" in entry["id"]:
                self.values[(entry["file_type"], fragment, str(entry["today"]))] = entry
            else:
                self.fields[(entry["file_type"], fragment)] = entry
        for entry in self.entries:
            convert = entry.get("convert") or {}
            if convert.get("action") == "transform" and convert.get(
                "transform"
            ) not in (self.transforms):
                raise TableError(
                    f"{entry['id']} names the unknown transform {convert.get('transform')}"
                )

    def entry(self, bundle, location):
        """The entry of a bundle position `owner.field`, or None."""
        for file_type in FILE_TYPES[bundle]:
            found = self.fields.get((file_type, location))
            if found is not None:
                return found
        return None

    def value_entry(self, bundle, location, value):
        for file_type in FILE_TYPES[bundle]:
            found = self.values.get((file_type, location, value))
            if found is not None:
                return found
        return None

    def get(self, entry_id):
        try:
            return self.by_id[entry_id]
        except KeyError:
            raise TableError(f"the rename table has no entry {entry_id}") from None

    def new_name(self, entry_id):
        """The 1.0 field name an entry gives, from its convert action or proposal."""
        entry = self.get(entry_id)
        convert = entry.get("convert") or {}
        if convert.get("new_name"):
            return convert["new_name"]
        proposed = entry.get("proposed")
        if isinstance(proposed, str) and proposed.isidentifier():
            return proposed
        return entry["today"]

    def condition(self, name):
        for item in self.conditions:
            if item.get("today") == name:
                return item
        return None

    def path_rule(self, today):
        for item in self.paths:
            if item.get("today") == today:
                return item
        raise TableError(f"the rename table has no path rule {today!r}")

    def check_implemented(self, transforms, path_rules):
        """Refuse a table whose transforms or path rules the code does not implement."""
        missing = sorted(set(self.transforms) - set(transforms))
        extra = sorted(set(transforms) - set(self.transforms))
        if missing or extra:
            raise TableError(
                f"transforms differ from the table: unimplemented {missing}, unknown {extra}"
            )
        written = {item["today"] for item in self.paths}
        missing = sorted(written - set(path_rules))
        extra = sorted(set(path_rules) - written)
        if missing or extra:
            raise TableError(
                f"path rules differ from the table: unimplemented {missing}, unknown {extra}"
            )


_TABLE = None


def load(path=None):
    global _TABLE
    if path is not None:
        return RenameTable(path)
    if _TABLE is None:
        _TABLE = RenameTable()
    return _TABLE
