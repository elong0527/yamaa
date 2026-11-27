//! A partitioned run cancelled between its partitions, and runs with four
//! partitions deriving at once (#4193), which report the events and attempt
//! of one at a time and decode again in turn a partition that does not
//! decode on its thread.

use super::*;

/// A partitioned run polls once at admission, at every checkpoint of each
/// partition's derivation, then at the checkpoints of its output keys and
/// checks. [`graded`] over five one-record partitions polls its five
/// derivation checkpoints in each, then its output keys and its row count.
/// Cancelled at any of those polls, the run keeps nothing any partition
/// built, and its observer exits the phase it stopped in.
#[test]
fn a_run_cancelled_between_partitions_keeps_nothing_and_names_its_phase() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    let (derivation, tail) = polled(&resolved);
    assert_eq!((derivation, tail), (5, 2), "the checkpoints polled");
    let total = 1 + 5 * derivation + tail;
    let run = |probe: &Probe, observe: &mut dyn FnMut(ExecutionEvent)| {
        resolved.execute_partitioned(
            (&mut Source::new(&input, &[1; 5]), None),
            &[],
            &mut NoFunctions::new(),
            Some(observe),
            &|| probe.cancelled(),
            (0, NonZeroUsize::MIN),
        )
    };
    let probe = Probe::new(None);
    assert_eq!(
        as_stated(run(&probe, &mut |_| {})),
        graded_attempt(),
        "uncancelled"
    );
    assert_eq!(probe.polls.get(), total, "polls of an uncancelled run");
    assert_cancelled_at_every_poll(&run, (5, derivation, total));
}

/// The events of `run`, with its attempt.
fn observed_run(
    run: impl FnOnce(&mut dyn FnMut(ExecutionEvent)) -> ExecutionAttempt<Infallible>,
) -> (ExecutionAttempt<Infallible>, Vec<ExecutionEvent>) {
    let mut events = Vec::new();
    let attempt = run(&mut |event| events.push(event));
    (attempt, events)
}

/// Four partitions deriving at once report what one at a time reports
/// (#4193): the same events in the same order, each partition the serial
/// run decodes reported once, in order, and the same attempt, whichever
/// thread decodes them. [`graded`] over five one-record partitions reports
/// partitions 1 to 5 and succeeds; [`conversions`] over a, a, then b, under
/// its two keys, reports both partitions and fails in the first, at a2's
/// 3.5, so the second partition's run stops where the first failed.
#[test]
fn a_run_on_four_threads_reports_the_events_and_attempt_of_a_run_on_one() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    let run = |threads, decoding| {
        observed_run(|observe| {
            decoded_as(&mut Source::new(&input, &[1; 5]), decoding, |source| {
                resolved.execute_partitioned(
                    source,
                    &[],
                    &mut NoFunctions::new(),
                    Some(observe),
                    &|| false,
                    (0, threads),
                )
            })
        })
    };
    let (one, events) = run(NonZeroUsize::MIN, Decoding::Calling);
    let partitions: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            ExecutionEvent::Partition { number, records } => Some((*number, *records)),
            _ => None,
        })
        .collect();
    assert_eq!(
        partitions,
        [(1, 1), (2, 1), (3, 1), (4, 1), (5, 1)],
        "the partitions one thread reports"
    );
    assert_eq!(
        as_stated(run(NonZeroUsize::MIN, Decoding::Calling).0),
        graded_attempt(),
        "graded on one thread"
    );
    let one = (one, events);
    for decoding in DECODINGS {
        assert_eq!(
            run(FOUR, decoding),
            one,
            "graded on four threads, decoded {decoding:?}"
        );
    }
    let (plan, input) = conversions(&[("a", 1, 0.5, 1.0), ("a", 2, 1.5, 3.5), ("b", 1, 1.0, 2.5)]);
    let resolved = plan.resolve().unwrap();
    let run = |threads| {
        observed_run(|observe| {
            resolved.execute_divided(
                by_keys(2, 1, false).map(|partitioning| Partitioning {
                    threads,
                    ..partitioning
                }),
                &input,
                &[],
                &mut NoFunctions::new(),
                Some(observe),
                &|| false,
            )
        })
    };
    let one = run(NonZeroUsize::MIN);
    assert!(
        one.1.contains(&ExecutionEvent::Partition {
            number: 2,
            records: 1
        }),
        "one thread reports the second partition: {:?}",
        one.1
    );
    assert_eq!(run(FOUR), one, "conversions on four threads");
}

