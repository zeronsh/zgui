//! The test harness: zgui-script's public testing kit (`testing::TestApp`),
//! which app authors use too, so these tests keep it honest. A local type
//! around it, so a test file can add helpers of its own.
use zgui::{compose::View, reactive::Runtime};
use zgui_script::{ScriptHost, testing::TestApp};

pub struct Harness(pub TestApp);
impl std::ops::Deref for Harness {
    type Target = TestApp;
    fn deref(&self) -> &TestApp {
        &self.0
    }
}
impl std::ops::DerefMut for Harness {
    fn deref_mut(&mut self) -> &mut TestApp {
        &mut self.0
    }
}
#[allow(dead_code)]
impl Harness {
    pub fn new(host: impl FnOnce(&Runtime) -> ScriptHost) -> Self {
        Self(TestApp::new(host))
    }
    pub fn inline(code: &str) -> Self {
        Self(TestApp::inline(code))
    }
    pub fn with_desktop(host: impl FnOnce(&Runtime) -> ScriptHost) -> Self {
        Self(TestApp::with_desktop(host))
    }
    pub fn inspected(
        host: impl FnOnce(&Runtime) -> ScriptHost,
        inspector: &zgui::inspect::Inspector,
    ) -> Self {
        Self(TestApp::inspected(host, inspector))
    }
    pub fn wrapped(
        host: impl FnOnce(&Runtime) -> ScriptHost,
        wrap: impl FnOnce(View) -> View,
    ) -> Self {
        Self(TestApp::wrapped(host, wrap))
    }
}
