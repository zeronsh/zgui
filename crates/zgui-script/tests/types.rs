//! The Luau type definitions (`types/zgui.d.luau`) cover what the engine
//! accepts: every style property, element option, event, `ui` and `anime`
//! function, method and field of the engine's objects, and animatable key.
//! Adding one to the engine without its type fails here.
//!
//! The definitions are written in `types/zgui.d.luau.in`, where element
//! props are `Merge<Props, { … }>`; luau-lsp gives up on that many type
//! function calls ("code is too complex"), so `zgui.d.luau` has them written
//! out. `ZGUI_BLESS=1 cargo test -p zgui-script --test tooling types::` writes it.

const SOURCE: &str = include_str!("../types/zgui.d.luau.in");
const DEFS: &str = include_str!("../types/zgui.d.luau");
const NODE: &str = include_str!("../src/node.rs");
const ELEMENT: &str = include_str!("../src/element.rs");
const VALUE: &str = include_str!("../src/value.rs");
const TERM: &str = include_str!("../src/term.rs");
const COMMAND: &str = include_str!("../src/command.rs");
const CANVAS: &str = include_str!("../src/canvas.rs");
const MOTION: &str = include_str!("../src/motion.rs");
const TARGET: &str = include_str!("../src/anime/target.rs");
const API: &str = include_str!("../src/anime/api.rs");
const DRAGGABLE: &str = include_str!("../src/anime/draggable.rs");

// The generated definitions ------------------------------------------------

/// One property of a table type: its key and its lines (comments above it
/// included).
#[derive(Clone)]
struct Entry {
    key: String,
    text: String,
}

