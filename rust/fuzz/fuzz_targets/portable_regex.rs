//! The portable regular-expression subset: its parser, lowering and
//! compilation, then every search a consumer runs. Each input is a pattern,
//! then optionally a NUL byte and a subject (YAML text holds no NUL
//! character, so no written pattern does; `\u{0}` spells one). A compiled
//! pattern's two scans of `search_all`, the restarted search and the
//! checkpointed backward scan, must find the same matches (TEXT-11), for the
//! subject as written and for it repeated past `RESTART_BYTES`, where
//! `search_all` switches from one to the other; and the first of those
//! matches is the one `search` and `is_match` report. The restarted search is
//! quadratic past the limit by design, so a subject longer than twice the
//! limit is searched by `search_all` alone.
#![no_main]

use libfuzzer_sys::fuzz_target;
use yamaa_core::portable_regex::{Pattern, RESTART_BYTES};

/// The scans agree, `search_all` is the one its length selects, and its
/// first span is the leftmost-first match `search` reports.
fn check(pattern: &Pattern, subject: &str) {
    let [restarted, scanned] = pattern.search_all_by_each_scan(subject);
    assert_eq!(restarted, scanned, "the scans of {} bytes", subject.len());
    assert_eq!(pattern.search_all(subject), restarted);
    let first = pattern.search(subject).map(|found| {
        let whole = found.groups[0].expect("group zero is the whole match");
        let start = whole.as_ptr() as usize - subject.as_ptr() as usize;
        (start, start + whole.len())
    });
    assert_eq!(first, restarted.first().copied());
    assert_eq!(pattern.is_match(subject), first.is_some());
    assert_eq!(
        pattern.search(subject).map(|found| found.groups.len()),
        first.map(|_| pattern.group_count() + 1)
    );
}

fuzz_target!(|data: &[u8]| {
    let (pattern, subject) = match data.iter().position(|&byte| byte == 0) {
        Some(at) => (&data[..at], &data[at + 1..]),
        None => (data, &[][..]),
    };
    let (Ok(pattern), Ok(subject)) = (std::str::from_utf8(pattern), std::str::from_utf8(subject))
    else {
        return;
    };
    let Ok(pattern) = Pattern::compile(pattern) else {
        return;
    };
    if subject.len() > 2 * RESTART_BYTES {
        let _ = pattern.search_all(subject);
        return;
    }
    check(&pattern, subject);
    if !subject.is_empty() && subject.len() <= RESTART_BYTES {
        // The fewest copies that pass the limit, so `search_all` takes the
        // backward scan where the subject alone took restarted searches.
        let long = subject.repeat(RESTART_BYTES / subject.len() + 1);
        check(&pattern, &long);
    }
});
