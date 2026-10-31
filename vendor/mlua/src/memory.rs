use std::alloc::{self, Layout};
use std::os::raw::c_void;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) static ALLOCATOR: ffi::lua_Alloc = allocator;

// zgx patch: Luau's 16 KB pages come from a pool of their own. In the
// program's heap a free page sits in a run of the allocator's, kept whole
// while one neighbour lives; here each slot goes back to the OS on its own
// (`release_free_pages`), so a heap that was once large stops costing memory.
#[cfg(all(feature = "luau", any(target_os = "macos", target_os = "linux")))]
mod pages {
    use std::ptr;
    use std::sync::Mutex;

    /// What Luau asks for a page of small objects (`kSmallPageSize`).
    pub(super) const SIZE: usize = 16 * 1024 - 24;
    const SLOT: usize = 16 * 1024;
    const CHUNK: usize = 2 << 20;

    /// The VM tag `footprint` shows these under (mimalloc's is 240).
    #[cfg(target_os = "macos")]
    const FD: i32 = 241 << 24;
    #[cfg(target_os = "linux")]
    const FD: i32 = -1;

    #[derive(Default)]
    struct Pool {
        /// Free, in memory: used first.
        ready: Vec<usize>,
        /// Free and given back to the OS.
        released: Vec<usize>,
    }
    static POOL: Mutex<Pool> = Mutex::new(Pool { ready: Vec::new(), released: Vec::new() });

    fn pool() -> std::sync::MutexGuard<'static, Pool> {
        POOL.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// A free slot, or null if the system has no memory for a new chunk.
    pub(super) unsafe fn take() -> *mut u8 {
        let mut pool = pool();
        if let Some(slot) = pool.ready.pop() {
            return slot as *mut u8;
        }
        if let Some(slot) = pool.released.pop() {
            // macOS counts a reusable page again only when told.
            #[cfg(target_os = "macos")]
            libc::madvise(slot as *mut _, SLOT, libc::MADV_FREE_REUSE);
            return slot as *mut u8;
        }
        let chunk = libc::mmap(
            ptr::null_mut(),
            CHUNK,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            FD,
            0,
        );
        if chunk == libc::MAP_FAILED {
            return ptr::null_mut();
        }
        let chunk = chunk as usize;
        pool.ready.extend((1..CHUNK / SLOT).rev().map(|i| chunk + i * SLOT));
        chunk as *mut u8
    }

    pub(super) unsafe fn give(slot: *mut u8) {
        pool().ready.push(slot as usize);
    }

    pub(super) fn release() {
        let mut pool = pool();
        let Pool { ready, released } = &mut *pool;
        // Neighbouring slots go in one call.
        ready.sort_unstable();
        let mut slots = ready.iter().copied().peekable();
        while let Some(start) = slots.next() {
            let mut end = start + SLOT;
            while slots.next_if_eq(&end).is_some() {
                end += SLOT;
            }
            #[cfg(target_os = "macos")]
            let advice = libc::MADV_FREE_REUSABLE;
            #[cfg(target_os = "linux")]
            let advice = libc::MADV_DONTNEED;
            unsafe { libc::madvise(start as *mut _, end - start, advice) };
        }
        released.append(ready);
    }

    pub(super) fn free() -> (usize, usize) {
        let pool = pool();
        (pool.ready.len(), pool.released.len())
    }
}

#[cfg(not(all(feature = "luau", any(target_os = "macos", target_os = "linux"))))]
mod pages {
    pub(super) const SIZE: usize = usize::MAX;
    pub(super) unsafe fn take() -> *mut u8 {
        std::ptr::null_mut()
    }
    pub(super) unsafe fn give(_: *mut u8) {}
    pub(super) fn release() {}
    pub(super) fn free() -> (usize, usize) {
        (0, 0)
    }
}

static POOL_PAGES: AtomicBool = AtomicBool::new(false);

/// zgx patch: whether VMs made from now on keep Luau's pages in the pool:
/// for a program whose allocator returns memory only in runs larger than
/// a page, as mimalloc does. Off, a VM allocates as any other.
pub fn pool_pages(on: bool) {
    POOL_PAGES.store(on, Ordering::Relaxed);
}

/// zgx patch: gives the memory of free Luau pages back to the OS, for every
/// VM of the process. For when the program goes idle: the next pages cost a
/// system call each.
pub fn release_free_pages() {
    pages::release();
}

/// zgx patch: free Luau pages, still in memory and given back to the OS.
#[doc(hidden)]
pub fn free_pages() -> (usize, usize) {
    pages::free()
}