/// Bracket depth of `code` (a line without its comment): `->` is not a
/// closing angle bracket.
fn depth(code: &str, angles: bool) -> i32 {
    let mut d = 0;
    let bytes = code.as_bytes();
    for (i, &c) in bytes.iter().enumerate() {
        match c {
            b'(' | b'{' | b'[' => d += 1,
            b')' | b'}' | b']' => d -= 1,
            b'<' if angles => d += 1,
            b'>' if angles && (i == 0 || bytes[i - 1] != b'-') => d -= 1,
            _ => {}
        }
    }
    d
}
fn code_of(line: &str) -> &str {
    match line.find("--") {
        Some(i) => &line[..i],
        None => line,
    }
}
/// A property's key, from its first line: `name`, `[number]`, `["repeat"]`.
fn key_of(line: &str) -> String {
    let line = line.trim().trim_start_matches("read ");
    if line.starts_with('[') {
        return line[..=line.find(']').expect("a closing bracket")].to_owned();
    }
    line[..line
        .find(':')
        .unwrap_or_else(|| panic!("a property: {line}"))]
        .to_owned()
}
/// The properties of a table body: lines between braces, or one line.
fn entries(body: &str) -> Vec<Entry> {
    let body = body.trim_matches('\n');
    if !body.contains('\n') {
        // `{ a: T, b: U }` on one line.
        return split_top(body)
            .into_iter()
            .map(|part| Entry {
                key: key_of(part),
                text: format!("\t{part},"),
            })
            .collect();
    }
    let mut out = Vec::new();
    let mut lines: Vec<&str> = Vec::new();
    let mut d = 0;
    for line in body.lines() {
        if line.trim().is_empty() {
            continue;
        }
        lines.push(line);
        if line.trim_start().starts_with("--") {
            continue;
        }
        d += depth(code_of(line), false);
        if d == 0 && code_of(line).trim_end().ends_with(',') {
            let first = lines
                .iter()
                .find(|l| !l.trim_start().starts_with("--"))
                .expect("a property line");
            out.push(Entry {
                key: key_of(first),
                text: lines.join("\n"),
            });
            lines.clear();
        }
    }
    assert!(lines.is_empty(), "a property without a comma: {lines:?}");
    out
}
/// `text`'s parts between top-level commas.
fn split_top(text: &str) -> Vec<&str> {
    let (mut parts, mut d, mut start) = (Vec::new(), 0, 0);
    let bytes = text.as_bytes();
    for (i, &c) in bytes.iter().enumerate() {
        match c {
            b'(' | b'{' | b'[' | b'<' => d += 1,
            b')' | b'}' | b']' => d -= 1,
            b'>' if i > 0 && bytes[i - 1] != b'-' => d -= 1,
            b',' if d == 0 => {
                parts.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts.retain(|p| !p.is_empty());
    parts
}
/// Splits `A, B` at its top-level comma.
fn split_args(args: &str) -> (&str, &str) {
    let bytes = args.as_bytes();
    let mut d = 0;
    for (i, &c) in bytes.iter().enumerate() {
        match c {
            b'(' | b'{' | b'[' | b'<' => d += 1,
            b')' | b'}' | b']' => d -= 1,
            b'>' if i > 0 && bytes[i - 1] != b'-' => d -= 1,
            b',' if d == 0 => return (&args[..i], &args[i + 1..]),
            _ => {}
        }
    }
    panic!("Merge takes two types: {args}")
}
fn resolve(expr: &str, bodies: &std::collections::HashMap<String, Vec<Entry>>) -> Vec<Entry> {
    let expr = expr.trim();
    if let Some(found) = bodies.get(expr) {
        return found.clone();
    }
    if let Some(args) = expr
        .strip_prefix("Merge<")
        .and_then(|e| e.strip_suffix('>'))
    {
        let (a, b) = split_args(args);
        let (a, b) = (resolve(a, bodies), resolve(b, bodies));
        // `[number]: T`, not `["repeat"]: T` (a property).
        let indexer = |e: &Entry| e.key.starts_with('[') && !e.key.starts_with("[\"");
        let replaced = b.iter().any(indexer);
        let mut out: Vec<Entry> = a
            .into_iter()
            .filter(|e| !(b.iter().any(|f| f.key == e.key) || replaced && indexer(e)))
            .collect();
        out.extend(b);
        return out;
    }
    if let Some(body) = expr.strip_prefix('{').and_then(|e| e.strip_suffix('}')) {
        return entries(body);
    }
    panic!("cannot expand `{expr}`: a table type defined above, Merge, or {{ … }}")
}
/// The definitions with every `export type X = Merge<…>` written out as the
/// table it makes.
fn expand(source: &str) -> String {
    use std::collections::HashMap;
    let mut bodies: HashMap<String, Vec<Entry>> = HashMap::new();
    let mut out = String::new();
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let named = line
            .strip_prefix("export type ")
            .and_then(|rest| rest.split_once(" = "))
            .filter(|(name, _)| name.chars().all(|c| c.is_alphanumeric() || c == '_'));
        match named {
            Some((name, rest)) if rest.starts_with("Merge<") => {
                // The statement runs until its brackets close.
                let mut statement = rest.to_owned();
                let mut d = depth(code_of(rest), true);
                while d > 0 {
                    i += 1;
                    statement.push('\n');
                    statement.push_str(lines[i]);
                    if !lines[i].trim_start().starts_with("--") {
                        d += depth(code_of(lines[i]), true);
                    }
                }
                let merged = resolve(&statement, &bodies);
                out.push_str(&format!("export type {name} = {{\n"));
                for entry in &merged {
                    out.push_str(&entry.text);
                    out.push('\n');
                }
                out.push_str("}\n");
                bodies.insert(name.to_owned(), merged);
            }
            Some((name, "{")) => {
                let start = i;
                while lines[i] != "}" {
                    i += 1;
                }
                let body = lines[start + 1..i].join("\n");
                bodies.insert(name.to_owned(), entries(&body));
                for l in &lines[start..=i] {
                    out.push_str(l);
                    out.push('\n');
                }
            }
            // A one-line record (`{ x: number }`, not `{ number }`).
            Some((name, rest))
                if rest.starts_with('{')
                    && rest.ends_with('}')
                    && split_top(&rest[1..rest.len() - 1])
                        .iter()
                        .all(|part| part.contains(':')) =>
            {
                bodies.insert(name.to_owned(), resolve(rest, &bodies));
                out.push_str(line);
                out.push('\n');
            }
            _ => {
                out.push_str(line);
                out.push('\n');
            }
        }
        i += 1;
    }
    out
}

#[test]
fn definitions_are_written_out_from_their_source() {
    let expanded = expand(SOURCE);
    if std::env::var_os("ZGUI_BLESS").is_some_and(|v| v != "0") {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/types/zgui.d.luau");
        std::fs::write(path, &expanded).unwrap();
        return;
    }
    assert!(
        !expanded
            .lines()
            .any(|l| l.starts_with("export type ") && l.contains(" = Merge<")),
        "every Merge is written out"
    );
    assert!(
        expanded == DEFS,
        "types/zgui.d.luau is not what zgui.d.luau.in makes: run \
         `ZGUI_BLESS=1 cargo test -p zgui-script --test tooling types::`"
    );
}

// Drift: what the engine reads, the definitions type ----------------------

/// The quoted names in `source` between `start` and the first `end` after it.
fn quoted(source: &str, start: &str, end: &str) -> Vec<String> {
    let from = source
        .find(start)
        .unwrap_or_else(|| panic!("`{start}` not found"));
    let rest = &source[from..];
    let to = rest.find(end).unwrap_or(rest.len());
    names_in(&rest[..to])
}
/// Every quoted identifier in `text`.
fn names_in(text: &str) -> Vec<String> {
    text.split('"')
        .skip(1)
        .step_by(2)
        .filter(|s| is_name(s))
        .map(str::to_owned)
        .collect()
}
fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !s.starts_with(|c: char| c.is_ascii_digit())
}
/// A type's body in the definitions: from `start` to its closing line.
fn body(start: &str) -> &'static str {
    let from = DEFS
        .find(start)
        .unwrap_or_else(|| panic!("`{start}` not in the definitions"));
    let rest = &DEFS[from..];
    let close = if start.starts_with("declare extern type") {
        "\nend"
    } else {
        "\n}"
    };
    &rest[..rest.find(close).expect("a closing line")]
}
/// The body of the table type `name` (`export type Name = {`), one
/// property a line.
fn table(name: &str) -> String {
    let start = format!("export type {name} = {{");
    let from = DEFS
        .find(&format!("{start}\n"))
        .or_else(|| DEFS.find(&format!("{start} ")))
        .unwrap_or_else(|| panic!("`{start}` not in the definitions"));
    let rest = &DEFS[from + start.len()..];
    let line = &rest[..rest.find('\n').unwrap()];
    if let Some(inline) = line.strip_suffix('}') {
        return entries(inline)
            .into_iter()
            .map(|e| e.text)
            .collect::<Vec<_>>()
            .join("\n");
    }
    rest[..rest.find("\n}").expect("a closing line")].to_owned()
}
fn has(within: &str, name: &str) -> bool {
    [
        format!("\t{name}: "),
        format!("\t[\"{name}\"]: "),
        format!("\tread {name}: "),
        format!("\tfunction {name}("),
    ]
    .iter()
    .any(|form| within.contains(form.as_str()))
}
fn missing(names: &[String], within: &str) -> Vec<String> {
    names.iter().filter(|n| !has(within, n)).cloned().collect()
}
/// `impl UserData for <ty>`'s block in `source`.
fn user_data<'a>(source: &'a str, ty: &str) -> &'a str {
    let from = source
        .find(&format!("impl UserData for {ty} {{"))
        .unwrap_or_else(|| panic!("no UserData for {ty}"));
    let rest = &source[from..];
    &rest[..rest.find("\n}\n").unwrap_or(rest.len())]
}
/// The names `add_method`, `add_field_method_get` and friends register.
fn registered(block: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in [
        "add_method(",
        "add_field_method_get(",
        "add_function(",
        "add_field_function_get(",
    ] {
        for chunk in block.split(marker).skip(1) {
            if let Some(name) = chunk
                .trim_start()
                .strip_prefix('"')
                .and_then(|s| s.split('"').next())
            {
                out.push(name.to_owned());
            }
        }
    }
    out
}

