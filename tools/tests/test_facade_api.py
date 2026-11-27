"""Keep the committed facade inventory synchronized with the facade crate."""

import tempfile
import unittest
from pathlib import Path

from tools.packaging import generate_facade_api as generator
from tools.rust_source import strip_comments


class FacadeApiTests(unittest.TestCase):
    def test_committed_inventory_matches_facade_sources(self):
        self.assertEqual(
            generator.TARGET.read_bytes(), generator.render().encode("utf-8")
        )

    def test_an_impl_of_a_type_its_module_keeps_private_is_not_surface(self):
        module = (
            "pub struct Shown;\n"
            "impl Shown {\n"
            "    pub fn visible(&self) {}\n"
            "}\n"
            "impl Default for Shown {\n"
            "    fn default() -> Self {\n"
            "        Shown\n"
            "    }\n"
            "}\n"
            "pub(crate) struct Kept;\n"
            "impl Kept {\n"
            "    pub fn hidden(&self) {}\n"
            "}\n"
            "impl std::fmt::Display for Kept {\n"
            "    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n"
            '        f.write_str("kept")\n'
            "    }\n"
            "}\n"
            "struct Private<'a>(&'a str);\n"
            "impl<'a> Private<'a> {\n"
            "    pub fn hidden(&self) -> &'a str {\n"
            "        self.0\n"
            "    }\n"
            "}\n"
        )
        entries = []
        state = {"modules": {}, "public": {("yamaa",)}, "files": set()}
        generator._parse_module(("yamaa", "m"), module, entries, state)
        self.assertEqual(
            sorted(entries),
            [
                "yamaa::m::Shown: impl Default for Shown",
                "yamaa::m::Shown: impl Shown",
                "yamaa::m::Shown: pub struct Shown",
                "yamaa::m::Shown::visible: pub fn visible(&self)",
            ],
            "the public type and its impls only; Kept's and Private's impls are not surface",
        )

    def test_a_public_constant_of_a_fixed_length_array_is_refused(self):
        # The parser's refusal, which the committed sources never reach: a
        # frozen array length would make every added value a break (#5080).
        state = {"modules": {}, "public": {("yamaa",)}, "files": set()}
        for module, refused in [
            ('pub const KINDS: [&str; 2] = ["a", "b"];\n', True),
            ('pub const KINDS: &[&str; 2] = &["a", "b"];\n', True),
            ('pub const KINDS: &\'static [&str; 2] = &["a", "b"];\n', True),
            ('pub static KINDS: [&str; 2] = ["a", "b"];\n', True),
            ('pub const KINDS: &[&str] = &["a", "b"];\n', False),
        ]:
            with self.subTest(case=module.strip()):
                entries = []
                if refused:
                    with self.assertRaisesRegex(ValueError, "fixed-length array"):
                        generator._parse_module(("yamaa", "m"), module, entries, state)
                else:
                    generator._parse_module(("yamaa", "m"), module, entries, state)
                    self.assertEqual(
                        entries, ["yamaa::m::KINDS: pub const KINDS: &[&str]"]
                    )

    def test_the_seam_is_one_hidden_module_that_no_frozen_item_names(self):
        # The seam checks (#5080), which the committed sources pass and so
        # never reach: each small source tree breaks one rule.
        seam_lib = "pub mod api;\n#[doc(hidden)]\npub mod seam;\n"
        declared = "pub struct Lock;\nimpl Lock {\n    pub fn new() -> Self {\n        Lock\n    }\n}\n"
        cases = [
            ("admitted", {"lib.rs": seam_lib, "seam.rs": declared}, None),
            (
                "no hidden module",
                {"lib.rs": "pub mod seam;\n", "seam.rs": declared},
                "one #\\[doc\\(hidden\\)\\] pub mod",
            ),
            (
                "two hidden modules",
                {
                    "lib.rs": seam_lib + "#[doc(hidden)]\npub mod other;\n",
                    "seam.rs": declared,
                    "other.rs": "",
                },
                "one #\\[doc\\(hidden\\)\\] pub mod",
            ),
            (
                "a hidden item outside lib.rs",
                {
                    "lib.rs": seam_lib,
                    "seam.rs": declared,
                    "api.rs": "#[doc(hidden)]\npub fn quiet() {}\n",
                },
                "api.rs: #\\[doc\\(hidden\\)\\] marks only",
            ),
            (
                "an impl of a type the seam does not declare",
                {
                    "lib.rs": seam_lib,
                    "seam.rs": declared
                    + "impl crate::api::Run {\n    pub fn extra(&self) {}\n}\n",
                },
                "implements a type it does not declare",
            ),
        ]
        for label, files, refusal in cases:
            with self.subTest(case=label), tempfile.TemporaryDirectory() as root:
                src = Path(root)
                for name, text in files.items():
                    (src / name).write_text(text, encoding="utf-8")

                def check():
                    lib = strip_comments(files["lib.rs"])
                    return generator._seam_types(generator._seam(lib, src), src)

                if refusal is None:
                    self.assertEqual(check(), {"Lock"}, label)
                else:
                    with self.assertRaisesRegex(ValueError, refusal):
                        check()
        frozen = ["yamaa::api::open: pub fn open() -> Lock"]
        with self.assertRaisesRegex(ValueError, "a frozen item names the seam's Lock"):
            generator._refuse_seam_names(frozen, {"Lock"})
        generator._refuse_seam_names(
            ["yamaa::api::Locker: pub struct Locker"], {"Lock"}
        )


if __name__ == "__main__":
    unittest.main()
