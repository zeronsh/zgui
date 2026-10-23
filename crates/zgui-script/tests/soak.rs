//! Soak: an app used for a while keeps what it holds level. Each cycle
//! mounts a large tree (rows with tooltips and context menus), opens and
//! closes a menu, a popover and a modal, streams markdown, switches theme
//! and direction, draws on the GPU (when there is one), and every few
//! cycles hot-reloads its script; then everything closes again. After a
//! warm-up, what the document, the reactive runtime, the GPU caches, the
//! Luau heap and the process hold must stop growing.
//!
//! `ZGUI_SOAK_CYCLES` runs longer (the default is CI's few seconds):
//! `ZGUI_SOAK_CYCLES=2000 cargo test --release -p zgui-script --test soak
//! -- --nocapture` prints each sample.
#[path = "common/mod.rs"]
mod common;

use common::Harness;
use zgui::{input::InputEvent, scene::Direction};
use zgui_script::ScriptHost;

const APP: &str = r#"
local big = ui.state("big", false)
local menu = ui.state("menu", false)
local pop = ui.state("pop", false)
local modal = ui.state("modal", false)
local stream = ui.state("stream", "")
local tick = ui.state("tick", 0)

ui.command { id = "open", run = function()
	ui.batch(function()
		big:set(true)
		tick:set(tick:peek() + 1)
	end)
end }
ui.command { id = "panels", run = function()
	ui.batch(function()
		menu:set(true)
		pop:set(true)
	end)
end }
ui.command { id = "modal", run = function()
	ui.batch(function()
		menu:set(false)
		pop:set(false)
		modal:set(true)
	end)
end }
ui.command { id = "stream", run = function()
	stream:set(stream:peek() .. `- item **{tick:peek()}** with [a link](https://zgui.dev)\n`)
end }
ui.command { id = "close", run = function()
	ui.batch(function()
		big:set(false)
		menu:set(false)
		pop:set(false)
		modal:set(false)
		stream:set("")
	end)
end }

local function row(i: number): Node
	return ui.row {
		id = `row-{i}`,
		gap = 6,
		tooltip = `Tip {i}`,
		context_menu = function()
			return { ui.menu_item { "Copy" }, ui.menu_item { "Delete" } }
		end,
		ui.text { `Row {i}`, bold = i % 2 == 0 },
		ui.button { `Act {i}` },
		ui.text { function()
			return `tick {tick:get()}`
		end },
	}
end

local blocks = ui.computed(function()
	return ui.markdown(stream:get(), { key = "stream" })
end)
local function runs(list: { any }): Node
	local props: RichProps = { wrap = true }
	for _, r in list do
		table.insert(props, ui.span { r.x, literal = true, weight = if r.b then 600 else nil })
	end
	return ui.rich(props)
end

local anchor = ui.button { id = "anchor", "Anchor" }
return ui.column {
	gap = 4,
	anchor,
	ui.menu { open = menu, anchor = anchor, ui.menu_item { "One" }, ui.menu_item { "Two" } },
	ui.popover { open = pop, anchor = anchor, ui.text { "Popover" }, ui.input { label = "Field", value = ui.state("field", "") } },
	ui.modal { open = modal, ui.text { "Modal" }, ui.button { "OK" } },
	ui.presence(function()
		return if big:get() then "rows" else nil
	end, function()
		local rows = {}
		for i = 1, 300 do
			table.insert(rows, row(i))
		end
		return ui.column { gap = 2, table.unpack(rows) }
	end),
	ui.column {
		selectable = true,
		ui.each(blocks, function(b: MarkdownBlock)
			if b.t == "ul" then
				local items: { Child } = {}
				for _, item in b.items do
					table.insert(items, runs(item[1].s))
				end
				return ui.column { gap = 2, items }
			end
			return runs(b.s or {})
		end),
	},
}
"#;

