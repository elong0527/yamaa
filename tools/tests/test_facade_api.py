"""Keep the committed facade inventory synchronized with the facade crate."""

import unittest

from tools.packaging import generate_facade_api as generator


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


if __name__ == "__main__":
    unittest.main()
