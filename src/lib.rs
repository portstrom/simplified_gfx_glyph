//! Fast GPU cached text rendering using gfx-rs & rusttype.
#![allow(unknown_lints)]
#![warn(clippy)]

#[cfg(test)]
#[macro_use]
extern crate approx;
#[cfg(test)]
#[macro_use]
extern crate lazy_static;

#[macro_use]
extern crate gfx;
#[macro_use]
extern crate log;

mod builder;
#[cfg(feature = "performance_stats")]
mod performance_stats;
mod pipe;

pub use builder::*;
pub use rusttype::{
    Font, Glyph, GlyphId, HMetrics, Point, PositionedGlyph, Rect, Scale, ScaledGlyph, SharedBytes,
    VMetrics, Vector,
};

use gfx::{
    format, handle,
    handle::{RawDepthStencilView, RawRenderTargetView},
    texture,
    traits::FactoryExt,
};
use pipe::*;
use rusttype::{gpu_cache::Cache, point};
use std::error::Error;

pub(crate) type Color = [f32; 4];

#[derive(Clone)]
pub struct LayoutGlyph<'font> {
    pub color: Color,
    pub font_id: usize,
    pub glyph: PositionedGlyph<'font>,
}

#[derive(Clone)]
pub struct Section<'font> {
    pub bounds: Rect<f32>,
    pub glyphs: Vec<LayoutGlyph<'font>>,
    pub z: f32,
}

// Type for the generated glyph cache texture
type TexForm = format::U8Norm;
type TexSurface = <TexForm as format::Formatted>::Surface;
type TexChannel = <TexForm as format::Formatted>::Channel;
type TexFormView = <TexForm as format::Formatted>::View;
type TexSurfaceHandle<R> = handle::Texture<R, TexSurface>;
type TexShaderView<R> = handle::ShaderResourceView<R, TexFormView>;

const IDENTITY_MATRIX4: [[f32; 4]; 4] = [
    [1., 0., 0., 0.],
    [0., 1., 0., 0.],
    [0., 0., 1., 0.],
    [0., 0., 0., 1.],
];

/// Object allowing glyph drawing, containing cache state. Manages glyph positioning cacheing,
/// glyph draw caching & efficient GPU texture cache updating and re-sizing on demand.
///
/// Build using a [`GlyphBrushBuilder`](struct.GlyphBrushBuilder.html).
pub struct GlyphBrush<'font, R: gfx::Resources, F: gfx::Factory<R>> {
    depth_test: gfx::state::Depth,
    draw_cache: Option<DrawnGlyphBrush<R>>,
    factory: F,
    font_cache: Cache<'font>,
    font_cache_tex: (
        gfx::handle::Texture<R, TexSurface>,
        gfx_core::handle::ShaderResourceView<R, f32>,
    ),
    fonts: Vec<Font<'font>>,
    #[cfg(feature = "performance_stats")]
    perf: performance_stats::PerformanceStats,
    program: gfx::handle::Program<R>,
    sections: Vec<Section<'font>>,
    texture_filter_method: texture::FilterMethod,
}

impl<'font> LayoutGlyph<'font> {
    pub fn translated(&self, offset: Vector<f32>) -> Self {
        LayoutGlyph {
            glyph: self
                .glyph
                .unpositioned()
                .clone()
                .positioned(self.glyph.position() + offset),
            ..*self
        }
    }
}

impl<'font, R: gfx::Resources, F: gfx::Factory<R>> GlyphBrush<'font, R, F> {
    pub fn queue_section(&mut self, section: Section<'font>) {
        self.sections.push(section);
    }

    /// Draws all queued sections onto a render target, applying a position transform (e.g.
    /// a projection).
    /// See [`queue`](struct.GlyphBrush.html#method.queue).
    ///
    /// Trims the cache, see [caching behaviour](#caching-behaviour).
    ///
    /// # Raw usage
    /// Can also be used with gfx raw render & depth views if necessary. The `Format` must also
    /// be provided. [See example.](struct.GlyphBrush.html#raw-usage-1)
    pub fn draw_queued<C, CV, DV>(
        &mut self,
        encoder: &mut gfx::Encoder<R, C>,
        target: &CV,
        depth_target: &DV,
    ) -> Result<(), String>
    where
        C: gfx::CommandBuffer<R>,
        CV: RawAndFormat<Raw = RawRenderTargetView<R>>,
        DV: RawAndFormat<Raw = RawDepthStencilView<R>>,
    {
        self.draw_queued_with_transform(IDENTITY_MATRIX4, encoder, target, depth_target)
    }

