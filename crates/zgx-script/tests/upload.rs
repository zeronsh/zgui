//! Files sent with `ui.fetch` (`file`, `multipart`), read off the window's
//! thread, and an upload's progress (`on_progress`): told by a transport
//! as it writes (`Body::sent`), or all at once at the answer from one that
//! doesn't.
mod common;
use common::Harness;
use std::{
    cell::RefCell,
    path::PathBuf,
    rc::Rc,
    time::{Duration, Instant},
};
use zgx_script::{Request, Response, ScriptHost};

/// `bytes` as `photo.png`, in a folder of the test's own (`test`).
fn file(test: &str, bytes: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("zgx-upload-{}-{test}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("photo.png");
    std::fs::write(&path, bytes).unwrap();
    path
}

fn app(photo: &std::path::Path, streaming: bool) -> (Harness, Rc<RefCell<Vec<Request>>>) {
    let script = format!(
        r#"
local photo = {{ path = "{}", name = "photo.png" }}
local shown = ui.state("shown", "")
local progress = ui.state("progress", "")
local function report(sent, total)
    progress:set(`{{progress:peek()}}{{sent}}/{{total}};`)
end
return ui.column {{
    ui.text {{ id = "shown", "{{shown}}" }},
    ui.text {{ id = "progress", "{{progress}}" }},
    ui.button {{ id = "raw", w = 40, h = 20, "r", on_click = function()
        ui.task(function()
            local r = ui.fetch("https://up.test/raw", {{ file = photo, on_progress = report }}):await()
            shown:set(`raw {{r.status}}`)
        end)
    end }},
    ui.button {{ id = "form", w = 40, h = 20, "f", on_click = function()
        ui.task(function()
            local r = ui.fetch("https://up.test/form", {{
                multipart = {{ caption = "Hi", attachment = photo }},
            }}):await()
            shown:set(`form {{r.status}}`)
        end)
    end }},
}}
"#,
        photo.display()
    );
    let asked: Rc<RefCell<Vec<Request>>> = Rc::default();
    let seen = asked.clone();
    let h = Harness::new(move |runtime| {
        let host = ScriptHost::inline(runtime, "app", script);
        if streaming {
            host.streaming_fetcher(move |request, body| {
                let seen = seen.clone();
                async move {
                    let total = request.body.as_ref().map_or(0, |b| b.len() as u64);
                    seen.borrow_mut().push(request);
                    body.sent(total / 2, total);
                    zgx::timer::sleep(Duration::from_millis(2)).await;
                    body.sent(total, total);
                    Ok(Response {
                        status: 201,
                        ..Response::default()
                    })
                }
            });
        } else {
            host.fetcher(move |request| {
                let seen = seen.clone();
                async move {
                    seen.borrow_mut().push(request);
                    zgx::timer::sleep(Duration::from_millis(2)).await;
                    Ok(Response {
                        status: 200,
                        ..Response::default()
                    })
                }
            });
        }
        host
    });
    (h, asked)
}

fn wait_for(h: &mut Harness, id: &str, text: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        h.settle();
        h.ui.prepare_frame();
        if h.text(id) == text {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("`{id}` reads {:?}, not {text:?}", h.text(id));
}

#[test]
fn a_file_is_the_body_with_its_type_and_progress_comes_at_the_answer() {
    let photo = file("raw", &[1, 2, 3, 4, 5, 6, 7, 8]);
    let (mut h, asked) = app(&photo, false);
    h.click("raw");
    wait_for(&mut h, "shown", "raw 200");
    let request = asked.borrow()[0].clone();
    assert_eq!(request.method, "POST");
    assert_eq!(request.body.as_deref(), Some(&[1, 2, 3, 4, 5, 6, 7, 8][..]));
    assert!(
        request
            .headers
            .contains(&("content-type".into(), "image/png".into()))
    );
    assert_eq!(h.text("progress"), "8/8;");
}

#[test]
fn a_streaming_transport_tells_the_progress_as_it_writes() {
    let photo = file("streaming", &[0; 10]);
    let (mut h, _) = app(&photo, true);
    h.click("raw");
    wait_for(&mut h, "shown", "raw 201");
    assert_eq!(h.text("progress"), "5/10;10/10;");
}

#[test]
fn a_multipart_form_carries_text_and_files() {
    let photo = file("form", b"PNGDATA");
    let (mut h, asked) = app(&photo, false);
    h.click("form");
    wait_for(&mut h, "shown", "form 200");
    let request = asked.borrow()[0].clone();
    let kind = request
        .headers
        .iter()
        .find(|(name, _)| name == "content-type")
        .map(|(_, value)| value.clone())
        .unwrap();
    let boundary = kind
        .strip_prefix("multipart/form-data; boundary=")
        .expect(&kind)
        .to_string();
    let body = String::from_utf8(request.body.unwrap()).unwrap();
    assert_eq!(
        body,
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"attachment\"; filename=\"photo.png\"\r\nContent-Type: image/png\r\n\r\nPNGDATA\r\n\
             --{boundary}\r\nContent-Disposition: form-data; name=\"caption\"\r\n\r\nHi\r\n\
             --{boundary}--\r\n"
        )
    );
}
