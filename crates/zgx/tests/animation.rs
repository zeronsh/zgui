use std::{cell::RefCell, num::NonZeroU32, rc::Rc, sync::Arc, time::Duration};
use zgx::{
    animation::{Animation, Frame, LoopCount},
    compose::{TaskRunner, prelude::*},
    image::ImageData,
    scene::{Effects, Transform},
    task::LocalExecutor,
    widgets::Ui,
};
fn animation(loops: LoopCount) -> Arc<Animation> {
    Arc::new(
        Animation::new(
            vec![
                Frame {
                    image: Arc::new(ImageData::new(1, 1, vec![255, 0, 0, 255]).unwrap()),
                    duration: Duration::from_millis(20),
                },
                Frame {
                    image: Arc::new(ImageData::new(1, 1, vec![0, 255, 0, 255]).unwrap()),
                    duration: Duration::from_millis(30),
                },
            ],
            loops,
        )
        .unwrap(),
    )
}
#[test]
fn timeline_skips_delayed_frames_and_retains_last_finite_frame() {
    let data = animation(LoopCount::Finite(NonZeroU32::new(2).unwrap()));
    assert_eq!(
        data.at(Duration::ZERO),
        (0, Some(Duration::from_millis(20)))
    );
    assert_eq!(
        data.at(Duration::from_millis(20)),
        (1, Some(Duration::from_millis(30)))
    );
    assert_eq!(
        data.at(Duration::from_millis(50)),
        (0, Some(Duration::from_millis(20)))
    );
    assert_eq!(
        data.at(Duration::from_millis(99)),
        (1, Some(Duration::from_millis(1)))
    );
    assert_eq!(data.at(Duration::from_millis(100)), (1, None));
    assert_eq!(
        animation(LoopCount::Infinite).at(Duration::from_secs(100000)),
        (0, Some(Duration::from_millis(20)))
    );
}
#[test]
fn hidden_offscreen_paused_and_removed_players_cancel_owned_tasks() {
    let mut ui = Ui::new(100., 100.);
    let executor = Rc::new(RefCell::new(LocalExecutor::new()));
    let playing = ui.signal(true);
    let handle = ui.mount(provide(
        TaskRunner::from_executor(executor.clone()),
        animated_image_controlled("GIF", animation(LoopCount::Infinite), playing.clone())
            .id("animation")
            .size(40., 40.),
    ));
    ui.prepare_frame();
    executor.borrow_mut().tick();
    assert!(!executor.borrow().is_empty());
    ui.set_presented(false);
    executor.borrow_mut().tick();
    assert!(executor.borrow().is_empty());
    ui.set_presented(true);
    executor.borrow_mut().tick();
    assert!(!executor.borrow().is_empty());
    let node = handle.find("animation").unwrap();
    ui.scene
        .borrow_mut()
        .set_transform(node, Transform { x: 200., y: 0. });
    ui.prepare_frame();
    executor.borrow_mut().tick();
    assert!(executor.borrow().is_empty());
    ui.scene
        .borrow_mut()
        .set_transform(node, Transform::default());
    ui.prepare_frame();
    executor.borrow_mut().tick();
    assert!(!executor.borrow().is_empty());
    ui.scene.borrow_mut().set_effects(
        node,
        Effects {
            opacity: 0.,
            ..Default::default()
        },
    );
    ui.prepare_frame();
    executor.borrow_mut().tick();
    assert!(executor.borrow().is_empty());
    ui.scene.borrow_mut().set_effects(node, Effects::default());
    ui.prepare_frame();
    executor.borrow_mut().tick();
    assert!(!executor.borrow().is_empty());
    playing.set(false);
    executor.borrow_mut().tick();
    assert!(executor.borrow().is_empty());
    playing.set(true);
    executor.borrow_mut().tick();
    assert!(!executor.borrow().is_empty());
    handle.unmount();
    executor.borrow_mut().tick();
    assert!(executor.borrow().is_empty());
}