/// Every type the elements' constructors take: they all carry the styles.
const ELEMENT_PROPS: &[&str] = &[
    "Props",
    "ContainerProps",
    "ScrollProps",
    "TextProps",
    "RichProps",
    "ImageProps",
    "SvgProps",
    "CanvasProps",
    "CheckboxProps",
    "InputProps",
    "ProgressProps",
    "SliderProps",
    "ListProps",
    "ModalProps",
    "PopoverProps",
    "MenuItemProps",
    "SubmenuProps",
    "WebViewProps",
    "TerminalProps",
    "EachOptions",
    "PresenceOptions",
];

#[test]
fn style_properties_are_typed() {
    let props = quoted(NODE, "props! {\n", "\n}");
    assert!(
        props.len() > 90,
        "found the property table: {}",
        props.len()
    );
    for ty in ["Styles", "StyleProps"].iter().chain(ELEMENT_PROPS) {
        let gaps = missing(&props, &table(ty));
        assert!(gaps.is_empty(), "style properties not in {ty}: {gaps:?}");
    }
}

#[test]
fn events_and_node_options_are_typed() {
    // What `fill` reads itself, on every node: identity, events, layout
    // animation, masks, shaders, pseudo-states.
    let fill = &NODE[NODE.find("fn set_handler").expect("set_handler")..];
    let fill = &fill[..fill
        .find("name if WIDGET_FIELDS")
        .expect("fill's widget fields")];
    // The keys it matches: `"on_click" =>`, `"hover" | "active"`.
    let names: Vec<String> = fill
        .split('"')
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|w| is_name(w[0]) && (w[1].starts_with(" =>") || w[1].starts_with(" |")))
        .map(|w| w[0].to_owned())
        // `layout`'s values.
        .filter(|n| !["height", "width", "size", "position"].contains(&n.as_str()))
        .collect();
    assert!(names.len() > 30, "found fill's keys: {names:?}");
    for ty in ELEMENT_PROPS {
        let gaps = missing(&names, &table(ty));
        assert!(gaps.is_empty(), "node options not in {ty}: {gaps:?}");
    }
}

