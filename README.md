# Simplified gfx_glyph

This is a simplified version of gfx_glyph made as a proof of concept of how [gfx_glyph](https://github.com/alexheretic/glyph-brush) can be made faster and more versatile by removing the layout features and letting the caller do it's own layout and caching.

There is an example displaying an HTML document: `cargo run --example html`

The example only works on X11 due to an issue ([231](https://github.com/tomaka/winit/issues/231)) ([276](https://github.com/tomaka/winit/issues/276)) in winit that its maintainers don't care to fix.