/// What the process holds, where the platform says.
#[derive(Clone, Copy, Debug, Default)]
struct Process {
    footprint: u64,
    fds: usize,
    ports: usize,
}
#[cfg(target_os = "macos")]
fn process() -> Process {
    #[repr(C)]
    struct RusageV0 {
        uuid: [u8; 16],
        fields: [u64; 10],
    }
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
        fn getpid() -> i32;
        static mach_task_self_: u32;
        fn mach_port_names(
            task: u32,
            names: *mut *mut u32,
            names_count: *mut u32,
            types: *mut *mut u32,
            types_count: *mut u32,
        ) -> i32;
        fn vm_deallocate(task: u32, address: usize, size: usize) -> i32;
    }
    let mut usage = RusageV0 {
        uuid: [0; 16],
        fields: [0; 10],
    };
    let footprint = unsafe {
        if proc_pid_rusage(getpid(), 0, (&mut usage as *mut RusageV0).cast()) == 0 {
            usage.fields[7]
        } else {
            0
        }
    };
    let ports = unsafe {
        let (mut names, mut types) = (std::ptr::null_mut(), std::ptr::null_mut());
        let (mut count, mut types_count) = (0u32, 0u32);
        let task = mach_task_self_;
        if mach_port_names(task, &mut names, &mut count, &mut types, &mut types_count) == 0 {
            vm_deallocate(task, names as usize, count as usize * 4);
            vm_deallocate(task, types as usize, types_count as usize * 4);
            count as usize
        } else {
            0
        }
    };
    Process {
        footprint,
        fds: std::fs::read_dir("/dev/fd").map_or(0, |d| d.count()),
        ports,
    }
}
#[cfg(target_os = "linux")]
fn process() -> Process {
    let resident = std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|s| s.split_whitespace().nth(1)?.parse::<u64>().ok())
        .unwrap_or(0);
    Process {
        footprint: resident * 4096,
        fds: std::fs::read_dir("/proc/self/fd").map_or(0, |d| d.count()),
        ports: 0,
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process() -> Process {
    Process::default()
}

#[derive(Clone, Copy, Debug)]
struct Sample {
    nodes: usize,
    effects: usize,
    semantics: usize,
    luau: usize,
    converted: usize,
    gpu_cache: usize,
    gpu_device: u64,
    process: Process,
}

#[test]
fn an_app_used_for_a_while_holds_what_it_did() {
    let cycles: usize = std::env::var("ZGUI_SOAK_CYCLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(12);
    // Every so many cycles the script reloads (`ZGUI_SOAK_RELOAD`, 0: never).
    let reload_every: usize = std::env::var("ZGUI_SOAK_RELOAD")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let mut h = Harness::new(|runtime| ScriptHost::inline(runtime, "soak", APP));
    h.ui.scene.borrow_mut().resize(800., 600.);
    // The GPU, where there is one (a CI runner may have none).
    let mut gpu = zgui_gpu::GpuRenderer::new(800, 600).ok();
    if let Some(gpu) = &gpu {
        gpu.install_text(&mut h.ui.scene.borrow_mut());
        h.ui.refresh_text_geometry();
    }
    let draw = |h: &Harness, gpu: &mut Option<zgui_gpu::GpuRenderer>| {
        h.ui.prepare_frame();
        let mut scene = h.ui.scene.borrow_mut();
        let damage = scene.flush().damage;
        if let Some(gpu) = gpu {
            gpu.render(&scene, &damage).unwrap();
        }
    };
    let root = h.ui.root();
    let mut samples = Vec::new();
    for cycle in 0..cycles {
        assert!(h.host.run_command("open"));
        h.frames(2);
        draw(&h, &mut gpu);
        // A row's tooltip (the first waits its delay; then they're warm).
        let at = h.bounds(&format!("row-{}", cycle % 20 + 1));
        h.ui.dispatch(InputEvent::PointerMove {
            x: at.x + 4.,
            y: at.y + at.height / 2.,
        });
        let tip = format!("Tip {}", cycle % 20 + 1);
        assert!(h.wait_until(|h| h.texts().contains(&tip)), "{tip} shows");
        h.ui.dispatch(InputEvent::PointerMove { x: 790., y: 590. });
        assert!(h.host.run_command("panels"));
        h.frames(3);
        draw(&h, &mut gpu);
        assert!(h.host.run_command("modal"));
        h.frames(3);
        for _ in 0..4 {
            assert!(h.host.run_command("stream"));
            h.frames(1);
        }
        // Theme and direction, then back.
        h.ui.theme()
            .set(if cycle % 2 == 0 {
                "zgui-light"
            } else {
                "zgui-dark"
            })
            .unwrap();
        h.ui.scene.borrow_mut().set_direction(
            root,
            Some(if cycle % 2 == 0 {
                Direction::Rtl
            } else {
                Direction::Ltr
            }),
        );
        h.frames(2);
        draw(&h, &mut gpu);
        assert!(h.host.run_command("close"));
        h.frames(30);
        if reload_every > 0 && cycle % reload_every == reload_every - 1 {
            h.host.reload().unwrap();
            h.frames(2);
        }
        h.ui.theme().set("zgui-dark").unwrap();
        h.ui.scene
            .borrow_mut()
            .set_direction(root, Some(Direction::Ltr));
        h.frames(30);
        draw(&h, &mut gpu);
        let gpu_cache = gpu.as_ref().map_or(0, |gpu| {
            let s = gpu.debug_cache_stats();
            s.shaped_bytes
                + s.atlas_bytes
                + s.image_bytes
                + s.canvas_raster_bytes
                + s.svg_raster_bytes
                + s.vertex_buffer_bytes
                + s.layer_bytes
        });
        samples.push(Sample {
            nodes: h.ui.scene.borrow().len(),
            effects: h.ui.runtime.effect_count(),
            semantics: h.ui.semantics.borrow().iter().count(),
            luau: h.host.script_memory(),
            converted: h.host.converted_values(),
            gpu_cache,
            gpu_device: gpu
                .as_ref()
                .and_then(|g| g.debug_device_allocated_bytes())
                .unwrap_or(0),
            process: process(),
        });
        eprintln!("{cycle}: {:?}", samples.last().unwrap());
    }
    assert_eq!(h.error(), None);
    // After a warm-up (caches filling, the allocator settling), what it
    // holds stops growing: counts are those of a cycle a reload period
    // earlier, and sizes stay within what the warm-up reached.
    let period = reload_every.max(1);
    let warm = (cycles / 3).max(2 * period).min(cycles);
    let max = |f: fn(&Sample) -> u64| samples[..warm].iter().map(f).max().unwrap_or(0);
    let (luau, converted, cache, device) = (
        max(|s| s.luau as u64),
        max(|s| s.converted as u64),
        max(|s| s.gpu_cache as u64),
        max(|s| s.gpu_device),
    );
    let (footprint, fds, ports) = (
        max(|s| s.process.footprint),
        max(|s| s.process.fds as u64),
        max(|s| s.process.ports as u64),
    );
    for (cycle, sample) in samples.iter().enumerate().skip(warm) {
        let then = &samples[cycle - period];
        let at = format!("cycle {cycle}: {sample:?}, a period before: {then:?}");
        assert_eq!(sample.nodes, then.nodes, "scene nodes, {at}");
        assert_eq!(sample.effects, then.effects, "effects, {at}");
        assert_eq!(sample.semantics, then.semantics, "semantic nodes, {at}");
        assert!(sample.luau as u64 <= luau + luau / 8, "Luau heap, {at}");
        assert!(
            sample.converted as u64 <= converted,
            "converted values, {at}"
        );
        assert!(
            sample.gpu_cache as u64 <= cache + (64 << 10),
            "GPU caches, {at}"
        );
        assert!(sample.gpu_device <= device, "GPU allocations, {at}");
        assert!(sample.process.fds as u64 <= fds, "file descriptors, {at}");
        assert!(sample.process.ports as u64 <= ports + 2, "Mach ports, {at}");
        assert!(
            sample.process.footprint <= footprint + (32 << 20),
            "physical footprint, {at}"
        );
    }
}
