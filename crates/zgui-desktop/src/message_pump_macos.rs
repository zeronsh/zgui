use std::cell::RefCell;

use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained, sel,
};
use objc2_foundation::{NSObject, NSObjectProtocol, NSTimer};

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = RefCell<Box<dyn FnMut()>>]
    struct PumpTarget;

    impl PumpTarget {
        #[unsafe(method(tick:))]
        fn tick(&self, _timer: &NSTimer) {
            // A pump may enter a nested run loop. Never invoke the same closure
            // reentrantly while it holds mutable browser state.
            if let Ok(mut pump) = self.ivars().try_borrow_mut() {
                pump();
            }
        }
    }

    unsafe impl NSObjectProtocol for PumpTarget {}
);

pub(crate) struct MessagePump {
    timer: Retained<NSTimer>,
    _target: Retained<PumpTarget>,
}

impl MessagePump {
    pub(crate) fn new(pump: Box<dyn FnMut()>) -> Self {
        let mtm = MainThreadMarker::new().expect("The message pump must run on the main thread");
        let target = PumpTarget::alloc(mtm).set_ivars(RefCell::new(pump));
        let target: Retained<PumpTarget> = unsafe { msg_send![super(target), init] };
        // NSTimer dispatches outside winit's ApplicationHandler borrow, while
        // run_app keeps its handler registered. Default mode deliberately excludes
        // modal/tracking loops entered from application callbacks.
        let timer = unsafe {
            NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                0.01,
                &target,
                sel!(tick:),
                None,
                true,
            )
        };
        Self {
            timer,
            _target: target,
        }
    }
}

impl Drop for MessagePump {
    fn drop(&mut self) {
        self.timer.invalidate();
    }
}
