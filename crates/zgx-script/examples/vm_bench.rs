//! What a script's VM costs to set up, headless: one script of one text,
//! loaded again `VMS` times (50); each load makes a new VM and installs
//! `ui` into it. Count it under cachegrind at `VMS=0` and `VMS=50`: the
//! difference over 50 is one VM's setup. On macOS it prints the
//! instructions each load retired too.
//! Run: VMS=50 cargo run --release -p zgx-script --example vm_bench
use zgx_script::{ScriptHost, testing::TestApp};

/// Instructions this process has retired (macOS; 0 elsewhere).
fn instructions() -> u64 {
    #[cfg(target_os = "macos")]
    {
        #[repr(C)]
        struct Usage {
            uuid: [u8; 16],
            fields: [u64; 40],
        }
        unsafe extern "C" {
            fn proc_pid_rusage(pid: i32, flavor: i32, buffer: *mut Usage) -> i32;
        }
        let mut usage = Usage {
            uuid: [0; 16],
            fields: [0; 40],
        };
        // RUSAGE_INFO_V4: `ri_instructions` is the 30th field after the uuid.
        unsafe { proc_pid_rusage(std::process::id() as i32, 4, &mut usage) };
        usage.fields[29]
    }
    #[cfg(not(target_os = "macos"))]
    0
}

fn main() {
    let number = |name: &str, default: usize| {
        zgx::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let vms = number("VMS", 50);
    let dir = std::env::temp_dir().join(format!("zgx-vm-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let script =
        zgx::env::var("SCRIPT").unwrap_or_else(|_| "return ui.text { \"hello\" }\n".into());
    std::fs::write(dir.join("app.luau"), script).unwrap();
    let path = dir.join("app.luau");
    let app = TestApp::new(|runtime| ScriptHost::new(runtime, path));
    app.settle();
    let start = instructions();
    for _ in 0..vms {
        app.host.reload().unwrap();
        app.frame();
    }
    let retired = instructions() - start;
    assert_eq!(app.host.error().get(), None);
    let _ = std::fs::remove_dir_all(&dir);
    match retired {
        0 => println!("{vms} loads"),
        _ => println!(
            "{vms} loads: {} instructions each",
            retired / vms.max(1) as u64
        ),
    }
}
