//! Executor behaviour no specification can reach.
use crate::table::ValueRef;
use std::convert::Infallible;

/// A buffer the executor cannot reserve fails the run as an allocation
/// failure instead of aborting the process: a frame, a filled vector, and the
/// hash partition.
#[test]
fn a_refused_reservation_is_an_allocation_failure() {
    use crate::dataset::execution::ExecutionError;
    let mut frame = super::frame::Frame::new(3);
    let refused = super::reserved::<(), Infallible>(frame.try_reserve(usize::MAX, 0));
    assert!(matches!(*refused.unwrap_err(), ExecutionError::Allocation));
    let refused = super::filled::<u64, Infallible>(usize::MAX, 0);
    assert!(matches!(*refused.unwrap_err(), ExecutionError::Allocation));
    assert!(super::group::partition(usize::MAX, 1, |_, _| ValueRef::Missing).is_err());
    // A reservation that fits changes nothing a reader sees.
    super::reserved::<(), Infallible>(frame.try_reserve(4, 4)).unwrap();
    assert_eq!(frame.rows(), 0);
}
