use std::{cell::RefCell, rc::Rc};
use zgx::{
    compose::prelude::*,
    input::{InputEvent, Modifiers, PointerButton},
    scene::NodeId,
    semantics::Role,
    widgets::Ui,
};

/// What a link asks of the host: the URLs it opened, the text it copied.
#[derive(Default)]
struct Host {
    opened: RefCell<Vec<String>>,
    clipboard: RefCell<Option<String>>,
}
impl zgx::platform::Platform for Host {
    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.borrow().clone()
    }
    fn set_clipboard_text(&self, text: String) -> bool {
        *self.clipboard.borrow_mut() = Some(text);
        true
    }
    fn open_url(&self, url: &str) -> bool {
        self.opened.borrow_mut().push(url.to_owned());
        true
    }
    fn open_files(
        &self,
        _: zgx::platform::FilePicker,
    ) -> zgx::platform::Pending<Result<Vec<std::path::PathBuf>, String>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn save_file(
        &self,
        _: zgx::platform::FilePicker,
    ) -> zgx::platform::Pending<Result<Option<std::path::PathBuf>, String>> {
        Box::pin(async { Ok(None) })
    }
}

/// The links shown, left to right.
fn links(ui: &Ui) -> Vec<NodeId> {
    let mut links: Vec<_> = ui
        .semantics
        .borrow()
        .iter()
        .filter(|(_, s)| s.role == Role::Link)
        .map(|(id, _)| id)
        .collect();
    links.sort_by(|a, b| {
        let scene = ui.scene.borrow();
        scene.bounds(*a).x.total_cmp(&scene.bounds(*b).x)
    });
    links
}

fn click(ui: &mut Ui, link: NodeId, button: PointerButton, held: Modifiers) {
    let r = ui.scene.borrow().bounds(link);
    let (x, y) = (r.x + r.width / 2., r.y + r.height / 2.);
    ui.dispatch_with_modifiers(InputEvent::PointerDown { x, y, button }, held);
    ui.dispatch_with_modifiers(InputEvent::PointerUp { x, y, button }, held);
    ui.prepare_frame();
}

const COMMAND: Modifiers = Modifiers {
    shift: false,
    control: !cfg!(target_os = "macos"),
    alt: false,
    meta: cfg!(target_os = "macos"),
};

#[test]
fn a_click_on_an_href_opens_it_and_tells_its_action_the_keys_held() {
    let host = Rc::new(Host::default());
    let held = Rc::new(RefCell::new(Vec::new()));
    let seen = held.clone();
    let mut ui = Ui::new(300., 100.);
    ui.mount(provide(
        zgx::platform::PlatformService(host.clone()),
        rich_text()
            .text_size(16.)
            .child(text_span("Read "))
            .child(
                text_span("the docs")
                    .href("https://example.org/docs")
                    .on_click_with(move |keys| seen.borrow_mut().push(keys)),
            )
            .child(text_span(" or "))
            .child(text_span("the site").href("https://example.org"))
            .into(),
    ));
    ui.prepare_frame();
    let links = links(&ui);
    assert_eq!(links.len(), 2, "an href alone makes a link");
    let r = ui.scene.borrow().bounds(links[1]);
    assert_eq!(
        ui.scene
            .borrow()
            .cursor_at(r.x + r.width / 2., r.y + r.height / 2.),
        Some(zgx::cursor::Cursor::Pointer),
        "a link's hand"
    );
    click(
        &mut ui,
        links[0],
        PointerButton::Primary,
        Modifiers::default(),
    );
    assert_eq!(*host.opened.borrow(), ["https://example.org/docs"]);
    assert_eq!(*held.borrow(), [Modifiers::default()]);
    // A ⌘-click (Control elsewhere): the action knows, to open it aside.
    click(&mut ui, links[0], PointerButton::Primary, COMMAND);
    assert_eq!(held.borrow()[1], COMMAND);
    // A middle click passes as one, as a browser's opens a background tab.
    click(
        &mut ui,
        links[0],
        PointerButton::Middle,
        Modifiers::default(),
    );
    assert_eq!(held.borrow()[2], COMMAND);
    assert_eq!(host.opened.borrow().len(), 3);
    // Activated from the keyboard or assistive technology, it opens too.
    ui.input
        .dispatch_to(&ui.scene, links[1], InputEvent::Activate);
    assert_eq!(host.opened.borrow()[3], "https://example.org");
}

