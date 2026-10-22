"""Keep the committed facade inventory synchronized with the facade crate."""

import unittest

from tools.packaging import generate_facade_api as generator


class FacadeApiTests(unittest.TestCase):
    def test_committed_inventory_matches_facade_sources(self):
        self.assertEqual(
            generator.TARGET.read_bytes(), generator.render().encode("utf-8")
        )

    def test_only_exact_test_gates_are_test_only(self):
        self.assertTrue(generator._is_test_cfg(["#[cfg(test)]"]))
        self.assertTrue(generator._is_test_cfg(["#[cfg(all(test, unix))]"]))
        self.assertFalse(generator._is_test_cfg(["#[cfg(not(test))]"]))
        self.assertFalse(generator._is_test_cfg(['#[cfg(feature = "test-utils")]']))
        self.assertFalse(generator._is_test_cfg(["#[cfg(unix)]"]))
        self.assertFalse(generator._is_test_cfg([]))

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


if __name__ == "__main__":
    unittest.main()