unsafe fn allocate(pooled: bool, size: usize, layout: Layout) -> *mut c_void {
    let block = if pooled && size == pages::SIZE {
        pages::take()
    } else {
        alloc::alloc(layout)
    };
    if block.is_null() {
        alloc::handle_alloc_error(layout);
    }
    block as *mut c_void
}

unsafe fn release(pooled: bool, block: *mut c_void, size: usize) {
    if pooled && size == pages::SIZE {
        pages::give(block as *mut u8);
    } else {
        alloc::dealloc(block as *mut u8, Layout::from_size_align_unchecked(size, ffi::SYS_MIN_ALIGN));
    }
}

#[repr(C)]
#[derive(Default)]
pub(crate) struct MemoryState {
    used_memory: isize,
    memory_limit: isize,
    // Can be set to temporary ignore the memory limit.
    // This is used when calling `lua_pushcfunction` for lua5.1/jit/luau.
    ignore_limit: bool,
    // Indicates that the memory limit was reached on the last allocation.
    #[cfg(feature = "luau")]
    limit_reached: bool,
    // zgx patch: an allocation taking `used_memory` past `alarm_at` (when
    // above 0) calls `alarm` once (see `set_memory_alarm`).
    #[cfg(feature = "luau")]
    alarm_at: isize,
    #[cfg(feature = "luau")]
    alarm: Option<MemoryAlarm>,
    // zgx patch: the least of the limit and the alarm's threshold (0: none),
    // so an allocation under both is one comparison, as with the limit
    // alone (see `allocator`).
    check_at: isize,
    // zgx patch: this VM's pages come from the pool (`pool_pages`, when it
    // was made: a block is released as it was allocated).
    pooled: bool,
}

/// zgx patch: what an allocation past a VM's alarm threshold calls (see
/// [`set_memory_alarm`]): `call(data)`.
#[cfg(feature = "luau")]
#[derive(Clone, Copy)]
pub struct MemoryAlarm {
    /// Runs inside the allocation: it must not touch the VM (allocate,
    /// collect or raise an error) or unwind.
    pub call: unsafe fn(*const c_void),
    pub data: *const c_void,
}

/// zgx patch: once the VM of `state` holds more than `at` bytes (0: never),
/// its allocator calls `alarm`, once: the alarm is then cleared until set
/// again. A host collects garbage near its memory limit this way without a
/// check at every safepoint: the alarm (which must not touch the VM) sets
/// the interrupt, and the interrupt collects.
///
/// # Safety
///
/// `state` is a live Luau state made by mlua (with its allocator), called
/// on its thread while the VM is not allocating; `alarm.data` stays valid
/// for `alarm.call` as long as the alarm is set.
#[cfg(feature = "luau")]
pub unsafe fn set_memory_alarm(state: *mut ffi::lua_State, at: usize, alarm: Option<MemoryAlarm>) {
    let mem_state = &mut *MemoryState::get(state);
    mem_state.alarm_at = if alarm.is_some() {
        at.min(isize::MAX as usize) as isize
    } else {
        0
    };
    mem_state.alarm = alarm;
    mem_state.update_check_at();
}

impl MemoryState {
    // zgx patch: what a new VM starts with, its pages pooled or not.
    pub(crate) fn new() -> Self {
        Self {
            pooled: POOL_PAGES.load(Ordering::Relaxed),
            ..Self::default()
        }
    }

    #[cfg(feature = "luau")]
    #[inline]
    pub(crate) unsafe fn get(state: *mut ffi::lua_State) -> *mut Self {
        let mut mem_state = ptr::null_mut();
        ffi::lua_getallocf(state, &mut mem_state);
        mlua_assert!(!mem_state.is_null(), "Luau state has no allocator userdata");
        mem_state as *mut MemoryState
    }

    #[cfg(not(feature = "luau"))]
    #[inline]
    pub(crate) unsafe fn get(state: *mut ffi::lua_State) -> *mut Self {
        let mut mem_state = ptr::null_mut();
        if !ptr::fn_addr_eq(ffi::lua_getallocf(state, &mut mem_state), ALLOCATOR) {
            mem_state = ptr::null_mut();
        }
        mem_state as *mut MemoryState
    }

    #[inline]
    pub(crate) fn used_memory(&self) -> usize {
        self.used_memory as usize
    }

    #[inline]
    pub(crate) fn memory_limit(&self) -> usize {
        self.memory_limit as usize
    }

    #[inline]
    pub(crate) fn set_memory_limit(&mut self, limit: usize) -> usize {
        let prev_limit = self.memory_limit;
        self.memory_limit = limit.min(isize::MAX as usize) as isize;
        self.update_check_at();
        prev_limit as usize
    }

