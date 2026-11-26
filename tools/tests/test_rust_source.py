"""The shared reader of Rust source: its tokens, brackets and test gates."""

import unittest

from tools import rust_source


class Tokens(unittest.TestCase):
    def test_literals_and_nested_comments_are_whole_tokens(self):
        # Seam: the token boundaries every guard and generator reads Rust
        # through; their outputs show only the effect of one boundary.
        source = (
            'let a = \'"\'; /* outer /* inner */ still "text" */\n'
            'let b = r#"a "quoted" }"#; let c = \'\\u{7B}\';\n'
            "fn f<'a>(x: &'a str) -> &'a str { x } // done\n"
            'let d = b"x\\"}";\n'
        )
        self.assertEqual(
            [
                (kind, source[start:end], line, text)
                for kind, start, end, line, text in rust_source.rust_tokens(source)
            ],
            [
                ("character", "'\"'", 1, None),
                ("comment", '/* outer /* inner */ still "text" */', 1, None),
                ("string", 'r#"a "quoted" }"#', 2, 'a "quoted" }'),
                ("character", "'\\u{7B}'", 2, None),
                ("comment", "// done", 3, None),
                ("string", '"x\\"}"', 4, 'x"}'),
            ],
            "a raw string ends at its own delimiter, a block comment nests, "
            "a quote in a character is not a string and a lifetime is not a "
            "character",
        )

    def test_a_bracket_inside_a_literal_does_not_close(self):
        # Seam: bracket matching on source that keeps its literals, as the
        # facade inventory reads it.
        source = '{ \'}\' "}" r#"}"# }'
        self.assertEqual(
            rust_source.balanced(source, 0, "{", "}"),
            len(source),
            "only the last brace closes the first",
        )


class TestGates(unittest.TestCase):
    def test_only_exact_test_gates_are_test_only(self):
        # Seam: the one judgement of a test-only attribute that the facade
        # inventory and the diagnostics guard both apply.
        cases = (
            ("#[cfg(test)]", True),
            ("#[cfg( test )]", True),
            ("#[cfg(all(test, unix))]", True),
            ("#[cfg(not(test))]", False),
            ('#[cfg(feature = "test-utils")]', False),
            ("#[cfg(all(unix, not(test)))]", False),
            ("#[cfg(unix)]", False),
        )
        for attribute, gated in cases:
            with self.subTest(case=attribute):
                self.assertEqual(rust_source.is_test_gate(attribute), gated)


if __name__ == "__main__":
    unittest.main()
