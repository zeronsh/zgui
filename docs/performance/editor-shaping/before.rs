use std::{cell::Cell, rc::Rc};
use zgx::{input::{InputEvent, Key, Modifiers}, text_layout::{FallbackTextLayout, FontStyle, TextLayout}, widgets::{EditorHandle, Ui}};

fn fixture() -> (Ui, EditorHandle, Rc<Cell<usize>>) {
    let mut ui = Ui::new(400., 300.);
    let calls = Rc::new(Cell::new(0));
    let counted = calls.clone();
    ui.scene.borrow_mut().set_font_text_shaper(move |text: &str, size, width, font: &FontStyle| -> Box<dyn TextLayout> {
        counted.set(counted.get()+1);
        Box::new(FallbackTextLayout::with_line_height(text,size,width,font.line_height))
    });
    let value = ui.signal("abcdefghijabcdefghij\nx\nabcdefghijabcdefghij\nlast\n".repeat(4));
    let editor = ui.text_input(ui.root(),"Editor",value,100.,true);
    editor.set_typography(ui.theme.text,10.,FontStyle::default());
    editor.set_wrap(true);
    ui.prepare_frame();
    ui.input.focus(&ui.scene,Some(editor.node));
    ui.prepare_frame();
    calls.set(0);
    (ui,editor,calls)
}
fn key(ui: &mut Ui, key: Key) {
    ui.dispatch(InputEvent::KeyDown{key,modifiers:Modifiers::default(),repeat:false});
    ui.prepare_frame();
}
#[test]
fn baseline_counts() {
    let (mut ui,editor,calls)=fixture();
    for _ in 0..5 { editor.refresh(); ui.prepare_frame(); }
    println!("BASELINE refresh5={}",calls.replace(0));
    for i in [1,2,3] {editor.editor.borrow_mut().set_selection(0,i);editor.refresh();ui.prepare_frame();}
    println!("BASELINE selection3={}",calls.replace(0));
    for _ in 0..3 {ui.dispatch(InputEvent::Scroll{x:20.,y:20.,delta_x:0.,delta_y:14.});ui.prepare_frame();}
    println!("BASELINE scroll3={}",calls.replace(0));
    for k in [Key::End,Key::ArrowDown,Key::Home,Key::ArrowUp]{key(&mut ui,k);}
    println!("BASELINE navigation4={}",calls.get());
}
