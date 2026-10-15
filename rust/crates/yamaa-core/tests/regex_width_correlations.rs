//! Public controls for structural lookbehind width and capture participation.
use std::fmt::Write;
use yamaa_core::regex::{CompileError, Pattern};

fn append_references(source: &mut String, count: usize) {
    for number in 1..=count {
        write!(source, r"\{number}").unwrap();
    }
}

#[test]
fn every_referenced_independent_capture_can_change_the_lookbehind_width() {
    // Each of the thirty independent choices contributes either one or two.
    // Their total varies from thirty to sixty; none is an unrelated capture.
    let mut source = "(a|bb)".repeat(30);
    let byte = source.len();
    source.push_str("(?<=");
    append_references(&mut source, 30);
    source.push(')');

    assert_eq!(
        Pattern::compile(&source).unwrap_err(),
        CompileError::Invalid {
            byte,
            reason: "variable-length lookbehind",
        }
    );
}

#[test]
fn all_referenced_correlated_capture_blocks_have_fixed_total_width() {
    // Each block is (1, 2, 0, 0) or (0, 0, 2, 1). Its four references
    // therefore total three in either arm, so thirty blocks total ninety.
    let mut source = "^".to_owned() + &"(?:(a)(bc)|(ab)(c))".repeat(30);
    source.push_str("(?<=");
    append_references(&mut source, 120);
    source.push_str(")d$");

    let pattern = Pattern::compile(&source).unwrap();
    assert_eq!(pattern.group_count(), 120);
    let subject = "abc".repeat(30) + "d";
    let found = pattern.full_match(&subject).unwrap().unwrap();
    assert_eq!(found.groups.len(), 121);
    assert_eq!(found.groups[0], Some(subject.as_str()));
    for captures in found.groups[1..].chunks_exact(4) {
        assert_eq!(captures, [Some("a"), Some("bc"), None, None]);
    }
}

#[test]
fn empty_optional_iterations_do_not_commit_positive_assertion_captures() {
    // The assertion arm consumes zero and cannot be an optional iteration.
    // The stop, b, and bb paths all leave capture one unentered, so its
    // lookbehind reference always has width zero.
    let pattern = Pattern::compile(r"^(?:(?=(a))|b|bb){0,1}(?<=\1)c$").unwrap();
    for subject in ["c", "bc", "bbc"] {
        assert_eq!(
            pattern.full_match(subject).unwrap().unwrap().groups,
            [Some(subject), None]
        );
    }

    // A successful zero-consuming assertion still cannot commit an optional
    // iteration. A required iteration does commit it; the later empty optional
    // iteration must preserve that required capture rather than erase it.
    for (source, capture) in [
        (r"^a(?:(?=(a))){0,1}(?<=\1)a$", None),
        (r"^a(?:(?=(a))){1,2}(?<=\1)a$", Some("a")),
    ] {
        let pattern = Pattern::compile(source).unwrap();
        assert_eq!(
            pattern.full_match("aa").unwrap().unwrap().groups,
            [Some("aa"), capture]
        );
    }
}

#[test]
fn repeated_reference_widths_retain_incoming_capture_correlations() {
    // The first reference pair is one or two and the second is two or one.
    // Repeating each twice always totals six; iteration choices cannot erase
    // the shared choice that established the incoming captures.
    let source = r"^(?:(a)(bc)|(ab)(c))(?:aabcbc|ababcc)(?<=(?:\1\3){2}(?:\2\4){2})x$";
    let pattern = Pattern::compile(source).unwrap();
    for (subject, captures) in [
        ("abcaabcbcx", [Some("a"), Some("bc"), None, None]),
        ("abcababccx", [None, None, Some("ab"), Some("c")]),
    ] {
        let found = pattern.full_match(subject).unwrap().unwrap();
        assert_eq!(found.groups[0], Some(subject));
        assert_eq!(found.groups[1..], captures);
    }
}

#[test]
fn flat_capture_dependency_chains_keep_their_fixed_width() {
    // Group n references group n - 1 and adds one scalar, so group n has
    // width n. These dependencies are long while grammar nesting stays one.
    let mut source = "(a)".to_owned();
    for previous in 1..3000 {
        write!(source, r"(\{previous}a)").unwrap();
    }
    source.push_str(r"(?<=\3000)");

    let pattern = Pattern::compile(&source).unwrap();
    assert_eq!(pattern.group_count(), 3000);
}

