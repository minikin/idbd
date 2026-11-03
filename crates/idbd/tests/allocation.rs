//! C23: Avoid per-example heap allocation in the core.
//!
//! A counting global allocator records allocations made by the current thread while a
//! measurement window is open. Setup, snapshots and harness work run outside the window.

#![expect(
    unsafe_code,
    reason = "a counting allocator must implement the unsafe GlobalAlloc trait"
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::hint::black_box;

use idbd::{Bounds, Config, Idbd};

struct CountingAllocator;

thread_local! {
    static MEASURING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn record() {
    // `try_with` because the allocator can run while thread-local storage is torn down.
    let _ = MEASURING.try_with(|measuring| {
        if measuring.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: every method forwards to `System` with the caller's arguments unchanged, so
// `System` upholds the `GlobalAlloc` contract. Counting touches only const-initialized
// thread-local cells, which never allocate.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: the caller upholds `GlobalAlloc::alloc`'s contract for `layout`.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: the caller upholds `GlobalAlloc::alloc_zeroed`'s contract for `layout`.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record();
        // SAFETY: the caller upholds `GlobalAlloc::realloc`'s contract for these arguments.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller upholds `GlobalAlloc::dealloc`'s contract for these arguments.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn count_allocations(work: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(0));
    MEASURING.with(|measuring| measuring.set(true));
    work();
    MEASURING.with(|measuring| measuring.set(false));
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn counter_detects_allocation() {
    let allocations = count_allocations(|| {
        black_box(vec![1.0_f64; 16]);
    });
    assert!(allocations > 0, "the counting allocator must observe a Vec");
}

#[test]
fn c23_avoid_per_example_heap_allocation_in_the_core() {
    const N: usize = 64;
    for bounds in [Bounds::none(), Bounds::sutton_safety()] {
        let config = Config::new(0.05, 0.01)
            .and_then(|c| c.with_bounds(bounds))
            .expect("valid config");
        let mut learner = Idbd::new(N, config).expect("valid learner");
        let mut features = vec![0.0; N];
        let mut sink = 0.0;

        let allocations = count_allocations(|| {
            for t in 0..2_000_u32 {
                let phase = f64::from(t);
                for (i, x) in (0_u32..).zip(features.iter_mut()) {
                    *x = (phase * 0.37 + f64::from(i)).sin();
                }
                let target = features[0] - 0.5 * features[1] + 0.25 * features[2];
                sink += learner.predict(&features).expect("prediction");
                let report = learner.update(&features, target).expect("update");
                sink += report.error;
                sink += learner.weights()[0] + learner.log_step_sizes()[1] + learner.traces()[2];
                sink += learner.step_sizes().sum::<f64>();
                sink += f64::from(u32::try_from(learner.num_features()).expect("small"));
                sink += learner.config().meta_step_size();
                if learner.update(&features, f64::NAN).is_ok() {
                    sink += 1.0;
                }
            }
        });

        black_box(sink);
        assert_eq!(allocations, 0, "bounds {bounds:?}");
    }
}
