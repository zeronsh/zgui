use std::{cell::Cell, cell::RefCell, rc::Rc};
use zgx::{
    compose::prelude::*,
    i18n::{ArgValue, ArgumentKind, Catalog, I18n, IssueKind, Pattern},
    scene::{Direction, NodeId, NodeKind},
    widgets::Ui,
};

const EN: &str = r#"{
  "inbox": { "count": "{n, plural, =0 {No messages} one {# message} other {# messages}}" },
  "greeting": "Hello, {name}!",
  "place": "{n, selectordinal, one {#st} two {#nd} few {#rd} other {#th}}",
  "pronoun": "{who, select, her {She} him {He} other {They}} replied",
  "only.en": "English only"
}"#;
const FR: &str = r#"{
  "inbox": { "count": "{n, plural, =0 {Aucun message} one {# message} other {# messages}}" },
  "greeting": "Bonjour, {name} !"
}"#;

fn catalog(runtime: &zgx::reactive::Runtime) -> I18n {
    let i18n = I18n::new(runtime, "en");
    i18n.insert("en", EN).unwrap();
    i18n.insert("fr", FR).unwrap();
    i18n.set_locale(Some("en"));
    i18n
}

/// The text of every text node under `root`.
fn texts(ui: &Ui, root: NodeId) -> Vec<String> {
    let scene = ui.scene.borrow();
    let mut todo = vec![root];
    let mut out = vec![];
    while let Some(n) = todo.pop() {
        if let NodeKind::Text { text, .. } = scene.kind(n) {
            out.push(text.to_string());
        }
        todo.extend_from_slice(scene.children(n));
    }
    out
}

#[test]
fn messages_format_plurals_selects_and_arguments() {
    let ui = Ui::new(100., 100.);
    let i18n = catalog(&ui.runtime);
    let count = |n: i32| i18n.t("inbox.count").arg("n", n).get();
    assert_eq!(count(0), "No messages");
    assert_eq!(count(1), "1 message");
    assert_eq!(count(5), "5 messages");
    assert_eq!(i18n.t("greeting").arg("name", "Ada").get(), "Hello, Ada!");
    assert_eq!(i18n.t("place").arg("n", 22).get(), "22nd");
    assert_eq!(i18n.t("place").arg("n", 13).get(), "13th");
    assert_eq!(i18n.t("pronoun").arg("who", "her").get(), "She replied");
    assert_eq!(i18n.t("pronoun").arg("who", "them").get(), "They replied");
}

#[test]
fn quotes_offsets_and_nesting_parse_as_icu_does() {
    let ui = Ui::new(100., 100.);
    let i18n = I18n::new(&ui.runtime, "en");
    i18n.insert(
        "en",
        r#"{
          "quote": "It''s '{literal}' text",
          "others": "{n, plural, offset:1 =0 {nobody} =1 {{who}} one {{who} and # other} other {{who} and # others}}",
          "nested": "{g, select, f {{n, plural, one {her # cat} other {her # cats}}} other {{n, plural, one {their # cat} other {their # cats}}}}"
        }"#,
    )
    .unwrap();
    assert_eq!(i18n.t("quote").get(), "It's {literal} text");
    let others = |n: i32| i18n.t("others").arg("n", n).arg("who", "Ada").get();
    assert_eq!(others(0), "nobody");
    assert_eq!(others(1), "Ada");
    assert_eq!(others(2), "Ada and 1 other");
    assert_eq!(others(4), "Ada and 3 others");
    let nested = i18n.t("nested").arg("g", "f").arg("n", 2).get();
    assert_eq!(nested, "her 2 cats");
}

#[test]
fn parse_errors_say_where() {
    let error = Pattern::parse("ok\n{n, plural, one {#}}").unwrap_err();
    assert_eq!((error.line, error.column), (2, 20), "{error:?}");
    assert!(error.message.contains("other"), "{error:?}");
    let error = Pattern::parse("{n, bogus}").unwrap_err();
    assert_eq!(error.line, 1);
    assert!(Pattern::parse("a } b").is_err());
    // A plural chooses among at most 255 cases.
    let many: String = (0..256).map(|i| format!("={i} {{x}} ")).collect();
    let error = Pattern::parse(&format!("{{n, plural, {many}other {{y}}}}")).unwrap_err();
    assert!(error.message.contains("at most 255"), "{error:?}");
    assert!(Pattern::parse("{n").is_err());
}