/// The keys `text` reads from tables: `get("key")`, `get::<T>("key")`,
/// `raw_get(…)`, and the `handler("key")` and `value("key")` closures.
fn gets(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for marker in [".get", ".raw_get", " handler", " value", "(value"] {
        for piece in text.split(marker).skip(1) {
            let rest = match piece.strip_prefix("::<") {
                // `get::<Option<String>>("key")`.
                Some(generic) => generic.split_once(">(\"").map(|(_, rest)| rest),
                None => piece.strip_prefix("(\""),
            };
            keys.extend(
                rest.and_then(|r| r.split('"').next())
                    .filter(|k| is_name(k))
                    .map(str::to_owned),
            );
        }
    }
    keys
}
/// What `ui.<name>`'s options can name: the table types its declaration
/// mentions, and inline table types.
fn options_of(name: &str) -> Option<(String, Vec<String>)> {
    let ui = body("declare ui: {");
    let line = ui
        .lines()
        .find(|l| l.starts_with(&format!("\t{name}: (")))?
        .to_owned();
    let tables: Vec<String> = line
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter_map(|word| {
            let start = format!("export type {word} = {{");
            (DEFS.contains(&format!("{start}\n")) || DEFS.contains(&format!("{start} ")))
                .then(|| table(word))
        })
        .collect();
    (line.contains("{ ") || !tables.is_empty()).then_some((line, tables))
}

