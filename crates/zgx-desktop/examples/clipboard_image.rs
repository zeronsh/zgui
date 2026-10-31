//! Images through the system clipboard: Paste shows the clipboard's image,
//! Copy puts a gradient with a transparent edge there for other apps.
//! `--smoke` pastes, prints what it read and closes; `--copy` with it
//! copies too (`scripts/windows_clipboard_image_smoke.py`).
use std::{cell::RefCell, rc::Rc, sync::Arc};
use zgx::{compose::prelude::*, image::ImageData, platform::PlatformService};
use zgx_desktop::{Application, WindowOptions};

/// The image Copy puts on the clipboard: 64 by 48, red across, green down,
/// its left quarter fading from transparent.
fn gradient() -> ImageData {
    let (width, height) = (64, 48);
    let mut pixels = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let alpha = if x < 16 { x * 16 } else { 255 };
            pixels.extend([(x * 4) as u8, (y * 5) as u8, 160, alpha as u8]);
        }
    }
    ImageData::new(width as u32, height as u32, pixels).expect("a valid image")
}

/// FNV-1a over the pixels, for the smoke to compare.
fn digest(pixels: &[u8]) -> u64 {
    pixels.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke");
    let copies = std::env::args().any(|arg| arg == "--copy");
    Application::new()
        .window(WindowOptions {
            title: "zgx clipboard image".into(),
            width: 420.,
            height: 320.,
            ..Default::default()
        })
        .run(move |cx| {
            // What the window provides (its clipboard).
            let platform: Rc<RefCell<Option<Rc<PlatformService>>>> = Default::default();
            let found = platform.clone();
            let pasted = cx.ui.signal(None::<Arc<ImageData>>);
            let status = cx
                .ui
                .signal(String::from("Copy an image anywhere, then Paste."));
            // Each a task: the image is read (and written) off the UI thread.
            let paste = {
                let (platform, pasted, status) = (platform.clone(), pasted.clone(), status.clone());
                move || {
                    let reading = platform.borrow().as_ref().map(|p| p.0.clipboard_image());
                    let (pasted, status) = (pasted.clone(), status.clone());
                    async move {
                        let image = match reading {
                            Some(reading) => reading.await,
                            None => None,
                        };
                        match &image {
                            Some(image) => {
                                println!(
                                    "PASTED {}x{} {:016x} first={:?}",
                                    image.width(),
                                    image.height(),
                                    digest(image.pixels()),
                                    &image.pixels()[..4]
                                );
                                status.set(format!(
                                    "Pasted {} × {}",
                                    image.width(),
                                    image.height()
                                ));
                            }
                            None => {
                                println!("PASTED none");
                                status.set("No image on the clipboard".into());
                            }
                        }
                        pasted.set(image.map(Arc::new));
                    }
                }
            };
            let copy = {
                let (platform, status) = (platform.clone(), status.clone());
                move || {
                    let image = Arc::new(gradient());
                    let writing = platform
                        .borrow()
                        .as_ref()
                        .map(|p| p.0.set_clipboard_image(image.clone()));
                    let status = status.clone();
                    async move {
                        let written = match writing {
                            Some(writing) => writing.await,
                            None => false,
                        };
                        println!("COPIED written={written} {:016x}", digest(image.pixels()));
                        status.set(format!("Copied a 64 × 48 gradient: {written}"));
                    }
                }
            };
            let note = cx.ui.signal(String::new());
            let shown = pasted.clone();
            let blank = Arc::new(ImageData::new(1, 1, vec![0; 4]).expect("a valid image"));
            cx.render(
                column()
                    .p(20.)
                    .gap(12.)
                    .child(component(move |cx| {
                        *found.borrow_mut() = cx.try_service::<PlatformService>();
                        div()
                    }))
                    .child(
                        row()
                            .gap(8.)
                            .child(button().child("Paste image").on_click({
                                let (tasks, paste) = (cx.tasks.clone(), paste.clone());
                                move || tasks.spawn(paste())
                            }))
                            .child(button().child("Copy image").on_click({
                                let (tasks, copy) = (cx.tasks.clone(), copy.clone());
                                move || tasks.spawn(copy())
                            }))
                            // What hears a paste may read the clipboard
                            // itself; text starting "take" is taken.
                            .child(text_input("Note", note).w(120.).on_paste({
                                let platform = platform.clone();
                                move |text| {
                                    let read = platform
                                        .borrow()
                                        .as_ref()
                                        .and_then(|p| p.0.clipboard_text());
                                    let files = platform
                                        .borrow()
                                        .as_ref()
                                        .map(|p| p.0.clipboard_files())
                                        .unwrap_or_default();
                                    println!("HEARD text={text:?} read={read:?} files={files:?}");
                                    text.is_none_or(|text| text.starts_with("take"))
                                }
                            })),
                    )
                    .child(text_signal(move || status.get()))
                    .child(
                        image_signal("Pasted image", move || {
                            shown.get().unwrap_or_else(|| blank.clone())
                        })
                        .max_w(380.)
                        .max_h(200.),
                    ),
            );
            if smoke {
                let window = cx.window.clone();
                cx.tasks.spawn(async move {
                    zgx::timer::sleep(std::time::Duration::from_millis(500)).await;
                    paste().await;
                    if copies {
                        copy().await;
                    }
                    zgx::timer::sleep(std::time::Duration::from_millis(300)).await;
                    window.close();
                });
            }
            println!("READY");
        })
}
