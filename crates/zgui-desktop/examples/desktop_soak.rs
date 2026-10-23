//! Real windows opened and closed for a while, for `scripts/desktop_soak.py`,
//! which judges whether what the process holds levels off. Each cycle opens
//! three windows (a large tree each, its menu and popover open: on macOS
//! they show in popup windows; a tooltip, shown by an in-app pointer move;
//! one window with a prompt sheet), then closes them (dismissing the
//! sheet), and prints the process's footprint, file descriptors, Mach ports
//! and threads.
//!
//! `desktop_soak [cycles]` (default 50) runs that many, then quits.
use std::time::Duration;
use zgui::{compose::prelude::*, input::InputEvent};
use zgui_desktop::{Application, PromptOptions, WindowOptions};

#[derive(Debug, Default)]
struct Process {
    footprint: u64,
    fds: usize,
    ports: usize,
    threads: usize,
}
#[cfg(target_os = "macos")]
fn process() -> Process {
    unsafe extern "C" {
        fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut std::ffi::c_void) -> i32;
        fn proc_pidinfo(
            pid: i32,
            flavor: i32,
            arg: u64,
            buffer: *mut std::ffi::c_void,
            size: i32,
        ) -> i32;
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
    #[repr(C)]
    struct Rusage {
        uuid: [u8; 16],
        fields: [u64; 10],
    }
    #[repr(C)]
    struct TaskInfo {
        sizes: [u64; 6],
        counts: [i32; 12],
    }
    let pid = unsafe { getpid() };
    let mut usage = Rusage {
        uuid: [0; 16],
        fields: [0; 10],
    };
    let footprint = unsafe {
        if proc_pid_rusage(pid, 0, (&mut usage as *mut Rusage).cast()) == 0 {
            usage.fields[7]
        } else {
            0
        }
    };
    let mut info = TaskInfo {
        sizes: [0; 6],
        counts: [0; 12],
    };
    let size = std::mem::size_of::<TaskInfo>() as i32;
    let threads = unsafe {
        // PROC_PIDTASKINFO; `pti_threadnum` is the tenth count.
        if proc_pidinfo(pid, 4, 0, (&mut info as *mut TaskInfo).cast(), size) == size {
            info.counts[9] as usize
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
        threads,
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
        threads: std::fs::read_dir("/proc/self/task").map_or(0, |d| d.count()),
    }
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn process() -> Process {
    Process::default()
}

/// One of a cycle's windows: rows with tooltips, a menu and a popover open
/// over them, and (`sheet`) a prompt sheet.
fn soaked(cx: &mut zgui_desktop::WindowContext, index: usize, sheet: bool) {
    let menu_open = cx.ui.signal(false);
    let pop_open = cx.ui.signal(false);
    let rows = (0..300).map(|i| {
        row()
            .id(format!("row-{i}"))
            .gap(6.)
            .child(text(format!("Row {i} of window {index}")))
            .child(button().child(text("Act")))
            .tooltip(format!("Tip {i}"))
    });
    cx.render(
        column()
            .w_full()
            .h_full()
            .p(12.)
            .gap(2.)
            .bg(rgb(0x1e2430))
            .text_color(rgb(0xffffff))
            .child(
                menu(
                    "Menu",
                    menu_open.clone(),
                    button().id("menu").child(text("Menu")),
                )
                .child(menu_item("One"))
                .child(menu_item("Two")),
            )
            .child(
                popover(
                    "Popover",
                    pop_open.clone(),
                    button().id("pop").child(text("Popover")),
                )
                .child(text("A popover")),
            )
            .child(scroll(cx.ui.signal(0.)).grow().children(rows)),
    );
    let ui = cx.ui.downgrade();
    let dialogs = cx.dialogs.clone();
    let tasks = cx.tasks.clone();
    let shown = cx.tasks.spawn_scoped(async move {
        zgui::timer::sleep(Duration::from_millis(120)).await;
        menu_open.set(true);
        pop_open.set(true);
        if let Some(mut ui) = ui.upgrade() {
            // In-app pointer events, not the person's pointer.
            ui.dispatch(InputEvent::PointerMove { x: 40., y: 120. });
        }
        if sheet {
            tasks.spawn(async move {
                let _ = dialogs
                    .prompt(PromptOptions::new(
                        "Soak",
                        "A sheet the window's closing dismisses",
                    ))
                    .await;
            });
        }
    });
    std::mem::forget(shown);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cycles: usize = std::env::args()
        .nth(1)
        .and_then(|n| n.parse().ok())
        .unwrap_or(50);
    Application::new()
        .window(WindowOptions {
            title: "Desktop soak".into(),
            width: 360.,
            height: 200.,
            ..Default::default()
        })
        .run(move |cx| {
            cx.render(
                div()
                    .p(16.)
                    .child(text("Soaking: opening and closing windows")),
            );
            let windows = cx.windows.clone();
            let main = cx.window.clone();
            let driver = cx.tasks.spawn_scoped(async move {
                for cycle in 0..cycles {
                    let opened: Vec<_> = (0..3)
                        .map(|index| {
                            windows.open(
                                WindowOptions {
                                    title: format!("Soak {cycle}.{index}"),
                                    width: 420.,
                                    height: 360.,
                                    ..Default::default()
                                },
                                move |cx| soaked(cx, index, index == 0),
                            )
                        })
                        .collect();
                    zgui::timer::sleep(Duration::from_millis(600)).await;
                    for window in opened {
                        window.close();
                    }
                    zgui::timer::sleep(Duration::from_millis(300)).await;
                    let p = process();
                    println!(
                        "soak cycle {cycle} footprint={} fds={} ports={} threads={}",
                        p.footprint, p.fds, p.ports, p.threads
                    );
                }
                main.close();
            });
            std::mem::forget(driver);
        })
}
