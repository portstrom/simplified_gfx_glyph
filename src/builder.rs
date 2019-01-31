use super::*;

/// Builder for a [`GlyphBrush`](struct.GlyphBrush.html).
///
/// # Example
///
/// ```no_run
/// # extern crate gfx;
/// # extern crate gfx_window_glutin;
/// # extern crate glutin;
/// extern crate gfx_glyph;
/// use gfx_glyph::GlyphBrushBuilder;
/// # fn main() {
/// # let events_loop = glutin::EventsLoop::new();
/// # let (_window, _device, gfx_factory, _gfx_target, _main_depth) =
/// #     gfx_window_glutin::init::<gfx::format::Srgba8, gfx::format::Depth>(
/// #         glutin::WindowBuilder::new(),
/// #         glutin::ContextBuilder::new(),
/// #         &events_loop);
///
/// let dejavu: &[u8] = include_bytes!("../examples/DejaVuSans.ttf");
/// let mut glyph_brush = GlyphBrushBuilder::using_font_bytes(dejavu).build(gfx_factory.clone());
/// # let _ = glyph_brush;
/// # }
/// ```
pub struct GlyphBrushBuilder<'a> {
    font_data: Vec<Font<'a>>,
    initial_cache_size: (u32, u32),
    gpu_cache_scale_tolerance: f32,
    gpu_cache_position_tolerance: f32,
    depth_test: gfx::state::Depth,
    texture_filter_method: texture::FilterMethod,
}

impl<'a> GlyphBrushBuilder<'a> {
    /// Specifies the default font data used to render glyphs.
    pub fn using_font_bytes(font_0_data: impl Into<SharedBytes<'a>>) -> Self {
        Self::using_font(Font::from_bytes(font_0_data).unwrap())
    }

    pub fn using_fonts_bytes<B: Into<SharedBytes<'a>>>(font_data: impl Into<Vec<B>>) -> Self {
        Self::using_fonts(
            font_data
                .into()
                .into_iter()
                .map(|data| Font::from_bytes(data).unwrap())
                .collect::<Vec<_>>(),
        )
    }

    /// Specifies the default font used to render glyphs.
    pub fn using_font(font_0: Font<'a>) -> Self {
        Self::using_fonts(vec![font_0])
    }

    pub fn using_fonts(fonts: impl Into<Vec<Font<'a>>>) -> Self {
        GlyphBrushBuilder {
            font_data: fonts.into(),
            initial_cache_size: (256, 256),
            gpu_cache_scale_tolerance: 0.5,
            gpu_cache_position_tolerance: 0.1,
            depth_test: gfx::preset::depth::PASS_TEST,
            texture_filter_method: texture::FilterMethod::Bilinear,
        }
    }
}

impl<'a> GlyphBrushBuilder<'a> {
    /// Adds additional fonts to the one added in [`using_font`](#method.using_font) /
    /// [`using_font_bytes`](#method.using_font_bytes).
    pub fn add_font_bytes(&mut self, font_data: impl Into<SharedBytes<'a>>) {
        self.font_data
            .push(Font::from_bytes(font_data.into()).unwrap());
    }

    /// Adds additional fonts to the one added in [`using_font`](#method.using_font) /
    /// [`using_font_bytes`](#method.using_font_bytes).
    pub fn add_font(&mut self, font_data: Font<'a>) {
        self.font_data.push(font_data);
    }

    /// Initial size of 2D texture used as a gpu cache, pixels (width, height).
    /// The GPU cache will dynamically quadruple in size whenever the current size
    /// is insufficient.
    ///
    /// Defaults to `(256, 256)`
    pub fn initial_cache_size(mut self, size: (u32, u32)) -> Self {
        self.initial_cache_size = size;
        self
    }

    /// Sets the maximum allowed difference in scale used for judging whether to reuse an
    /// existing glyph in the GPU cache.
    ///
    /// Defaults to `0.5`
    ///
    /// See rusttype docs for `rusttype::gpu_cache::Cache`
    pub fn gpu_cache_scale_tolerance(mut self, tolerance: f32) -> Self {
        self.gpu_cache_scale_tolerance = tolerance;
        self
    }

    /// Sets the maximum allowed difference in subpixel position used for judging whether
    /// to reuse an existing glyph in the GPU cache. Anything greater than or equal to
    /// 1.0 means "don't care".
    ///
    /// Defaults to `0.1`
    ///
    /// See rusttype docs for `rusttype::gpu_cache::Cache`
    pub fn gpu_cache_position_tolerance(mut self, tolerance: f32) -> Self {
        self.gpu_cache_position_tolerance = tolerance;
        self
    }

    /// Sets the depth test to use on the text section **z** values.
    ///
    /// Defaults to: *Always pass the depth test, never write to the depth buffer write*
    ///
    /// # Example
    ///
    /// ```no_run
    /// # extern crate gfx;
    /// # extern crate gfx_glyph;
    /// # use gfx_glyph::GlyphBrushBuilder;
    /// # fn main() {
    /// # let some_font: &[u8] = include_bytes!("../examples/DejaVuSans.ttf");
    /// GlyphBrushBuilder::using_font_bytes(some_font)
    ///     .depth_test(gfx::preset::depth::LESS_EQUAL_WRITE)
    ///     // ...
    /// # ;
    /// # }
    /// ```
    pub fn depth_test(mut self, depth_test: gfx::state::Depth) -> Self {
        self.depth_test = depth_test;
        self
    }

    /// Sets the texture filtering method.
    ///
    /// Defaults to `Bilinear`
    ///
    /// # Example
    /// ```no_run
    /// # extern crate gfx;
    /// # extern crate gfx_glyph;
    /// # use gfx_glyph::GlyphBrushBuilder;
    /// # fn main() {
    /// # let some_font: &[u8] = include_bytes!("../examples/DejaVuSans.ttf");
    /// GlyphBrushBuilder::using_font_bytes(some_font)
    ///     .texture_filter_method(gfx::texture::FilterMethod::Scale)
    ///     // ...
    /// # ;
    /// # }
    /// ```
    pub fn texture_filter_method(mut self, filter_method: texture::FilterMethod) -> Self {
        self.texture_filter_method = filter_method;
        self
    }

    /// Builds a `GlyphBrush` using the input gfx factory
    pub fn build<R, F>(self, mut factory: F) -> GlyphBrush<'a, R, F>
    where
        R: gfx::Resources,
        F: gfx::Factory<R>,
    {
        let (cache_width, cache_height) = self.initial_cache_size;
        let font_cache_tex = create_texture(&mut factory, cache_width, cache_height).unwrap();
        let program = factory
            .link_program(
                include_bytes!("shader/vert.glsl"),
                include_bytes!("shader/frag.glsl"),
            )
            .unwrap();

        GlyphBrush {
            sections: vec![],
            fonts: self.font_data,
            font_cache: Cache::builder()
                .dimensions(cache_width, cache_height)
                .scale_tolerance(self.gpu_cache_scale_tolerance)
                .position_tolerance(self.gpu_cache_position_tolerance)
                .build(),
            font_cache_tex,
            texture_filter_method: self.texture_filter_method,

            factory,
            program,
            draw_cache: None,

            depth_test: self.depth_test,

            #[cfg(feature = "performance_stats")]
            perf: performance_stats::PerformanceStats::default(),
        }
    }
}
