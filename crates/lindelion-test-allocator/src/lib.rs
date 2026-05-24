use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

thread_local! {
    static ALLOCATION_COUNT: Cell<Option<usize>> = const { Cell::new(None) };
}

pub struct CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation();
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[macro_export]
macro_rules! install_test_allocator {
    () => {
        #[global_allocator]
        static LINDELION_TEST_ALLOCATOR: $crate::CountingAllocator = $crate::CountingAllocator;
    };
}

pub fn assert_no_allocations<R>(label: &str, run: impl FnOnce() -> R) -> R {
    let mut guard = AllocationCountGuard::start();
    let result = run();
    let allocations = guard.finish();

    assert_eq!(allocations, 0, "{label} allocated {allocations} time(s)");
    result
}

fn record_allocation() {
    ALLOCATION_COUNT.with(|count| {
        if let Some(value) = count.get() {
            count.set(Some(value + 1));
        }
    });
}

struct AllocationCountGuard {
    active: bool,
}

impl AllocationCountGuard {
    fn start() -> Self {
        ALLOCATION_COUNT.with(|count| {
            assert!(
                count.get().is_none(),
                "allocation counter is already active on this thread"
            );
            count.set(Some(0));
        });
        Self { active: true }
    }

    fn finish(&mut self) -> usize {
        let allocations = ALLOCATION_COUNT.with(|count| {
            let allocations = count.get().unwrap_or(0);
            count.set(None);
            allocations
        });
        self.active = false;
        allocations
    }
}

impl Drop for AllocationCountGuard {
    fn drop(&mut self) {
        if self.active {
            ALLOCATION_COUNT.with(|count| count.set(None));
        }
    }
}

#[cfg(test)]
install_test_allocator!();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assert_no_allocations_returns_closure_result() {
        let value = assert_no_allocations("return value", || 42);

        assert_eq!(value, 42);
    }

    #[test]
    fn assert_no_allocations_panics_on_allocation() {
        let result = std::panic::catch_unwind(|| {
            assert_no_allocations("vec allocation", || vec![1]);
        });

        assert!(result.is_err());
    }

    #[test]
    fn panic_during_counting_clears_counter() {
        let result = std::panic::catch_unwind(|| {
            assert_no_allocations("panic", || panic!("boom"));
        });

        assert!(result.is_err());
        assert_no_allocations("after panic", || ());
    }
}