/// Four partitions deriving at once poll the probe on the calling thread
/// alone, which the probe's `Cell` holds it to (#4193): at admission, then
/// each time the calling thread hands out a partition or waits for one,
/// so at least once for each of [`graded`]'s five one-record partitions and
/// once more before the run combines them. Cancelled at any of its first
/// six polls, the run keeps nothing, polls no more, and its observer exits
/// admission at the first poll and derivation at the others.
/// So it is whichever thread decodes the partitions.
#[test]
fn a_run_on_four_threads_cancelled_in_derivation_keeps_nothing_and_names_its_phase() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    for (from, decoding) in (1..=6).flat_map(|from| DECODINGS.map(|decoding| (from, decoding))) {
        let probe = Probe::new(Some(from));
        let (attempt, events) = observed_run(|observe| {
            decoded_as(&mut Source::new(&input, &[1; 5]), decoding, |source| {
                resolved.execute_partitioned(
                    source,
                    &[],
                    &mut NoFunctions::new(),
                    Some(observe),
                    &|| probe.cancelled(),
                    (0, FOUR),
                )
            })
        });
        let phase = match from {
            1 => ExecutionPhase::Admission,
            _ => ExecutionPhase::Derivation,
        };
        let expected = ExecutionAttempt {
            result: Err(Box::new(ExecutionError::Cancelled)),
            handler_counts: Vec::new(),
            retained_verifications: Vec::new(),
            template_rows: Vec::new(),
        };
        assert_eq!(
            (attempt, probe.polls.get(), &events[events.len() - 2..]),
            (
                expected,
                from,
                &[
                    ExecutionEvent::Exit {
                        phase,
                        outcome: Outcome::Failed,
                    },
                    ExecutionEvent::Phase(ExecutionPhase::Finished),
                ][..]
            ),
            "cancelled at poll {from}, decoded {decoding:?}: the attempt, the polls and the phase it exits"
        );
    }
}

/// A partition that does not decode on the thread deriving it is decoded
/// again on the calling thread in the serial run's turn (#4193), so a
/// decode that fails only on a thread changes nothing, and one that fails
/// on every thread ends the run where the serial run ends it. [`graded`]
/// over five one-record partitions: when the third does not decode on its
/// thread, the run succeeds with the authored attempt and reports
/// partitions 1 to 5; when it does not decode at all, the run keeps
/// nothing, as a cancelled run, and reports partitions 1 to 3, the third
/// being the one the serial run was decoding when it ended.
#[test]
fn a_partition_that_does_not_decode_on_its_thread_decodes_again_in_turn() {
    let (plan, input) = graded();
    let resolved = plan.resolve().unwrap();
    let run = |threads, (calling, shared): (Option<usize>, &'static [usize])| {
        let mut source = Source::new(&input, &[1; 5]);
        source.undecodable = calling;
        let decode = source.shared(shared);
        let shared = (threads != NonZeroUsize::MIN).then_some(&decode as &SharedDecode<'_, _>);
        let (attempt, events) = observed_run(|observe| {
            resolved.execute_partitioned(
                (&mut source, shared),
                &[],
                &mut NoFunctions::new(),
                Some(observe),
                &|| false,
                (0, threads),
            )
        });
        let partitions: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                ExecutionEvent::Partition { number, records } => Some((*number, *records)),
                _ => None,
            })
            .collect();
        (attempt, partitions)
    };
    let cancelled = || ExecutionAttempt {
        result: Err(Box::new(ExecutionError::Cancelled)),
        handler_counts: Vec::new(),
        retained_verifications: Vec::new(),
        template_rows: Vec::new(),
    };
    // Each case: whether the third partition decodes on the calling
    // thread, the threads, and the partitions that do not decode on theirs.
    let cases: [(&str, bool, NonZeroUsize, &'static [usize]); 4] = [
        ("on one thread", true, NonZeroUsize::MIN, &[]),
        ("only on its thread", true, FOUR, &[2]),
        ("anywhere, on one thread", false, NonZeroUsize::MIN, &[2]),
        ("anywhere, on four threads", false, FOUR, &[2, 3]),
    ];
    for (label, decodes, threads, shared) in cases {
        let (attempt, reported) = match decodes {
            true => (graded_attempt(), 5),
            false => (cancelled(), 3),
        };
        let partitions: Vec<_> = (1..=reported).map(|number| (number, 1)).collect();
        let (got, events) = run(threads, ((!decodes).then_some(2), shared));
        assert_eq!(
            (as_stated(got), events),
            (attempt, partitions),
            "the third partition does not decode {label}: the attempt and the partitions reported"
        );
    }
}
