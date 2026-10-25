//! The suite's allocator, which counts each thread's live heap bytes and
//! their peak (#3830), so a test measures what a run on its own thread
//! holds whatever the tests beside it allocate. A run of the facade runs on
//! its caller's thread.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Counting;

#[global_allocator]
static COUNTING: Counting = Counting;

thread_local! {
    /// The bytes this thread allocated less those it freed, which a block
    /// another thread allocated can drive below zero.
    static LIVE: Cell<isize> = const { Cell::new(0) };
    static PEAK: Cell<isize> = const { Cell::new(0) };
}

fn grow(bytes: usize) {
    let bytes = isize::try_from(bytes).unwrap_or(isize::MAX);
    // A thread being torn down has no counters left; its blocks go uncounted.
    let _ = LIVE.try_with(|live| {
        let now = live.get().saturating_add(bytes);
        live.set(now);
        let _ = PEAK.try_with(|peak| peak.set(peak.get().max(now)));
    });
}

fn shrink(bytes: usize) {
    let bytes = isize::try_from(bytes).unwrap_or(isize::MAX);
    let _ = LIVE.try_with(|live| live.set(live.get().saturating_sub(bytes)));
}

// SAFETY: every block comes from, and returns to, the system allocator
// unchanged; the counters only observe the sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller's layout contract passes through.
        let block = unsafe { System.alloc(layout) };
        if !block.is_null() {
            grow(layout.size());
        }
        block
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: as `alloc`.
        let block = unsafe { System.alloc_zeroed(layout) };
        if !block.is_null() {
            grow(layout.size());
        }
        block
    }

    unsafe fn dealloc(&self, block: *mut u8, layout: Layout) {
        // SAFETY: the caller returns a block this allocator gave it.
        unsafe { System.dealloc(block, layout) };
        shrink(layout.size());
    }

    /// A reallocation counts both blocks while it copies, the most it can
    /// hold.
    unsafe fn realloc(&self, block: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: as `dealloc`, with the caller's new size.
        let moved = unsafe { System.realloc(block, layout, size) };
        if !moved.is_null() {
            grow(size);
            shrink(layout.size());
        }
        moved
    }
}

/// The bytes this thread holds now.
pub fn live() -> isize {
    LIVE.with(Cell::get)
}

/// The most bytes this thread held since the last call, which starts the
/// next count from what it holds now.
pub fn take_peak() -> isize {
    let now = live();
    PEAK.with(|peak| peak.replace(now))
}
