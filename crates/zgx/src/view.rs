//! Mount ownership for retained views, reactive bindings, and scoped services.
//!
//! A template compiler can emit ordinary scope creation, service lookup, and
//! bindings; slot content can be a function receiving `&mut ViewScope`.
//!
//! ```
//! use std::{cell::RefCell, rc::Rc};
//! use zgx::{reactive::Runtime, scene::{Color, Layout, Scene, Style}, view::ViewScope};
//! let runtime = Runtime::new();
//! let title = runtime.signal(String::from("stable"));
//! let scene = Rc::new(RefCell::new(Scene::new(640.0, 480.0)));
//! let root = scene.borrow().root();
//! let mut view = ViewScope::mount(&runtime, scene.clone(), root, Layout::Column, Style::default());
//! view.text(Style::default(), Color(255, 255, 255, 255), 16.0, {
//!     let title = title.clone();
//!     move || title.get()
//! });
//! title.set(String::from("updated"));
//! drop(view); // Removes its subtree and all subscriptions, including children.
//! assert_eq!(scene.borrow().len(), 1);
//! assert_eq!(runtime.effect_count(), 0);
//! ```
use crate::{
    reactive::{Effect, Runtime, ServiceScope},
    scene::{Color, Layout, NodeId, NodeKind, Scene, Style},
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

/// Owns one mounted subtree and its reactive subscriptions.
///
/// Child scopes inherit typed services and are disposed before their parent.
/// Keep a scope alive while its view is mounted. As with ordinary scene edits,
/// dropping a scope requires that no `RefCell` borrow of its scene is active.
pub struct ViewScope {
    runtime: Runtime,
    scene: Rc<RefCell<Scene>>,
    root: NodeId,
    services: ServiceScope,
    effects: Vec<Effect>,
    children: Vec<ViewScope>,
    resources: Vec<Box<dyn std::any::Any>>,
    tasks: Vec<crate::task::TaskHandle>,
}

impl ViewScope {
    /// Mount a container with `layout` and `style` under `parent`, owned by
    /// the new scope.
    pub fn mount(
        runtime: &Runtime,
        scene: Rc<RefCell<Scene>>,
        parent: NodeId,
        layout: Layout,
        style: Style,
    ) -> Self {
        let root = scene
            .borrow_mut()
            .append(parent, NodeKind::Container(layout), style);
        Self {
            runtime: runtime.clone(),
            scene,
            root,
            services: ServiceScope::new(),
            effects: Vec::new(),
            children: Vec::new(),
            resources: Vec::new(),
            tasks: Vec::new(),
        }
    }

    /// The task is cancelled before this view's nodes and subscriptions are disposed.
    pub fn spawn(
        &mut self,
        executor: &mut crate::task::LocalExecutor,
        future: impl std::future::Future<Output = ()> + 'static,
    ) {
        self.tasks.retain(|task| !task.is_finished());
        self.tasks.push(executor.spawn_scoped(future));
    }

    /// Retains an RAII resource (for example an input binding) for this scope's
    /// lifetime. Resources are dropped before subscriptions and scene nodes.
    pub fn retain(&mut self, resource: impl std::any::Any) {
        self.resources.push(Box::new(resource));
    }

    /// The scope's container node.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// The services this scope's children inherit.
    pub fn services(&self) -> &ServiceScope {
        &self.services
    }

    /// The services this scope's children inherit, to provide more.
    pub fn services_mut(&mut self) -> &mut ServiceScope {
        &mut self.services
    }

    /// Mounts an owned child. Services present at creation are inherited;
    /// providers inserted in the child shadow its inherited values.
    pub fn child(&mut self, layout: Layout, style: Style) -> &mut Self {
        let mut child = Self::mount(&self.runtime, self.scene.clone(), self.root, layout, style);
        child.services = self.services.child();
        self.children.push(child);
        self.children.last_mut().unwrap()
    }

    /// Removes all owned child scopes and their subscriptions, retaining this
    /// scope's own nodes and bindings. Useful for replacing conditional slots.
    pub fn clear_children(&mut self) {
        self.children.clear();
    }

    /// Append a node to the scope's container. It is removed with the scope.
    pub fn append(&mut self, kind: NodeKind, style: Style) -> NodeId {
        self.scene.borrow_mut().append(self.root, kind, style)
    }

    /// Runs immediately and retains the subscription for this scope's lifetime.
    pub fn effect(&mut self, callback: impl FnMut() + 'static) {
        self.effects.push(self.runtime.effect(callback));
    }

    /// Binds an existing text node. Reads in `compute` are tracked automatically.
    /// If a subtree was removed explicitly through `Scene`, future updates are
    /// harmless; generational node IDs prevent writes to a reused node slot.
    pub fn bind_text<T: Into<Arc<str>>>(
        &mut self,
        node: NodeId,
        mut compute: impl FnMut() -> T + 'static,
    ) {
        let scene = Rc::downgrade(&self.scene);
        self.effect(move || {
            let Some(scene) = scene.upgrade() else {
                return;
            };
            if !scene.borrow().contains(node) {
                return;
            }
            // Evaluate user code without a scene borrow, allowing computed text
            // to inspect layout or mount other independent views safely.
            let value = compute();
            let mut scene = scene.borrow_mut();
            if scene.contains(node) {
                scene.set_text(node, value);
            }
        });
    }

    /// Append a text node whose text follows what `compute` reads (see
    /// [`ViewScope::bind_text`]).
    pub fn text<T: Into<Arc<str>>>(
        &mut self,
        style: Style,
        color: Color,
        font_size: f32,
        compute: impl FnMut() -> T + 'static,
    ) -> NodeId {
        let node = self.append(
            NodeKind::Text {
                text: "".into(),
                color,
                font_size,
            },
            style,
        );
        self.bind_text(node, compute);
        node
    }
}

impl Drop for ViewScope {
    fn drop(&mut self) {
        self.tasks.clear();
        self.children.clear();
        self.resources.clear();
        self.effects.clear();
        let mut scene = self.scene.borrow_mut();
        if scene.contains(self.root) {
            scene.remove(self.root);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_resources_drop_before_the_owned_subtree() {
        struct Guard {
            scene: Rc<RefCell<Scene>>,
            root: NodeId,
            dropped: Rc<std::cell::Cell<bool>>,
        }
        impl Drop for Guard {
            fn drop(&mut self) {
                assert!(self.scene.borrow().contains(self.root));
                self.dropped.set(true);
            }
        }
        let runtime = Runtime::new();
        let scene = Rc::new(RefCell::new(Scene::new(100.0, 100.0)));
        let root = scene.borrow().root();
        let mut view = ViewScope::mount(
            &runtime,
            scene.clone(),
            root,
            Layout::Column,
            Style::default(),
        );
        let dropped = Rc::new(std::cell::Cell::new(false));
        view.retain(Guard {
            scene: scene.clone(),
            root: view.root(),
            dropped: dropped.clone(),
        });
        drop(view);
        assert!(dropped.get());
        assert_eq!(scene.borrow().len(), 1);
    }
    #[test]
    fn disposing_view_removes_subtree_and_effects_but_retains_sibling() {
        let runtime = Runtime::new();
        let scene = Rc::new(RefCell::new(Scene::new(300.0, 200.0)));
        let root = scene.borrow().root();
        let value = runtime.signal(String::from("before"));
        let sibling = scene.borrow_mut().append(
            root,
            NodeKind::Rect(Color(255, 0, 0, 255)),
            Style::default(),
        );
        let mut view = ViewScope::mount(
            &runtime,
            scene.clone(),
            root,
            Layout::Column,
            Style::default(),
        );
        let mounted_root = view.root();
        let text = view.text(Style::default(), Color(255, 255, 255, 255), 16.0, {
            let value = value.clone();
            move || value.get()
        });
        view.child(Layout::Row, Style::default()).text(
            Style::default(),
            Color(255, 255, 255, 255),
            16.0,
            {
                let value = value.clone();
                move || value.get()
            },
        );
        assert_eq!(runtime.effect_count(), 2);
        value.set(String::from("after"));
        assert!(scene.borrow().paint_items().any(|item| {
            item.id == text
                && matches!(item.kind, NodeKind::Text { text, .. } if text.as_ref() == "after")
        }));
        drop(view);
        value.set(String::from("unmounted"));
        assert_eq!(runtime.effect_count(), 0);
        assert!(!scene.borrow().contains(mounted_root));
        assert!(!scene.borrow().contains(text));
        assert!(scene.borrow().contains(sibling));
        assert_eq!(scene.borrow().len(), 2);
    }

    #[test]
    fn external_removal_and_slot_reuse_do_not_resurrect_text_binding() {
        let runtime = Runtime::new();
        let scene = Rc::new(RefCell::new(Scene::new(300.0, 200.0)));
        let root = scene.borrow().root();
        let value = runtime.signal(String::from("old"));
        let mut view = ViewScope::mount(
            &runtime,
            scene.clone(),
            root,
            Layout::Column,
            Style::default(),
        );
        view.text(Style::default(), Color(0, 0, 0, 255), 16.0, {
            let value = value.clone();
            move || value.get()
        });
        scene.borrow_mut().remove(view.root());
        let replacement =
            scene
                .borrow_mut()
                .append(root, NodeKind::Rect(Color(1, 2, 3, 255)), Style::default());
        value.set(String::from("new"));
        drop(view);
        assert!(scene.borrow().contains(replacement));
        assert_eq!(runtime.effect_count(), 0);
    }

    #[test]
    fn child_services_shadow_and_conditional_slot_cleanup_disposes_effects() {
        let runtime = Runtime::new();
        let scene = Rc::new(RefCell::new(Scene::new(300.0, 200.0)));
        let root = scene.borrow().root();
        let mut view = ViewScope::mount(
            &runtime,
            scene.clone(),
            root,
            Layout::Column,
            Style::default(),
        );
        view.services_mut().provide(12_u32);
        let child = view.child(Layout::Column, Style::default());
        assert_eq!(*child.services().get::<u32>().unwrap(), 12);
        child.services_mut().provide(24_u32);
        assert_eq!(*child.services().get::<u32>().unwrap(), 24);
        child.effect(|| {});
        assert_eq!(*view.services().get::<u32>().unwrap(), 12);
        assert_eq!(runtime.effect_count(), 1);
        view.clear_children();
        assert_eq!(runtime.effect_count(), 0);
        assert_eq!(scene.borrow().len(), 2);
    }
}
