use super::*;
use crate::{animation::Animation, image::ImageData};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
#[derive(Default)]
struct Playback {
    elapsed: Duration,
    started: Option<Instant>,
}
impl Playback {
    fn elapsed(&self) -> Duration {
        self.elapsed + self.started.map_or(Duration::ZERO, |start| start.elapsed())
    }
    fn pause(&mut self) {
        self.elapsed = self.elapsed();
        self.started = None;
    }
}
pub(crate) struct Animated {
    pub animation: Arc<Animation>,
    pub playing: Option<Signal<bool>>,
}
pub(crate) fn mount(
    ui: &mut Ui,
    root: NodeId,
    source: Signal<Option<Arc<ImageData>>>,
    animation: Animated,
    environment: &Environment,
) {
    if animation.animation.frames().len() < 2 {
        return;
    }
    let visible = ui.observe_visibility(root);
    let presented = ui.observe_presentation();
    let runner = environment
        .services
        .get::<TaskRunner>()
        .expect("animated_image requires a TaskRunner provider");
    let playback = Rc::new(RefCell::new(Playback::default()));
    let task: Rc<RefCell<Option<TaskToken>>> = Default::default();
    ui.retain(root, task.clone());
    ui.bind(root, move || {
        let active = presented.get()
            && visible.get()
            && animation
                .playing
                .as_ref()
                .is_none_or(|playing| playing.get());
        let previous = task.borrow_mut().take();
        drop(previous);
        playback.borrow_mut().pause();
        if !active {
            return;
        }
        playback.borrow_mut().started = Some(Instant::now());
        let data = animation.animation.clone();
        let playback = playback.clone();
        let source = source.clone();
        let future = async move {
            loop {
                let (index, remaining) = data.at(playback.borrow().elapsed());
                source.set(Some(data.frames()[index].image.clone()));
                let Some(remaining) = remaining else {
                    break;
                };
                crate::timer::sleep(remaining).await;
            }
        };
        *task.borrow_mut() = Some((runner.0)(Box::pin(future)));
    });
}
