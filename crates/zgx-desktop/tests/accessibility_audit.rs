//! The accessibility audit (`accessibility::audit`) over the widget gallery
//! and the Luau examples, headless: what a screen reader gets of each, and
//! where Tab goes, as they mount and with their toasts, menus, popovers,
//! dialogs and field errors showing. Every finding fails: a widget made
//! after this one that a reader cannot use fails here first.
//! `ZGX_AUDIT_JSON=1` prints each report, a line of JSON.
#[path = "../examples/support/gallery_view.rs"]
mod gallery_view;

use zgx::widgets::Ui;
use zgx_desktop::accessibility::audit::{Report, audit};
use zgx_script::{ScriptHost, testing::TestApp};

const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../zgx-script/examples");

fn check(name: &str, report: Report) {
    if std::env::var_os("ZGX_AUDIT_JSON").is_some() {
        println!("AUDIT {name} {}", report.to_json());
    }
    assert!(report.findings.is_empty(), "{name}: {}", report.to_json());
}

fn app(host: impl FnOnce(&zgx::reactive::Runtime) -> ScriptHost) -> TestApp {
    let app = TestApp::new(host);
    // Tall enough for each example's content without scrolling.
    app.ui.scene.borrow_mut().resize(1200., 2400.);
    app.frames(2);
    app
}
fn example(name: &str) -> TestApp {
    let path = format!("{EXAMPLES}/{name}/app.luau");
    let app = app(|runtime| ScriptHost::new(runtime, &path));
    assert_eq!(app.error(), None, "{name} failed to run");
    app
}

#[test]
fn the_gallery_is_usable_by_a_reader() {
    let mut ui = Ui::new(900., 760.);
    gallery_view::build(&mut ui, |_| {});
    check("gallery", audit(&mut ui));
}

#[test]
fn the_luau_examples_are_usable_by_a_reader() {
    for name in [
        "catalog",
        "dashboard",
        "mail",
        "chat",
        "team",
        "sketch",
        "code_editor",
        "notes",
        "popups",
        "selectable",
        "composer_states",
    ] {
        check(name, audit(&mut example(name).ui));
    }
}

/// What shows later: toasts (a status, and an error an alert), a menu, a
/// popover and a dialog, each over the catalogue.
#[test]
fn what_the_catalogue_opens_is_usable_by_a_reader() {
    for (state, target) in [
        ("toast", "toast"),
        ("error toast", "toast-error"),
        ("menu", "view-menu"),
        ("popover", "filters"),
        ("dialog", "open-dialog"),
    ] {
        let mut app = example("catalog");
        app.click(target);
        app.frames(2);
        check(&format!("catalog {state}"), audit(&mut app.ui));
    }
}

/// A form's fields wrong from the start: one showing its error under it,
/// one with nothing showing why (macos_ax_form's).
#[test]
fn a_reorderable_list_is_usable_by_a_reader() {
    let app = app(|runtime| {
        ScriptHost::inline(
            runtime,
            "app",
            r##"local folders = ui.state("folders", { "Inbox", "Work", "Personal" })
            return ui.column {
                ui.each(folders, function(name)
                    return ui.row { w = 160, h = 24, ui.text { name } }
                end, {
                    key = function(name) return name end,
                    reorder = function(from, to) end,
                }),
            }"##,
        )
    });
    assert_eq!(app.error(), None);
    let mut app = app;
    check("reorderable each", audit(&mut app.ui));
}

#[test]
fn an_invalid_form_says_why() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/macos_ax_form/app.luau"
    );
    let mut app = app(|runtime| ScriptHost::new(runtime, path));
    check("form", audit(&mut app.ui));
}

/// Each rule over a scene: a fixture breaking it is found, by its rule.
mod rules {
    use super::*;
    use zgx::{
        compose::prelude::*,
        input::{EventPhase, InputEvent},
        semantics::{Role, SemanticNode},
    };

