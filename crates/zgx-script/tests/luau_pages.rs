//! With the pool on, Luau's pages live apart from the program's heap, and
//! going idle gives the free ones back. A process of its own: the pool is
//! the process's.
use mlua::{Lua, ffi};
use std::os::raw::c_void;
use std::sync::Mutex;

/// The tests count the process's one pool: one at a time.
static POOL: Mutex<()> = Mutex::new(());

/// `kSmallPageSize` in Luau's lmem.cpp.
const PAGE: usize = 16 * 1024 - 24;

fn sum_after_filling(lua: &Lua, n: u32) -> i64 {
    lua.load(format!(
        "t = {{}} for i = 1, {n} do t[i] = {{ i }} end
         local sum = 0 for _, v in t do sum += v[1] end
         return sum"
    ))
    .eval()
    .unwrap()
}

#[test]
fn free_pages_leave_and_come_back_holding_what_is_written() {
    let _pool = POOL.lock().unwrap();
    mlua::pool_pages(true);
    let lua = Lua::new();
    let n = 200_000;
    assert_eq!(
        sum_after_filling(&lua, n),
        i64::from(n) * (i64::from(n) + 1) / 2
    );
    lua.load("t = nil").exec().unwrap();
    lua.gc_collect().unwrap();
    lua.gc_collect().unwrap();
    let (free, _) = mlua::free_pages();
    assert!(free > 64, "{free} pages free after the collection");

    mlua::release_free_pages();
    let (free, released) = mlua::free_pages();
    assert_eq!(free, 0);
    assert!(released > 64, "{released} pages released");

    assert_eq!(
        sum_after_filling(&lua, n),
        i64::from(n) * (i64::from(n) + 1) / 2
    );
    assert!(
        mlua::free_pages().1 < released,
        "the VM took released pages again"
    );
}

#[test]
fn a_vm_made_with_the_pool_off_stays_out_of_it_when_it_turns_on() {
    let _pool = POOL.lock().unwrap();
    mlua::pool_pages(false);
    let lua = Lua::new();
    mlua::pool_pages(true);
    mlua::release_free_pages();
    let before = mlua::free_pages();
    let n = 50_000;
    assert_eq!(
        sum_after_filling(&lua, n),
        i64::from(n) * (i64::from(n) + 1) / 2
    );
    lua.load("t = nil").exec().unwrap();
    lua.gc_collect().unwrap();
    lua.gc_collect().unwrap();
    assert_eq!(mlua::free_pages(), before);
}

#[test]
fn a_block_resized_into_and_out_of_the_pool_keeps_its_bytes() {
    let _pool = POOL.lock().unwrap();
    mlua::pool_pages(true);
    let lua = Lua::new();
    unsafe {
        lua.exec_raw::<()>((), |state| {
            let mut extra = std::ptr::null_mut::<c_void>();
            let alloc = ffi::lua_getallocf(state, &mut extra);
            let block = alloc(extra, std::ptr::null_mut(), 0, 4096) as *mut u8;
            block.write_bytes(7, 4096);
            let page = alloc(extra, block as *mut c_void, 4096, PAGE) as *mut u8;
            assert_eq!(*page.add(4095), 7);
            page.add(PAGE - 1).write(9);
            let large = alloc(extra, page as *mut c_void, PAGE, PAGE + 4096) as *mut u8;
            assert_eq!((*large.add(4095), *large.add(PAGE - 1)), (7, 9));
            let page = alloc(extra, large as *mut c_void, PAGE + 4096, PAGE) as *mut u8;
            assert_eq!((*page.add(4095), *page.add(PAGE - 1)), (7, 9));
            alloc(extra, page as *mut c_void, PAGE, 0);
        })
        .unwrap();
    }
}
