---
section: added
---
- **Pixelated images** (zgx, zgx-gpu, zgx-script): `Styled::image_rendering(ImageRendering::Pixelated)` (`Scene::set_image_rendering`), Luau `image_rendering = "pixelated"`, as CSS's `image-rendering`: an image, or a background image, drawn larger or smaller takes each point's nearest pixel, its edges hard, for pixel art, a magnified screenshot or a QR code. Its draw samples with a nearest sampler of its own (wgpu a bind group, Metal pipelines, Direct3D 11 a sampler state, each made with the first): the shaders are as they were, and an image not pixelated draws exactly as before, in as many draws.