#[test]
fn element_options_are_typed_on_their_constructors() {
    // Each constructor's own keys, in its `ui.set` (the names of a `for name
    // in [...]` loop share one), and those of the helpers it calls.
    let helpers: &[(&str, Vec<String>)] = &[
        ("label(&table)", vec!["label".into()]),
        ("widget(", vec!["label".into(), "value".into()]),
        ("web_options(", gets(fn_body(NODE, "fn web_options"))),
        (
            "each_motion(",
            gets(fn_body(MOTION, "pub(crate) fn each_motion")),
        ),
    ];
    let mut checked = 0;
    let chunks: Vec<&str> = NODE.split("ui.set(").collect();
    for (i, chunk) in chunks.iter().enumerate().skip(1) {
        let names: Vec<String> = match chunk.trim_start().strip_prefix('"') {
            Some(rest) => vec![rest.split('"').next().unwrap().to_owned()],
            None => {
                // `for name in ["image", "svg"]` (or `[("scroll", false), …]`).
                let before = chunks[i - 1];
                let list = &before[before.rfind("for ").expect("a loop of names")..];
                names_in(&list[..list.find('{').unwrap()])
            }
        };
        // Up to the next function: `ui.tween`'s shared closure is checked
        // below, not as `ui.shared`'s.
        let own = &chunk[..chunk.find("\n    let ").unwrap_or(chunk.len())];
        let mut keys = gets(own);
        for (call, extra) in helpers {
            if own.contains(call) {
                keys.extend(extra.iter().cloned());
            }
        }
        // A component's props are its own type (it reads their `id` and
        // `class`).
        if keys.is_empty() || names == ["component"] {
            continue;
        }
        for name in &names {
            let mut keys = keys.clone();
            if name == "image" {
                // Read with svg's (one constructor makes both): only svg
                // draws `code`.
                keys.retain(|k| k != "code");
            }
            if name == "svg" {
                // Likewise, only an image has a `fallback`.
                keys.retain(|k| k != "fallback");
            }
            let (line, tables) = options_of(name)
                .unwrap_or_else(|| panic!("ui.{name} reads {keys:?} but takes no table type"));
            let gaps: Vec<_> = keys
                .iter()
                .filter(|k| !tables.iter().any(|t| has(t, k)) && !line.contains(&format!(" {k}: ")))
                .collect();
            assert!(
                gaps.is_empty(),
                "ui.{name}'s options without types: {gaps:?}"
            );
            checked += 1;
        }
    }
    assert!(checked > 20, "checked {checked} constructors");
    // `ui.tween`, `ui.spring` and `ui.loop` share one closure.
    let anim = &NODE[NODE.find("let anim = |").expect("the bindings' closure")..];
    let anim = &anim[..anim.find("use crate::anime::ease").unwrap()];
    let typed = [
        table("TweenOptions"),
        table("SpringOptions"),
        table("LoopOptions"),
    ];
    let gaps: Vec<_> = gets(anim)
        .into_iter()
        .filter(|k| !typed.iter().any(|t| has(t, k)))
        .collect();
    assert!(gaps.is_empty(), "binding options without types: {gaps:?}");
    // Rich text runs' options.
    let gaps = missing(&gets(fn_body(NODE, "fn span(")), &table("SpanProps"));
    assert!(gaps.is_empty(), "span options: {gaps:?}");
    // Terminal sessions' and commands' options.
    let mut term = quoted(TERM, "const HANDLERS", "];");
    term.extend(quoted(TERM, "let known = matches!", ")"));
    let gaps = missing(&term, &table("TermOptions"));
    assert!(gaps.is_empty(), "ui.term options: {gaps:?}");
    let command = gets(fn_body(COMMAND, "pub(crate) fn install"));
    assert!(
        command.len() >= 6,
        "found ui.command's options: {command:?}"
    );
    let gaps = missing(&command, &table("CommandOptions"));
    assert!(gaps.is_empty(), "ui.command options: {gaps:?}");
}
/// A function's text in `source`, from its signature to its closing brace.
fn fn_body<'a>(source: &'a str, start: &str) -> &'a str {
    let from = source
        .find(start)
        .unwrap_or_else(|| panic!("`{start}` not found"));
    let rest = &source[from..];
    &rest[..rest.find("\n}").unwrap_or(rest.len())]
}