    /// Draws all queued sections onto a render target, applying a position transform (e.g.
    /// a projection).
    /// See [`queue`](struct.GlyphBrush.html#method.queue).
    ///
    /// Trims the cache, see [caching behaviour](#caching-behaviour).
    ///
    /// # Raw usage
    /// Can also be used with gfx raw render & depth views if necessary. The `Format` must also
    /// be provided.
    ///
    /// ```no_run
    /// # extern crate gfx;
    /// # extern crate gfx_window_glutin;
    /// # extern crate glutin;
    /// # extern crate gfx_glyph;
    /// # use gfx_glyph::{GlyphBrushBuilder};
    /// # use gfx_glyph::Section;
    /// # use gfx::format;
    /// # use gfx::format::Formatted;
    /// # use gfx::memory::Typed;
    /// # fn main() -> Result<(), String> {
    /// # let events_loop = glutin::EventsLoop::new();
    /// # let (_window, _device, mut gfx_factory, gfx_color, gfx_depth) =
    /// #     gfx_window_glutin::init::<gfx::format::Srgba8, gfx::format::Depth>(
    /// #         glutin::WindowBuilder::new(),
    /// #         glutin::ContextBuilder::new(),
    /// #         &events_loop);
    /// # let mut gfx_encoder: gfx::Encoder<_, _> = gfx_factory.create_command_buffer().into();
    /// # let dejavu: &[u8] = include_bytes!("../examples/DejaVuSans.ttf");
    /// # let mut glyph_brush = GlyphBrushBuilder::using_font_bytes(dejavu)
    /// #     .build(gfx_factory.clone());
    /// # let raw_render_view = gfx_color.raw();
    /// # let raw_depth_view = gfx_depth.raw();
    /// # let transform = [[0.0; 4]; 4];
    /// glyph_brush.draw_queued_with_transform(
    ///     transform,
    ///     &mut gfx_encoder,
    ///     &(raw_render_view, format::Srgba8::get_format()),
    ///     &(raw_depth_view, format::Depth::get_format()),
    /// )?
    /// # ;
    /// # Ok(())
    /// # }
    /// ```
    pub fn draw_queued_with_transform<C, CV, DV>(
        &mut self,
        transform: [[f32; 4]; 4],
        encoder: &mut gfx::Encoder<R, C>,
        target: &CV,
        depth_target: &DV,
    ) -> Result<(), String>
    where
        C: gfx::CommandBuffer<R>,
        CV: RawAndFormat<Raw = RawRenderTargetView<R>>,
        DV: RawAndFormat<Raw = RawDepthStencilView<R>>,
    {
        #[cfg(feature = "performance_stats")]
        self.perf.draw_start();

        let (screen_width, screen_height, ..) = target.as_raw().get_dimensions();
        let (screen_width, screen_height) = (u32::from(screen_width), u32::from(screen_height));

        let mut gpu_cache_rebuilt = false;
        loop {
            if !gpu_cache_rebuilt {
                let mut no_text = true;

                for section in &self.sections {
                    for glyph in &section.glyphs {
                        self.font_cache
                            .queue_glyph(glyph.font_id, glyph.glyph.clone());
                        no_text = false;
                    }
                }

                if no_text {
                    return Ok(());
                }
            }

            let tex = self.font_cache_tex.0.clone();
            if let Err(err) = self.font_cache.cache_queued(|rect, tex_data| {
                let info = texture::ImageInfoCommon {
                    xoffset: rect.min.x as u16,
                    yoffset: rect.min.y as u16,
                    zoffset: 0,
                    width: rect.width() as u16,
                    height: rect.height() as u16,
                    depth: 0,
                    format: (),
                    mipmap: 0,
                };
                encoder
                    .update_texture::<TexSurface, TexForm>(&tex, None, info, tex_data)
                    .unwrap();
            }) {
                let (width, height) = self.font_cache.dimensions();
                let (new_width, new_height) = (width * 2, height * 2);

                if log_enabled!(log::Level::Warn) {
                    warn!(
                        "Increasing glyph texture size {old:?} -> {new:?}, as {reason:?}. \
                         Consider building with `.initial_cache_size({new:?})` to avoid \
                         resizing.",
                        old = (width, height),
                        new = (new_width, new_height),
                        reason = err,
                    );
                }

                match create_texture(&mut self.factory, new_width, new_height) {
                    Ok((new_tex, tex_view)) => {
                        self.font_cache
                            .to_builder()
                            .dimensions(new_width, new_height)
                            .rebuild(&mut self.font_cache);

                        // queue is intact
                        gpu_cache_rebuilt = true;

                        if let Some(ref mut cache) = self.draw_cache {
                            cache.texture_updated = true;
                        }

                        self.font_cache_tex.1 = tex_view;
                        self.font_cache_tex.0 = new_tex;
                        continue;
                    }
                    Err(_) => {
                        return Err(format!(
                            "Failed to create {}x{} glyph texture",
                            new_width, new_height
                        ));
                    }
                }
            }

            break;
        }
        #[cfg(feature = "performance_stats")]
        self.perf.gpu_cache_done();

        let verts: Vec<GlyphVertex> = {
            let mut verts = Vec::with_capacity(
                self.sections
                    .iter()
                    .map(|section| section.glyphs.len())
                    .sum::<usize>(),
            );

            for section in &self.sections {
                verts.extend(section.glyphs.iter().filter_map(|glyph| {
                    vertex(
                        glyph,
                        &self.font_cache,
                        section.bounds,
                        section.z,
                        (screen_width as f32, screen_height as f32),
                    )
                }));
            }

            verts
        };
        #[cfg(feature = "performance_stats")]
        self.perf.vertex_generation_done();

        let vbuf = self.factory.create_vertex_buffer(&verts);

        let draw_cache = if let Some(mut cache) = self.draw_cache.take() {
            cache.pipe_data.vbuf = vbuf;
            cache.pipe_data.out = target.as_raw().clone();
            cache.pipe_data.out_depth = depth_target.as_raw().clone();
            if cache.pso.0 != target.format() {
                cache.pso = (
                    target.format(),
                    self.pso_using(target.format(), depth_target.format()),
                );
            }
            cache.slice.instances.as_mut().unwrap().0 = verts.len() as _;
            if cache.texture_updated {
                cache.pipe_data.font_tex.0 = self.font_cache_tex.1.clone();
                cache.texture_updated = false;
            }
            cache
        } else {
            DrawnGlyphBrush {
                pipe_data: {
                    let sampler = self.factory.create_sampler(texture::SamplerInfo::new(
                        self.texture_filter_method,
                        texture::WrapMode::Clamp,
                    ));
                    glyph_pipe::Data {
                        vbuf,
                        font_tex: (self.font_cache_tex.1.clone(), sampler),
                        transform,
                        out: target.as_raw().clone(),
                        out_depth: depth_target.as_raw().clone(),
                    }
                },
                pso: (
                    target.format(),
                    self.pso_using(target.format(), depth_target.format()),
                ),
                slice: gfx::Slice {
                    base_vertex: 0,
                    buffer: gfx::IndexBuffer::Auto,
                    end: 4,
                    instances: Some((verts.len() as _, 0)),
                    start: 0,
                },
                texture_updated: false,
            }
        };

        self.draw_cache = Some(draw_cache);

        if let Some(&mut DrawnGlyphBrush {
            ref pso,
            ref slice,
            ref mut pipe_data,
            ..
        }) = self.draw_cache.as_mut()
        {
            pipe_data.transform = transform;
            encoder.draw(slice, &pso.1, pipe_data);
        }

        self.sections.clear();

        #[cfg(feature = "performance_stats")]
        {
            self.perf.draw_finished();
            self.perf.log_sluggishness();
        }

        Ok(())
    }

