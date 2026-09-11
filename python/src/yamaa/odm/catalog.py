"""Study/version-scoped metadata names used to enrich clinical rows."""

from collections.abc import Mapping
from typing import Literal

from yamaa.odm.errors import ODMMetadataError
from yamaa.odm.profiles import MetadataScope

DefinitionKind = Literal["event", "form", "group", "item"]
CatalogKey = tuple[str, str, str]


class MetadataCatalog:
    """Reject ambiguous definitions and resolve only an explicit metadata scope."""

    def __init__(self) -> None:
        self._definitions: dict[DefinitionKind, dict[CatalogKey, str | None]] = {
            "event": {},
            "form": {},
            "group": {},
            "item": {},
        }

    def add(
        self,
        kind: DefinitionKind,
        study_oid: str,
        metadata_version_oid: str,
        oid: str,
        name: str | None,
    ) -> None:
        key = (study_oid, metadata_version_oid, oid)
        definitions = self._definitions[kind]
        if key in definitions and definitions[key] != name:
            raise ODMMetadataError(
                f"conflicting {kind} definition for {study_oid}/{metadata_version_oid}/{oid}"
            )
        definitions[key] = name

    def resolve(
        self,
        kind: DefinitionKind,
        study_oid: str,
        metadata_version_oid: str,
        oid: str,
        aliases: Mapping[MetadataScope, MetadataScope],
    ) -> tuple[bool, str | None]:
        target_study, target_version = aliases.get(
            (study_oid, metadata_version_oid),
            (study_oid, metadata_version_oid),
        )
        key = (target_study, target_version, oid)
        definitions = self._definitions[kind]
        return key in definitions, definitions.get(key)