#[test]
fn ui_functions_are_typed() {
    let mut names = Vec::new();
    for source in [NODE, TERM, COMMAND, CANVAS] {
        for chunk in source.split("ui.set(").skip(1) {
            if let Some(name) = chunk
                .trim_start()
                .strip_prefix('"')
                .and_then(|s| s.split('"').next())
            {
                names.push(name.to_owned());
            }
        }
    }
    for list in [
        "(\"scroll\", false), (\"scroll_x\", true)",
        "\"image\", \"svg\"",
        "(\"input\", false), (\"textarea\", true)",
        "\"popover\", \"menu\"",
    ] {
        assert!(NODE.contains(list), "the constructor list `{list}` moved");
        names.extend(list.split('"').skip(1).step_by(2).map(str::to_owned));
    }
    assert!(names.iter().any(|n| n == "web"), "ui.web is found");
    let gaps = missing(&names, body("declare ui: {"));
    assert!(gaps.is_empty(), "ui functions without types: {gaps:?}");
}

#[test]
fn engine_objects_methods_and_fields_are_typed() {
    let cases: [(&str, &str, &str); 9] = [
        (VALUE, "State", "export type State<T> = {"),
        (ELEMENT, "ElementUd", "declare extern type Element with"),
        (ELEMENT, "RefUd", "declare extern type Ref with"),
        (NODE, "WebUd", "declare extern type Web with"),
        (NODE, "BitmapUd", "declare extern type Bitmap with"),
        (TERM, "TermUd", "declare extern type Term with"),
        (CANVAS, "Recorder", "declare extern type Canvas with"),
        (CANVAS, "PathUd", "declare extern type Path with"),
        (API, "EngineUd", "\tengine: {"),
    ];
    for (source, ty, declared) in cases {
        let names = registered(user_data(source, ty));
        assert!(!names.is_empty(), "found {ty}'s methods");
        let within = if declared.starts_with('\t') {
            let from = DEFS.find(declared).expect("anime.engine");
            &DEFS[from..from + DEFS[from..].find("\n\t},").unwrap()]
        } else {
            body(declared)
        };
        let names: Vec<String> = names.iter().map(|n| n.replace('\t', "")).collect();
        let gaps: Vec<String> = missing(&names, within)
            .into_iter()
            // Indented once more inside `engine: { … }`.
            .filter(|n| !within.contains(&format!("\t{n}: ")))
            .collect();
        assert!(gaps.is_empty(), "{ty}: untyped {gaps:?} (in `{declared}`)");
    }
    // Animations and draggables answer by name (`__index`).
    let index = &API[API.find("fn index(lua").expect("Animation's index")..];
    let index = &index[..index.find("fn call_method").unwrap()];
    let names: Vec<String> = names_in(index)
        .into_iter()
        // `then` is a keyword: `done` is its name.
        .filter(|n| n != "then" && n != "engine")
        .collect();
    let gaps = missing(&names, body("declare extern type Animation with"));
    assert!(gaps.is_empty(), "Animation: untyped {gaps:?}");
    let drag = quoted(DRAGGABLE, "impl UserData for DraggableUd", "\nfn axis");
    let drag: Vec<String> = drag.into_iter().filter(|n| n != "this").collect();
    let gaps = missing(&drag, body("declare extern type Draggable with"));
    assert!(gaps.is_empty(), "Draggable: untyped {gaps:?}");
    // Callbacks also answer to snake_case (`on_grab`); the types name them
    // as anime.js does.
    let params: Vec<String> = quoted(DRAGGABLE, "pub(crate) fn create", "\n}")
        .into_iter()
        .filter(|n| !n.starts_with("on_"))
        .collect();
    let gaps = missing(&params, &table("DraggableParams"));
    assert!(gaps.is_empty(), "createDraggable options: {gaps:?}");
}

