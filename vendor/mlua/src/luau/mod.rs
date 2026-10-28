//! Luau-specific extensions and types.
//!
//! This module provides Luau-specific functionality including custom [`require`] implementations,
//! heap memory analysis, and Luau VM integration utilities.
//!
//! [`require`]: crate::Lua::create_require_function

use std::ffi::{CStr, CString};
use std::os::raw::c_int;
use std::ptr;

use crate::chunk::ChunkMode;
use crate::error::{Error, Result};
use crate::function::Function;
use crate::state::{ExtraData, Lua, callback_error_ext};
use crate::traits::{FromLuaMulti, IntoLua};
use crate::types::MaybeSend;

pub use heap_dump::HeapDump;
pub use require::{FsRequirer, NavigateError, Require};

/// zgx patch: whether `data` is Luau bytecode, by the versions Luau 0.22
/// loads (`LBC_VERSION_MAX`, 14 in Luau 744). mlua-sys 0.13's
/// `luaL_isbytecode` takes versions up to 11, and Luau 744 writes 13: its
/// compiled chunks read as text there, and loading one as binary failed
/// ("attempt to load a text chunk (mode is 'b')").
pub(crate) fn is_bytecode(data: &[u8]) -> bool {
    const LBC_VERSION_MAX: u8 = 14;
    match *data {
        [] => false,
        // An error chunk (0) or a version before types (below '\t').
        [version, ..] if version < b'\t' => true,
        // From version 4, a types version follows: text never starts with
        // a tab, a newline or a carriage return followed by a control
        // character from 1 to 3.
        [version, types, ..] if version <= LBC_VERSION_MAX => (1..=3).contains(&types),
        _ => false,
    }
}

/// zgx patch: mlua-sys 0.13's `luaL_loadbufferenv`, line for line, but
/// for its bytecode check (`is_bytecode`): text is compiled and loaded,
/// bytecode loaded as it is, the mode allowing each.
#[allow(non_snake_case)]
pub(crate) unsafe fn luaL_loadbufferenv(
    L: *mut ffi::lua_State,
    data: *const std::os::raw::c_char,
    mut size: usize,
    name: *const std::os::raw::c_char,
    mode: *const std::os::raw::c_char,
    mut env: c_int,
) -> c_int {
    use std::os::raw::{c_char, c_void};
    unsafe extern "C" {
        fn free(p: *mut c_void);
    }

    unsafe extern "C" fn data_dtor(_: *mut ffi::lua_State, data: *mut c_void) {
        free(*(data as *mut *mut c_char) as *mut c_void);
    }

    let is_bytecode = is_bytecode(std::slice::from_raw_parts(data.cast(), size));
    if !mode.is_null() {
        let modeb = CStr::from_ptr(mode).to_bytes();
        let allow_binary = modeb.contains(&b'b');
        let allow_text = modeb.contains(&b't');
        if is_bytecode && !allow_binary {
            ffi::lua_pushfstring(L, cstr!("attempt to load a binary chunk (mode is '%s')"), mode);
            return ffi::LUA_ERRSYNTAX;
        } else if !is_bytecode && !allow_text {
            ffi::lua_pushfstring(L, cstr!("attempt to load a text chunk (mode is '%s')"), mode);
            return ffi::LUA_ERRSYNTAX;
        }
    }

    let status = if !is_bytecode {
        if env < 0 {
            env -= 1;
        }
        let data_ud = ffi::lua_newuserdatadtor(L, std::mem::size_of::<*mut c_char>(), data_dtor) as *mut *mut c_char;
        let data = ffi::luau_compile_(data, size, ptr::null_mut(), &mut size);
        ptr::write(data_ud, data);
        // By deferring the `free(data)` to the userdata destructor, we ensure that
        // even if `luau_load` throws an error, the `data` is still released.
        let status = ffi::luau_load(L, name, data, size, env);
        ffi::lua_replace(L, -2); // replace data with the result
        status
    } else {
        ffi::luau_load(L, name, data, size, env)
    };

    if status != 0 {
        if ffi::lua_isstring(L, -1) != 0 && CStr::from_ptr(ffi::lua_tostring(L, -1)) == c"not enough memory" {
            // A case for Luau >= 0.679
            return ffi::LUA_ERRMEM;
        }
        return ffi::LUA_ERRSYNTAX;
    }

    ffi::LUA_OK
}