#[test]
fn a_catalog_flattens_keys_and_lists_their_arguments() {
    let (catalog, errors) = Catalog::parse(
        "en",
        r#"{ "a": { "b": "{n, plural, one {#} other {#}} {when, date, short}", "c": "{x, select, y {Y} other {Z}}" }, "bad": "{oops" }"#,
    );
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].key, "bad");
    let keys: Vec<_> = catalog.keys().collect();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0].0, "a.b");
    assert_eq!(keys[0].1[0].name, "n");
    assert_eq!(
        keys[0].1[0].kind,
        ArgumentKind::Plural(vec!["one".into(), "other".into()])
    );
    assert_eq!(keys[0].1[1].kind, ArgumentKind::Date);
    assert_eq!(
        keys[1].1[0].kind,
        ArgumentKind::Select(vec!["y".into(), "other".into()])
    );
}

#[test]
fn missing_keys_and_arguments_fall_back_and_are_told_once() {
    let ui = Ui::new(100., 100.);
    let i18n = catalog(&ui.runtime);
    let issues = Rc::new(RefCell::new(Vec::new()));
    let sink = issues.clone();
    i18n.on_issue(move |issue| sink.borrow_mut().push((issue.kind, issue.key.clone())));
    i18n.set_locale(Some("fr"));
    // French lacks it: English's.
    assert_eq!(i18n.t("only.en").get(), "English only");
    // Nobody has it: the key.
    assert_eq!(i18n.t("no.such").get(), "no.such");
    assert_eq!(i18n.t("no.such").get(), "no.such");
    // Not given: shown as it is named.
    assert_eq!(i18n.t("greeting").get(), "Bonjour, {name} !");
    assert_eq!(
        *issues.borrow(),
        [
            (IssueKind::MissingKey, "no.such".to_owned()),
            (IssueKind::MissingArgument, "greeting".to_owned()),
        ]
    );
}

