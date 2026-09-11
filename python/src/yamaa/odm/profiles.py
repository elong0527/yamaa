"""Explicit compatibility profiles for supported ODM exports."""

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field
from yamaa.odm.errors import ODMParseError

ODM_13_NAMESPACE = "http://www.cdisc.org/ns/odm/v1.3"
ODM_20_NAMESPACE = "http://www.cdisc.org/ns/odm/v2.0"
KN189_ARCHIVE_MEMBER = "KN189_odm_dag/odm.xml"

MetadataScope = tuple[str, str]


class ImportProfile(BaseModel):
    """Closed parsing and metadata-resolution decisions for an ODM source."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    name: str = Field(min_length=1)
    allowed_namespaces: frozenset[str]
    archive_member: str | None = None
    metadata_aliases: dict[MetadataScope, MetadataScope] = Field(default_factory=dict)
    require_item_metadata: bool = True
    max_expanded_bytes: int = Field(default=256 * 1024 * 1024, gt=0)
    max_xml_depth: int = Field(default=64, ge=8)


_MAIN_CART_SCOPE = ("S_20204824(TEST)", "v1.0.0")
_CART_T_ALIASES: dict[MetadataScope, MetadataScope] = {
    ("S_DF(TEST)", "v1.0.0-S_DF(TEST)"): _MAIN_CART_SCOPE,
    ("S_1234(TEST)", "v1.0.0-S_1234(TEST)"): _MAIN_CART_SCOPE,
    ("S_MGH_6315(TEST)", "v1.0.0-S_MGH_6315(TEST)"): _MAIN_CART_SCOPE,
    ("S_ABBOTT(TEST)", "v1.0.0-S_ABBOTT(TEST)"): _MAIN_CART_SCOPE,
    ("S_ZZ01_7109(TEST)", "v1.0.0-S_ZZ01_7109(TEST)"): _MAIN_CART_SCOPE,
    ("S_CNH(TEST)", "v1.0.0-S_CNH(TEST)"): _MAIN_CART_SCOPE,
    ("S_SJ01(TEST)", "v1.0.0-S_SJ01(TEST)"): _MAIN_CART_SCOPE,
}

STRICT_PROFILE = ImportProfile(
    name="strict",
    allowed_namespaces=frozenset({ODM_13_NAMESPACE, ODM_20_NAMESPACE}),
)
CART_T_OPENCLINICA_PROFILE = ImportProfile(
    name="cart-t-openclinica",
    allowed_namespaces=frozenset({ODM_13_NAMESPACE}),
    metadata_aliases=_CART_T_ALIASES,
)
KN189_PROFILE = ImportProfile(
    name="kn189",
    allowed_namespaces=frozenset({ODM_20_NAMESPACE}),
    archive_member=KN189_ARCHIVE_MEMBER,
)

ProfileName = Literal["strict", "cart-t-openclinica", "kn189"]

_PROFILES = {
    profile.name: profile
    for profile in (STRICT_PROFILE, CART_T_OPENCLINICA_PROFILE, KN189_PROFILE)
}


def get_profile(profile: ProfileName | str | ImportProfile) -> ImportProfile:
    """Resolve a registered profile while retaining custom typed profiles."""
    if isinstance(profile, ImportProfile):
        return profile
    try:
        return _PROFILES[profile]
    except KeyError as error:
        names = ", ".join(sorted(_PROFILES))
        raise ODMParseError(
            f"unknown ODM profile {profile!r}; choose {names}"
        ) from error