    pub fn fonts(&self) -> &[Font<'font>] {
        &self.fonts
    }

    fn pso_using(
        &mut self,
        color_format: gfx::format::Format,
        depth_format: gfx::format::Format,
    ) -> gfx::PipelineState<R, glyph_pipe::Meta> {
        self.factory
            .create_pipeline_from_program(
                &self.program,
                gfx::Primitive::TriangleStrip,
                gfx::state::Rasterizer::new_fill(),
                glyph_pipe::Init::new(color_format, depth_format, self.depth_test),
            )
            .unwrap()
    }

    /// Adds an additional font to the one(s) initially added on build.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # extern crate gfx;
    /// # extern crate gfx_window_glutin;
    /// # extern crate glutin;
    /// extern crate gfx_glyph;
    /// use gfx_glyph::{GlyphBrushBuilder, Section};
    /// # fn main() {
    /// # let events_loop = glutin::EventsLoop::new();
    /// # let (_window, _device, mut gfx_factory, gfx_color, gfx_depth) =
    /// #     gfx_window_glutin::init::<gfx::format::Srgba8, gfx::format::Depth>(
    /// #         glutin::WindowBuilder::new(),
    /// #         glutin::ContextBuilder::new(),
    /// #         &events_loop);
    /// # let mut gfx_encoder: gfx::Encoder<_, _> = gfx_factory.create_command_buffer().into();
    ///
    /// // dejavu is built as default
    /// let dejavu: &[u8] = include_bytes!("../examples/DejaVuSans.ttf");
    /// let mut glyph_brush = GlyphBrushBuilder::using_font_bytes(dejavu).build(gfx_factory.clone());
    ///
    /// // some time later, add another font
    /// let open_sans_italic: &[u8] = include_bytes!("../examples/OpenSans-Italic.ttf");
    /// let open_sans_italic_id = glyph_brush.add_font_bytes(open_sans_italic);
    /// # glyph_brush.draw_queued(&mut gfx_encoder, &gfx_color, &gfx_depth).unwrap();
    /// # let _ = open_sans_italic_id;
    /// # }
    /// ```
    pub fn add_font_bytes<'a: 'font, B: Into<SharedBytes<'a>>>(&mut self, font_data: B) {
        self.add_font(Font::from_bytes(font_data.into()).unwrap())
    }

    /// Adds an additional font to the one(s) initially added on build.
    pub fn add_font<'a: 'font>(&mut self, font_data: Font<'a>) {
        self.fonts.push(font_data);
    }
}

