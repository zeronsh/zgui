//! Scale, rotation, skew and tilt in perspective on scene nodes: zgx's local transforms, which
//! re-composite a node's cached layer without repainting it.
pub use zgx::scene::LocalTransform;
use zgx::{
    scene::{NodeId, Scene},
    style::Styled,
};

pub fn local_transform(scene: &Scene, id: NodeId) -> LocalTransform {
    scene.local_transform(id)
}
pub fn set_local_transform(scene: &mut Scene, id: NodeId, transform: LocalTransform) {
    scene.set_local_transform(id, transform);
}

/// Sets a whole local transform through a node's styles.
pub fn style<S: Styled>(styles: S, transform: LocalTransform) -> S {
    styles
        .scale_xy(transform.scale.0, transform.scale.1)
        .rotate(transform.rotate)
        .skew(transform.skew.0, transform.skew.1)
        .rotate_x(transform.rotate_x)
        .rotate_y(transform.rotate_y)
        .perspective(transform.perspective)
        .transform_origin(transform.origin.0, transform.origin.1)
}
