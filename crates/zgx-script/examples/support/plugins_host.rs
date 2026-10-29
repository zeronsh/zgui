//! The plugins example's host: its tasks (kept in Rust, shown to scripts
//! as the shared state `tasks`), the actions that change them, what it
//! exposes to plugins, and each plugin with what it is granted. The
//! example and its test (`tests/plugins_example.rs`) set the app up here.
use std::{cell::RefCell, path::Path, rc::Rc};
use zgx::reactive::Runtime;
use zgx_script::{Capability, Plugin, ScriptHost, Value};

/// How an action changes the tasks, given what the script passed.
type Change = Box<dyn Fn(&mut Vec<Task>, Value)>;

#[derive(Clone)]
struct Task {
    id: u32,
    title: String,
    done: bool,
}

/// The app in `examples/plugins`, its package `kit`, and its two plugins.
pub fn host(runtime: &Runtime) -> ScriptHost {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let app = dir.join("plugins");
    let host =
        ScriptHost::new(runtime, app.join("app.luau")).package("kit", dir.join("plugins_kit"));
    let tasks = Rc::new(RefCell::new(vec![
        Task {
            id: 1,
            title: "Write the guide".into(),
            done: false,
        },
        Task {
            id: 2,
            title: "Review the plugins".into(),
            done: true,
        },
        Task {
            id: 3,
            title: "Ship it".into(),
            done: false,
        },
    ]));
    // `tasks` is the host's: scripts see it as shared state, and change it
    // with the actions below.
    let shared = host.shared("tasks", Value::Nil);
    let show = {
        let (tasks, shared) = (tasks.clone(), shared.clone());
        move || {
            let json: Vec<String> = tasks
                .borrow()
                .iter()
                .map(|t| {
                    format!(
                        r#"{{"id":{},"title":{:?},"done":{}}}"#,
                        t.id, t.title, t.done
                    )
                })
                .collect();
            shared.set(Value::from_json(&format!("[{}]", json.join(","))).unwrap_or_default());
        }
    };
    show();
    let action = |name: &str, change: Change| {
        let (tasks, show) = (tasks.clone(), show.clone());
        host.action(name, move |arg| {
            change(&mut tasks.borrow_mut(), arg);
            show();
        });
    };
    action(
        "add",
        Box::new(|tasks, title| {
            if let Value::Str(title) = title {
                let id = tasks.iter().map(|t| t.id).max().unwrap_or(0) + 1;
                tasks.push(Task {
                    id,
                    title: title.to_string(),
                    done: false,
                });
            }
        }),
    );
    action(
        "complete",
        Box::new(|tasks, id| {
            if let Value::Num(id) = id {
                for task in tasks.iter_mut().filter(|t| f64::from(t.id) == id) {
                    task.done = true;
                }
            }
        }),
    );
    action("clear_done", Box::new(|tasks, _| tasks.retain(|t| !t.done)));
    // What plugins may see and do of the app's: its tasks, read only (a
    // plugin's write is an error), whether focus is on (theirs to set), and
    // completing a task. Adding and clearing stay the app's.
    host.shared("focus", false);
    host.expose_shared_read("tasks", None);
    host.expose_shared("focus", None);
    host.expose_action("complete", None);
    let plugins = app.join("plugins");
    host.set_plugins(vec![
        // Adds a button (open to every plugin) and filters the list.
        Plugin {
            name: "focus".into(),
            entry: plugins.join("focus/init.luau"),
            grants: vec![Capability::Override("#tasks".into())],
        },
        // Replaces the empty state and, trusted with it, the row module.
        Plugin {
            name: "stamps".into(),
            entry: plugins.join("stamps/init.luau"),
            grants: vec![
                Capability::Override("#empty".into()),
                Capability::Override("@app/components/task_row".into()),
            ],
        },
    ]);
    host
}