#[test]
fn an_hrefs_context_menu_opens_or_copies_it_and_a_plain_links_has_none() {
    let host = Rc::new(Host::default());
    let outer = Rc::new(RefCell::new(0));
    let counted = outer.clone();
    let mut ui = Ui::new(300., 100.);
    let view = ui.mount(provide(
        zgx::platform::PlatformService(host.clone()),
        column()
            .size(300., 100.)
            .context_menu(move |_| {
                *counted.borrow_mut() += 1;
                vec![menu_item("Outer")]
            })
            .child(View::from(
                rich_text()
                    .text_size(16.)
                    .child(text_span("site").href("https://example.org/a"))
                    .child(text_span(" and "))
                    .child(text_span("action").on_click(|| {})),
            )),
    ));
    ui.prepare_frame();
    let links = links(&ui);
    click(
        &mut ui,
        links[0],
        PointerButton::Secondary,
        Modifiers::default(),
    );
    assert_eq!(*outer.borrow(), 0, "the link's menu, not the view's");
    let copy = view.find("link-copy").expect("Copy Link shows");
    ui.input.dispatch_to(&ui.scene, copy, InputEvent::Activate);
    assert_eq!(
        host.clipboard.borrow().as_deref(),
        Some("https://example.org/a")
    );
    assert!(host.opened.borrow().is_empty(), "copying opens nothing");
    click(
        &mut ui,
        links[0],
        PointerButton::Secondary,
        Modifiers::default(),
    );
    let open = view.find("link-open").expect("Open Link shows");
    ui.input.dispatch_to(&ui.scene, open, InputEvent::Activate);
    assert_eq!(*host.opened.borrow(), ["https://example.org/a"]);
    // A link without an href: the press goes on to the view's menu.
    click(
        &mut ui,
        links[1],
        PointerButton::Secondary,
        Modifiers::default(),
    );
    assert_eq!(*outer.borrow(), 1);
}

#[test]
fn only_web_and_mail_links_open_through_the_system() {
    // Text often isn't the app's own: a `file:` link would launch what it
    // names, another scheme its handler. Its action still runs.
    let host = Rc::new(Host::default());
    let acted = Rc::new(RefCell::new(0));
    let count = acted.clone();
    let mut ui = Ui::new(400., 100.);
    let view = ui.mount(provide(
        zgx::platform::PlatformService(host.clone()),
        rich_text()
            .text_size(16.)
            .child(
                text_span("app")
                    .href("file:///Applications/Calculator.app")
                    .on_click(move || *count.borrow_mut() += 1),
            )
            .child(text_span(" "))
            .child(text_span("share").href("smb://server/share"))
            .child(text_span(" "))
            .child(text_span("mail").href("MAILTO:someone@example.org"))
            .child(text_span(" "))
            .child(text_span("web").href("HTTPS://example.org"))
            .into(),
    ));
    ui.prepare_frame();
    let links = links(&ui);
    for link in &links {
        click(&mut ui, *link, PointerButton::Primary, Modifiers::default());
    }
    assert_eq!(*acted.borrow(), 1, "the action ran");
    assert_eq!(
        *host.opened.borrow(),
        ["MAILTO:someone@example.org", "HTTPS://example.org"]
    );
    // Its menu copies it, but doesn't open it.
    click(
        &mut ui,
        links[0],
        PointerButton::Secondary,
        Modifiers::default(),
    );
    assert!(view.find("link-copy").is_some());
    assert!(view.find("link-open").is_none(), "no Open Link for a file");
}