#[test]
fn anime_functions_are_typed() {
    let install = &API[API.find("pub(crate) fn install").expect("anime's install")..];
    let mut names = Vec::new();
    for chunk in install.split("anime.set(").skip(1) {
        if let Some(name) = chunk
            .trim_start()
            .strip_prefix('"')
            .and_then(|s| s.split('"').next())
        {
            names.push(name.to_owned());
        }
    }
    assert!(names.len() > 12, "found anime's functions: {names:?}");
    let anime = body("declare anime: {");
    let gaps = missing(&names, anime);
    assert!(gaps.is_empty(), "anime functions without types: {gaps:?}");
    let mut utils = Vec::new();
    for chunk in API.split("utils.set(").skip(1) {
        if let Some(name) = chunk
            .trim_start()
            .strip_prefix('"')
            .and_then(|s| s.split('"').next())
        {
            utils.push(name.to_owned());
        }
    }
    let gaps: Vec<_> = utils
        .iter()
        .filter(|n| !anime.contains(&format!("\t\t{n}: ")))
        .collect();
    assert!(gaps.is_empty(), "anime.utils without types: {gaps:?}");
    let eases = quoted(API, "let names = [", "];");
    let gaps: Vec<_> = eases
        .iter()
        .filter(|n| {
            !anime.contains(&format!("\t\t{n}: ")) && !anime.contains(&format!("\t\t[\"{n}\"]: "))
        })
        .collect();
    assert!(gaps.is_empty(), "anime.eases without types: {gaps:?}");
}

#[test]
fn animatable_keys_are_typed() {
    let mut keys = quoted(TARGET, "pub fn for_element", "pub fn unit");
    // `shaderParam0` to `shaderParam15` share a prefix in the engine.
    assert!(
        keys.iter().any(|k| k == "shaderParam"),
        "the shader parameter keys moved"
    );
    keys.retain(|k| k != "shaderParam");
    keys.extend((0..16).map(|i| format!("shaderParam{i}")));
    for ty in ["ElementKeys", "ElementParams"] {
        let gaps = missing(&keys, &table(ty));
        assert!(gaps.is_empty(), "animatable keys not in {ty}: {gaps:?}");
    }
}

#[test]
fn standard_tokens_are_typed() {
    use zgui::theme::TokenKind;
    let union = |name: &str| {
        let from = DEFS
            .find(&format!("type {name} ="))
            .unwrap_or_else(|| panic!("`type {name}` not in the definitions"));
        let rest = &DEFS[from..];
        let end = ["\ntype ", "\nexport type "]
            .iter()
            .filter_map(|next| rest[1..].find(next))
            .min();
        &rest[..end.map_or(rest.len(), |i| i + 1)]
    };
    for (_, name, kind) in zgui::theme::standard() {
        let list = match kind {
            TokenKind::Color => "ColorToken",
            TokenKind::Number => "NumberToken",
            TokenKind::Font => "FontToken",
            TokenKind::Shadow => "ShadowToken",
            TokenKind::Text => "EaseToken",
        };
        assert!(
            union(list).contains(&format!("\"${name}\"")),
            "the token `{name}` is not in `{list}`"
        );
        assert!(
            union("TokenName").contains(&format!("\"{name}\"")),
            "the token `{name}` is not in `TokenName`"
        );
    }
}