#[test]
fn issues_found_before_a_listener_reach_it() {
    let ui = Ui::new(100., 100.);
    let i18n = I18n::new(&ui.runtime, "en");
    assert!(
        i18n.insert("en", r#"{ "ok": "fine", "bad": "{n, plural, one {x}}" }"#)
            .is_err()
    );
    let told = Rc::new(RefCell::new(Vec::new()));
    let sink = told.clone();
    i18n.on_issue(move |issue| sink.borrow_mut().push((issue.kind, issue.key.clone())));
    assert_eq!(*told.borrow(), [(IssueKind::Parse, "bad".to_owned())]);
    assert_eq!(i18n.t("ok").get(), "fine");
}

#[test]
fn the_locale_follows_what_is_asked_and_its_direction() {
    let ui = Ui::new(100., 100.);
    let i18n = catalog(&ui.runtime);
    i18n.insert("ar", r#"{ "greeting": "مرحبا {name}" }"#)
        .unwrap();
    i18n.set_locale(Some("fr-CA"));
    assert_eq!(&*i18n.current(), "fr");
    assert_eq!(i18n.direction(), Direction::Ltr);
    i18n.set_locale(Some("ar-EG"));
    assert_eq!(&*i18n.current(), "ar");
    assert_eq!(i18n.direction(), Direction::Rtl);
    assert_eq!(i18n.locales()[0], "en");
}

#[test]
fn a_switch_or_reload_redraws_only_the_text_that_changed() {
    let mut ui = Ui::new(400., 200.);
    let i18n = catalog(&ui.runtime);
    let count = ui.signal(3);
    let (greeted, counted) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let (g, c) = (greeted.clone(), counted.clone());
    let only = Rc::new(Cell::new(0));
    let o = only.clone();
    let n = count.clone();
    let view = ui.mount(
        column()
            .child(text_message(i18n.t("greeting").arg_with(
                "name",
                move || {
                    g.set(g.get() + 1);
                    ArgValue::Text("Ada".into())
                },
            )))
            .child(text_message(i18n.t("inbox.count").arg_with(
                "n",
                move || {
                    c.set(c.get() + 1);
                    ArgValue::Number(f64::from(n.get()))
                },
            )))
            .child({
                let message = i18n.t("only.en");
                text_signal(move || {
                    o.set(o.get() + 1);
                    message.get()
                })
            }),
    );
    ui.prepare_frame();
    let root = view.node();
    let mut shown = texts(&ui, root);
    shown.sort();
    assert_eq!(shown, ["3 messages", "English only", "Hello, Ada!"]);
    let (g0, c0) = (greeted.get(), counted.get());
    // An argument: that message alone.
    count.set(1);
    ui.prepare_frame();
    assert!(texts(&ui, root).contains(&"1 message".to_owned()));
    assert_eq!((greeted.get(), counted.get() - c0), (g0, 1));
    // A locale: what it changes, not the key French lacks.
    let only_before = only.get();
    i18n.set_locale(Some("fr"));
    ui.prepare_frame();
    assert!(texts(&ui, root).contains(&"Bonjour, Ada !".to_owned()));
    assert_eq!(
        only.get(),
        only_before,
        "a message the switch didn't change formats no more"
    );
    // A reload: the key taken out falls back to English.
    let (g1, c1) = (greeted.get(), counted.get());
    i18n.insert("fr", r#"{ "greeting": "Salut, {name} !" }"#)
        .unwrap();
    ui.prepare_frame();
    let shown = texts(&ui, root);
    assert!(shown.contains(&"Salut, Ada !".to_owned()), "{shown:?}");
    assert!(shown.contains(&"1 message".to_owned()), "{shown:?}");
    assert_eq!((greeted.get() - g1, counted.get() - c1), (1, 1));
}

#[test]
fn a_provided_catalog_reaches_components() {
    let mut ui = Ui::new(400., 200.);
    let i18n = catalog(&ui.runtime);
    let view = ui.mount(provide(
        i18n.clone(),
        component(|cx| {
            let i18n = cx.service::<I18n>();
            text_message(i18n.t("greeting").arg("name", "Lin"))
        }),
    ));
    ui.prepare_frame();
    assert_eq!(texts(&ui, view.node()), ["Hello, Lin!"]);
}

#[test]
fn plural_categories_cover_the_locale() {
    let categories = zgx::i18n::plural_categories("en", false);
    assert!(categories.contains(&"one") && categories.contains(&"other"));
}

#[cfg(feature = "i18n")]
#[test]
fn icu_data_gives_each_locale_its_plurals_and_numbers() {
    let ar = zgx::i18n::plural_categories("ar", false);
    for category in ["zero", "one", "two", "few", "many", "other"] {
        assert!(ar.contains(&category), "{ar:?}");
    }
    let ui = Ui::new(100., 100.);
    let i18n = catalog(&ui.runtime);
    i18n.insert("ar", r#"{ "inbox": { "count": "{n, plural, zero {لا رسائل} one {رسالة} two {رسالتان} few {# رسائل} many {# رسالة} other {# رسالة}}" } }"#)
        .unwrap();
    assert_eq!(
        i18n.t("inbox.count").arg("n", 12_345).get(),
        "12,345 messages"
    );
    i18n.set_locale(Some("fr"));
    // A narrow no-break space groups French thousands.
    assert_eq!(
        i18n.t("inbox.count").arg("n", 12_345).get(),
        "12\u{202f}345 messages"
    );
    i18n.set_locale(Some("ar"));
    assert_eq!(i18n.t("inbox.count").arg("n", 2).get(), "رسالتان");
    assert_eq!(i18n.t("inbox.count").arg("n", 0).get(), "لا رسائل");
}

#[cfg(feature = "i18n-dates")]
#[test]
fn dates_follow_the_locale_and_length() {
    let ui = Ui::new(100., 100.);
    let i18n = I18n::new(&ui.runtime, "en");
    i18n.insert(
        "en",
        r#"{ "on": "on {d, date, medium}", "long": "{d, date, long}" }"#,
    )
    .unwrap();
    i18n.set_locale(Some("en"));
    // 2026-10-28T12:00:00Z (UTC: no host has set a local offset).
    let day = ArgValue::Date(1_793_188_800_000.);
    assert_eq!(i18n.t("on").arg("d", day.clone()).get(), "on Oct 28, 2026");
    assert_eq!(i18n.t("long").arg("d", day).get(), "October 28, 2026");
}