#[cfg(feature = "luau-jit")]
pub(crate) fn init_jit_flags() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        // These process-global flags must be set before any VM or compiler uses them.
        let _ = Lua::set_fflag("LuauCallFeedback", true);
        let _ = Lua::set_fflag("LuauEmitCallFeedback", true);
        let _ = Lua::set_fflag("LuauCIProto", true);
        let _ = Lua::set_fflag("LuauPromoteProto", true);
        let _ = Lua::set_fflag("LuauVirtualBcBuilder", true);
    });
}

// Since Luau has some missing standard functions, we re-implement them here

impl Lua {
    /// Create a custom Luau `require` function using provided [`Require`] implementation to find
    /// and load modules.
    #[cfg(any(feature = "luau", doc))]
    #[cfg_attr(docsrs, doc(cfg(feature = "luau")))]
    pub fn create_require_function<R: Require + MaybeSend + 'static>(&self, require: R) -> Result<Function> {
        require::create_require_function(self, require)
    }

    /// Set the memory category for subsequent allocations from this Lua state.
    ///
    /// The category "main" is reserved for the default memory category.
    /// Maximum of 255 categories can be registered.
    /// The category is set per Lua thread (state) and affects all allocations made from that
    /// thread.
    ///
    /// Return error if too many categories are registered or if the category name is invalid.
    ///
    /// See [`Lua::heap_dump`] for tracking memory usage by category.
    #[cfg(any(feature = "luau", doc))]
    #[cfg_attr(docsrs, doc(cfg(feature = "luau")))]
    pub fn set_memory_category(&self, category: &str) -> Result<()> {
        let lua = self.lock();

        if category.contains(|c| !matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_')) {
            return Err(Error::runtime("invalid memory category name"));
        }
        let cat_id = unsafe {
            let extra = ExtraData::get(lua.state());
            match ((*extra).mem_categories.iter().enumerate())
                .find(|&(_, name)| name.as_bytes() == category.as_bytes())
            {
                Some((id, _)) => id as u8,
                None => {
                    let new_id = (*extra).mem_categories.len() as u8;
                    if new_id == 255 {
                        return Err(Error::runtime("too many memory categories registered"));
                    }
                    (*extra).mem_categories.push(CString::new(category).unwrap());
                    new_id
                }
            }
        };
        unsafe { ffi::lua_setmemcat(lua.state(), cat_id as i32) };

        Ok(())
    }

    /// Dumps the current Lua VM heap state.
    ///
    /// The returned `HeapDump` can be used to analyze memory usage.
    /// It's recommended to call [`Lua::gc_collect`] before dumping the heap.
    #[cfg(any(feature = "luau", doc))]
    #[cfg_attr(docsrs, doc(cfg(feature = "luau")))]
    pub fn heap_dump(&self) -> Result<HeapDump> {
        let lua = self.lock();
        unsafe { heap_dump::HeapDump::new(lua.state()).ok_or_else(|| Error::runtime("unable to dump heap")) }
    }

    pub(crate) unsafe fn configure_luau(&self) -> Result<()> {
        let globals = self.try_globals()?;

        globals.raw_set("collectgarbage", self.create_c_function(lua_collectgarbage)?)?;
        globals.raw_set("loadstring", self.create_c_function(lua_loadstring)?)?;

        // Set `_VERSION` global to include version number
        // The environment variable `LUAU_VERSION` set by the build script
        if let Some(version) = ffi::luau_version() {
            globals.raw_set("_VERSION", format!("Luau {version}"))?;
        }

        // Enable default `require` implementation
        let require = self.create_require_function(FsRequirer::new())?;
        globals.raw_set("require", require)?;

        Ok(())
    }
}