struct DrawnGlyphBrush<R: gfx::Resources> {
    pipe_data: glyph_pipe::Data<R>,
    pso: (gfx::format::Format, gfx::PipelineState<R, glyph_pipe::Meta>),
    slice: gfx::Slice<R>,
    texture_updated: bool,
}

#[inline]
fn vertex(
    glyph: &LayoutGlyph,
    cache: &Cache,
    bounds: Rect<f32>,
    z: f32,
    (screen_width, screen_height): (f32, f32),
) -> Option<GlyphVertex> {
    let gl_bounds = Rect {
        min: point(
            2.0 * (bounds.min.x / screen_width - 0.5),
            2.0 * (0.5 - bounds.min.y / screen_height),
        ),
        max: point(
            2.0 * (bounds.max.x / screen_width - 0.5),
            2.0 * (0.5 - bounds.max.y / screen_height),
        ),
    };

    let rect = cache.rect_for(glyph.font_id, &glyph.glyph);
    if let Ok(Some((mut uv_rect, screen_rect))) = rect {
        if screen_rect.min.x as f32 > bounds.max.x
            || screen_rect.min.y as f32 > bounds.max.y
            || bounds.min.x > screen_rect.max.x as f32
            || bounds.min.y > screen_rect.max.y as f32
        {
            // glyph is totally outside the bounds
            return None;
        }

        let mut gl_rect = Rect {
            min: point(
                2.0 * (screen_rect.min.x as f32 / screen_width - 0.5),
                2.0 * (0.5 - screen_rect.min.y as f32 / screen_height),
            ),
            max: point(
                2.0 * (screen_rect.max.x as f32 / screen_width - 0.5),
                2.0 * (0.5 - screen_rect.max.y as f32 / screen_height),
            ),
        };

        // handle overlapping bounds, modify uv_rect to preserve texture aspect
        if gl_rect.max.x > gl_bounds.max.x {
            let old_width = gl_rect.width();
            gl_rect.max.x = gl_bounds.max.x;
            uv_rect.max.x = uv_rect.min.x + uv_rect.width() * gl_rect.width() / old_width;
        }
        if gl_rect.min.x < gl_bounds.min.x {
            let old_width = gl_rect.width();
            gl_rect.min.x = gl_bounds.min.x;
            uv_rect.min.x = uv_rect.max.x - uv_rect.width() * gl_rect.width() / old_width;
        }
        // note: y access is flipped gl compared with screen,
        // texture is not flipped (ie is a headache)
        if gl_rect.max.y < gl_bounds.max.y {
            let old_height = gl_rect.height();
            gl_rect.max.y = gl_bounds.max.y;
            uv_rect.max.y = uv_rect.min.y + uv_rect.height() * gl_rect.height() / old_height;
        }
        if gl_rect.min.y > gl_bounds.min.y {
            let old_height = gl_rect.height();
            gl_rect.min.y = gl_bounds.min.y;
            uv_rect.min.y = uv_rect.max.y - uv_rect.height() * gl_rect.height() / old_height;
        }

        Some(GlyphVertex {
            left_top: [gl_rect.min.x, gl_rect.max.y, z],
            right_bottom: [gl_rect.max.x, gl_rect.min.y],
            tex_left_top: [uv_rect.min.x, uv_rect.max.y],
            tex_right_bottom: [uv_rect.max.x, uv_rect.min.y],
            color: glyph.color,
        })
    } else {
        if rect.is_err() {
            panic!("Cache miss?: {:?}", rect);
        }
        None
    }
}

// Creates a gfx texture with the given data
fn create_texture<R: gfx::Resources>(
    factory: &mut impl gfx::Factory<R>,
    width: u32,
    height: u32,
) -> Result<(TexSurfaceHandle<R>, TexShaderView<R>), Box<Error>> {
    let kind = texture::Kind::D2(
        width as texture::Size,
        height as texture::Size,
        texture::AaMode::Single,
    );

    let tex = factory.create_texture(
        kind,
        1 as texture::Level,
        gfx::memory::Bind::SHADER_RESOURCE,
        gfx::memory::Usage::Dynamic,
        Some(<TexChannel as format::ChannelTyped>::get_channel_type()),
    )?;

    let view =
        factory.view_texture_as_shader_resource::<TexForm>(&tex, (0, 0), format::Swizzle::new())?;

    Ok((tex, view))
}
