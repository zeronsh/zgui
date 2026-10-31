---
section: added
---
- **Gradient masks** (zgx, zgx-script): `Styled::mask_gradient(Gradient)` (`Scene::set_mask_gradient`), Luau `mask_gradient` and `mask_radial` written as `gradient` and `radial` are: a node and what it holds show through a linear or radial gradient's alpha over its box, at whatever size it's laid out (CSS's `mask-image` with a gradient), for a list fading out at its bottom or a vignette. The mask's image is made from the gradient at the box's size when it's laid out, and again when the box resizes; it then draws as an image mask does.