    // zgx patch: `check_at` again, after the limit or the alarm changed.
    fn update_check_at(&mut self) {
        #[cfg(feature = "luau")]
        let alarm_at = self.alarm_at;
        #[cfg(not(feature = "luau"))]
        let alarm_at = 0;
        self.check_at = match (self.memory_limit, alarm_at) {
            (0, at) | (at, 0) => at,
            (limit, at) => limit.min(at),
        };
    }

    // This function is used primarily for calling `lua_pushcfunction` in lua5.1/jit/luau
    // to bypass the memory limit (if set).
    #[cfg(any(feature = "lua51", feature = "luajit", feature = "luau"))]
    #[inline]
    pub(crate) unsafe fn relax_limit_with(state: *mut ffi::lua_State, f: impl FnOnce()) {
        let mem_state = Self::get(state);
        if !mem_state.is_null() {
            (*mem_state).ignore_limit = true;
            f();
            (*mem_state).ignore_limit = false;
        } else {
            f();
        }
    }

    // Does nothing apart from calling `f()`, we don't need to bypass any limits
    #[cfg(any(feature = "lua55", feature = "lua54", feature = "lua53", feature = "lua52"))]
    #[inline]
    pub(crate) unsafe fn relax_limit_with(_state: *mut ffi::lua_State, f: impl FnOnce()) {
        f();
    }

    // Returns `true` if the memory limit was reached on the last memory operation
    #[cfg(feature = "luau")]
    #[inline]
    pub(crate) unsafe fn limit_reached(state: *mut ffi::lua_State) -> bool {
        (*Self::get(state)).limit_reached
    }
}

unsafe extern "C" fn allocator(
    extra: *mut c_void,
    ptr: *mut c_void,
    osize: usize,
    nsize: usize,
) -> *mut c_void {
    let mem_state = &mut *(extra as *mut MemoryState);
    #[cfg(feature = "luau")]
    {
        // Reset the flag
        mem_state.limit_reached = false;
    }

    if nsize == 0 {
        // Free memory
        if !ptr.is_null() {
            release(mem_state.pooled, ptr, osize);
            mem_state.used_memory -= osize as isize;
        }
        return ptr::null_mut();
    }

    // Do not allocate more than isize::MAX
    if nsize > isize::MAX as usize {
        return ptr::null_mut();
    }

    // Are we fit to the memory limits?
    let mut mem_diff = nsize as isize;
    if !ptr.is_null() {
        mem_diff -= osize as isize;
    }
    let new_used_memory = mem_state.used_memory + mem_diff;
    // zgx patch: one comparison for the limit and the alarm (`check_at`).
    if mem_diff > 0 && mem_state.check_at > 0 && new_used_memory > mem_state.check_at {
        let mem_limit = mem_state.memory_limit;
        if mem_limit > 0 && new_used_memory > mem_limit && !mem_state.ignore_limit {
            #[cfg(feature = "luau")]
            {
                mem_state.limit_reached = true;
            }
            return ptr::null_mut();
        }
        // Past the alarm's threshold: the alarm, once.
        #[cfg(feature = "luau")]
        if mem_state.alarm_at > 0 && new_used_memory > mem_state.alarm_at {
            mem_state.alarm_at = 0;
            mem_state.update_check_at();
            if let Some(alarm) = mem_state.alarm {
                (alarm.call)(alarm.data);
            }
        }
    }
    mem_state.used_memory += mem_diff;

    if ptr.is_null() {
        // Allocate new memory
        let new_layout = match Layout::from_size_align(nsize, ffi::SYS_MIN_ALIGN) {
            Ok(layout) => layout,
            Err(_) => return ptr::null_mut(),
        };
        return allocate(mem_state.pooled, nsize, new_layout);
    }

    // A pooled block moves by hand: it is no block of the program's heap.
    if mem_state.pooled && (osize == pages::SIZE || nsize == pages::SIZE) {
        let new_layout = match Layout::from_size_align(nsize, ffi::SYS_MIN_ALIGN) {
            Ok(layout) => layout,
            Err(_) => return ptr::null_mut(),
        };
        let new_ptr = allocate(true, nsize, new_layout);
        ptr::copy_nonoverlapping(ptr as *const u8, new_ptr as *mut u8, osize.min(nsize));
        release(true, ptr, osize);
        return new_ptr;
    }

    // Reallocate memory
    let old_layout = Layout::from_size_align_unchecked(osize, ffi::SYS_MIN_ALIGN);
    let new_ptr = alloc::realloc(ptr as *mut u8, old_layout, nsize) as *mut c_void;
    if new_ptr.is_null() {
        alloc::handle_alloc_error(old_layout);
    }
    new_ptr
}