#[test]
fn flat_capture_dependency_chains_retain_their_initial_width_choice() {
    // The first two captures total one or two. Group three adds one, and
    // each subsequent group adds one more, leaving group 3000 variable.
    let mut source = r"(?:(a)|(bb))(\1\2a)".to_owned();
    for previous in 3..3000 {
        write!(source, r"(\{previous}a)").unwrap();
    }
    let byte = source.len();
    source.push_str(r"(?<=\3000)");

    assert_eq!(
        Pattern::compile(&source).unwrap_err(),
        CompileError::Invalid {
            byte,
            reason: "variable-length lookbehind",
        }
    );
}

#[test]
fn flat_capture_dependency_chains_keep_each_new_width_choice() {
    // Each group independently chooses one scalar or the preceding capture
    // followed by one scalar. Group n therefore ranges from one to n, with
    // a fresh alternative at every link rather than one shared initial choice.
    let mut source = "(a)".to_owned();
    for previous in 1..3000 {
        write!(source, r"(a|\{previous}b)").unwrap();
    }
    let byte = source.len();
    source.push_str(r"(?<=\3000)");

    assert_eq!(
        Pattern::compile(&source).unwrap_err(),
        CompileError::Invalid {
            byte,
            reason: "variable-length lookbehind",
        }
    );
}

#[test]
fn repeated_body_choices_do_not_bind_independent_incoming_captures() {
    // The sixty incoming captures form thirty independent pairs of width one
    // or two. Captures 61 and 62 belong to the repeated body; their shared
    // choice must be unioned for earlier iterations without binding the thirty
    // incoming choices. The body still varies, so repeating it twice varies.
    let mut source = "(?:(a)|(bb))".repeat(30);
    let byte = source.len();
    source.push_str("(?<=(?:");
    append_references(&mut source, 62);
    source.push_str("(?:(c)|(dd))){2})");

    assert_eq!(
        Pattern::compile(&source).unwrap_err(),
        CompileError::Invalid {
            byte,
            reason: "variable-length lookbehind",
        }
    );
}

#[test]
fn alternative_width_unions_keep_independent_incoming_choices() {
    // Both lookbehind arms preserve the incoming captures. Their widths are
    // the same thirty-pair sum, or that sum plus one. The complete range is
    // therefore thirty through sixty-one, independent of which arm is used.
    let mut source = "(?:(a)|(bb))".repeat(30);
    let byte = source.len();
    source.push_str("(?<=(?:");
    append_references(&mut source, 60);
    source.push('|');
    append_references(&mut source, 60);
    source.push_str("c))");

    assert_eq!(
        Pattern::compile(&source).unwrap_err(),
        CompileError::Invalid {
            byte,
            reason: "variable-length lookbehind",
        }
    );
}

#[test]
fn independent_empty_optional_iterations_leave_every_capture_unentered() {
    // Each only-capturing arm consumes zero and cannot commit an optional
    // iteration. All thirty references must therefore consume zero, even
    // though the blocks can independently stop or consume b or bb.
    let mut source = "^".to_owned() + &"(?:(?=(a))|b|bb){0,1}".repeat(30);
    source.push_str("(?<=");
    append_references(&mut source, 30);
    source.push_str(")c$");

    let pattern = Pattern::compile(&source).unwrap();
    for subject in ["c".to_owned(), "b".repeat(30) + "c"] {
        let found = pattern.full_match(&subject).unwrap().unwrap();
        assert_eq!(found.groups.len(), 31);
        assert_eq!(found.groups[0], Some(subject.as_str()));
        assert!(found.groups[1..].iter().all(Option::is_none));
    }
}

#[test]
fn flat_capture_pairs_retain_a_fresh_choice_at_every_dependency() {
    // Each new pair captures either a, or the preceding pair followed by b.
    // Unlike a noncapturing alternative, the arms change referenced capture
    // participation. At stage n the pair has width one through n; 1500 stages
    // use 2999 captures while preserving shallow grammar nesting.
    let mut source = "(a)".to_owned();
    let mut previous = r"\1".to_owned();
    for first in (2..3000).step_by(2) {
        write!(source, r"(?:(a)|({previous}b))").unwrap();
        previous = format!(r"\{first}\{second}", second = first + 1);
    }
    let byte = source.len();
    write!(source, "(?<={previous})").unwrap();

    assert_eq!(
        Pattern::compile(&source).unwrap_err(),
        CompileError::Invalid {
            byte,
            reason: "variable-length lookbehind",
        }
    );
}
