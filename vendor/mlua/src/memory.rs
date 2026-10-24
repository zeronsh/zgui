use std::alloc::{self, Layout};
use std::os::raw::c_void;
use std::ptr;

pub(crate) static ALLOCATOR: ffi::lua_Alloc = allocator;

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
            let layout = Layout::from_size_align_unchecked(osize, ffi::SYS_MIN_ALIGN);
            alloc::dealloc(ptr as *mut u8, layout);
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
        let new_ptr = alloc::alloc(new_layout) as *mut c_void;
        if new_ptr.is_null() {
            alloc::handle_alloc_error(new_layout);
        }
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