unsafe extern "C-unwind" fn lua_collectgarbage(state: *mut ffi::lua_State) -> c_int {
    let option = ffi::luaL_optstring(state, 1, cstr!("collect"));
    let option = CStr::from_ptr(option);
    let arg = ffi::luaL_optinteger(state, 2, 0);
    let is_sandboxed = (*ExtraData::get(state)).sandboxed;
    match option.to_str() {
        Ok("collect") if !is_sandboxed => {
            ffi::lua_gc(state, ffi::LUA_GCCOLLECT, 0);
            0
        }
        Ok("stop") if !is_sandboxed => {
            ffi::lua_gc(state, ffi::LUA_GCSTOP, 0);
            0
        }
        Ok("restart") if !is_sandboxed => {
            ffi::lua_gc(state, ffi::LUA_GCRESTART, 0);
            0
        }
        Ok("count") => {
            let kbytes = ffi::lua_gc(state, ffi::LUA_GCCOUNT, 0) as ffi::lua_Number;
            let kbytes_rem = ffi::lua_gc(state, ffi::LUA_GCCOUNTB, 0) as ffi::lua_Number;
            ffi::lua_pushnumber(state, kbytes + kbytes_rem / 1024.0);
            1
        }
        Ok("step") if !is_sandboxed => {
            let res = ffi::lua_gc(state, ffi::LUA_GCSTEP, arg as _);
            ffi::lua_pushboolean(state, res);
            1
        }
        Ok("isrunning") if !is_sandboxed => {
            let res = ffi::lua_gc(state, ffi::LUA_GCISRUNNING, 0);
            ffi::lua_pushboolean(state, res);
            1
        }
        _ => ffi::luaL_error(state, cstr!("collectgarbage called with invalid option")),
    }
}

unsafe extern "C-unwind" fn lua_loadstring(state: *mut ffi::lua_State) -> c_int {
    callback_error_ext(state, ptr::null_mut(), false, move |extra, nargs| {
        let rawlua = (*extra).raw_lua();
        let (chunk, chunk_name) =
            <(String, Option<String>)>::from_stack_args(nargs, 1, Some("loadstring"), rawlua)?;
        let chunk_name = chunk_name.as_deref().unwrap_or("=(loadstring)");
        (rawlua.lua())
            .load(chunk)
            .set_name(chunk_name)
            .set_mode(ChunkMode::Text)
            .into_function()?
            .push_into_stack(rawlua)?;
        Ok(1)
    })
}

mod heap_dump;
mod json;
mod require;

#[cfg(test)]
mod zgx_tests {
    use super::is_bytecode;

    #[test]
    fn bytecode_of_every_luau_0_22_version_is_bytecode_and_text_is_not() {
        // An error chunk, and versions before a types version follows.
        assert!(is_bytecode(&[0, b'e']));
        assert!(is_bytecode(&[3, 0, 0]));
        // Luau 740 writes 9 (a tab), Luau 744 writes 13 (a carriage
        // return), each with a types version from 1 to 3; mlua-sys 0.13
        // took up to 11.
        for version in 9..=14 {
            for types in 1..=3 {
                assert!(is_bytecode(&[version, types, 0]), "{version} {types}");
            }
        }
        // Text, even starting with a tab or a line break.
        assert!(!is_bytecode(b""));
        assert!(!is_bytecode(b"return 1"));
        assert!(!is_bytecode(b"\treturn 1"));
        assert!(!is_bytecode(b"\r\nreturn 1"));
        assert!(!is_bytecode(b"\n"));
        // Versions Luau 0.22 doesn't load.
        assert!(!is_bytecode(&[15, 3, 0]));
    }
}