    fn found(view: View) -> Vec<&'static str> {
        let mut ui = Ui::new(600., 400.);
        ui.mount(column().w(600.).h(400.).child(view));
        let report = audit(&mut ui);
        report.findings.iter().map(|f| f.rule).collect()
    }
    fn press(label: &str) -> View {
        button().w(80.).h(30.).child(text(label))
    }

    #[test]
    fn what_takes_focus_without_a_role_is_found() {
        assert_eq!(
            found(div().w(80.).h(30.).focusable(true).child(text("File"))),
            ["no-role"]
        );
        assert_eq!(
            found(
                div()
                    .w(80.)
                    .h(30.)
                    .focusable(true)
                    .role(Role::Button)
                    .accessible_label("File")
            ),
            Vec::<&str>::new()
        );
    }

    #[test]
    fn a_control_keeping_tab_and_escape_is_a_trap() {
        let keeper = press("Keeps keys").on_event(|cx| {
            if cx.phase != EventPhase::Bubble && matches!(cx.event, InputEvent::KeyDown { .. }) {
                cx.prevent_default();
                cx.stop_propagation();
            }
        });
        assert_eq!(
            found(
                column()
                    .child(press("Before"))
                    .child(keeper)
                    .child(press("After"))
            ),
            ["focus-trap"]
        );
    }

    #[test]
    fn a_control_tab_skips_is_unreachable() {
        // Arrows move between the group's buttons, but nothing tells a
        // reader so (it is no toolbar): the second is unreachable.
        let group = row()
            .focus_group(zgx::input::FocusGroup::Horizontal)
            .child(press("One"))
            .child(press("Two"));
        let mut ui = Ui::new(600., 400.);
        ui.mount(column().w(600.).h(400.).child(press("First")).child(group));
        let report = audit(&mut ui);
        let rules: Vec<_> = report
            .findings
            .iter()
            .map(|f| (f.rule, f.name.as_str()))
            .collect();
        assert_eq!(rules, [("focus-unreachable", "Two")]);
        // As a toolbar, the arrows are its own: a reader knows.
        let toolbar = row()
            .role(Role::Toolbar)
            .accessible_label("Format")
            .focus_group(zgx::input::FocusGroup::Horizontal)
            .child(press("One"))
            .child(press("Two"));
        assert_eq!(
            found(column().child(press("First")).child(toolbar)),
            Vec::<&str>::new()
        );
    }

    /// An alert is assertively live without saying so, as ARIA's is.
    #[test]
    fn an_alert_is_announced() {
        let mut ui = Ui::new(600., 400.);
        let view = ui.mount(
            div()
                .w(200.)
                .h(40.)
                .child(text("Disk full"))
                .role(Role::Alert),
        );
        let report = audit(&mut ui);
        assert_eq!(report.findings, []);
        let mut tree = zgx_desktop::accessibility::AccessibilityTree::new();
        let update = tree.update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "T", 1.);
        let alert = update
            .nodes
            .iter()
            .find(|(_, n)| n.role() == accesskit::Role::Alert)
            .unwrap();
        assert_eq!(alert.1.live(), Some(accesskit::Live::Assertive));
        let _ = view;
    }

    /// A button's unnamed icon is the button's (ARIA's presentational
    /// children); an unnamed image on its own is found.
    #[test]
    fn an_unnamed_image_is_found_unless_a_control_holds_it() {
        let mut ui = Ui::new(600., 400.);
        let view = ui.mount(
            column()
                .w(600.)
                .h(400.)
                .child(press("Send").child(div().id("icon").w(16.).h(16.)))
                .child(div().id("loose").w(16.).h(16.)),
        );
        for id in ["icon", "loose"] {
            ui.semantics
                .borrow_mut()
                .set(view.find(id).unwrap(), SemanticNode::new(Role::Image, ""));
        }
        let report = audit(&mut ui);
        let rules: Vec<_> = report.findings.iter().map(|f| (f.rule, f.node)).collect();
        assert_eq!(rules, [("no-name", view.find("loose"))]);
        assert_eq!(report.nodes, ui_nodes_without_icon(&mut ui));
    }
    fn ui_nodes_without_icon(ui: &mut Ui) -> usize {
        let mut tree = zgx_desktop::accessibility::AccessibilityTree::new();
        let update = tree.update(&ui.scene.borrow(), &ui.semantics.borrow(), None, "T", 1.);
        assert!(
            update
                .nodes
                .iter()
                .filter(|(_, n)| n.role() == accesskit::Role::Image)
                .count()
                == 1,
            "only the loose image is in the tree"
        );
        update.nodes.len()
    }
}
