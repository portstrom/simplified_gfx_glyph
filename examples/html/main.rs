mod display_document;
mod html_document;

use gfx::Device;
use glutin::{
    os::unix::{EventsLoopExt, WindowExt},
    WindowEvent,
};

const BACKGROUND_COLOR: [f32; 4] = [1., 1., 1., 1.];
const MARGIN: f32 = 32.;

fn main() {
    let mut events_loop = glutin::EventsLoop::new_x11().unwrap();
    let window = glutin::WindowBuilder::new()
        .with_multitouch()
        .with_title("gfx_glyph example");
    let context = glutin::ContextBuilder::new()
        .with_gl(glutin::GlRequest::Specific(glutin::Api::OpenGl, (3, 2)))
        .with_vsync(true);
    let (window, mut device, mut factory, mut color_view, mut depth_view) =
        gfx_window_glutin::init::<gfx::format::Rgba8, gfx::format::DepthStencil>(
            window,
            context,
            &events_loop,
        )
        .unwrap();
    let mut encoder: gfx::Encoder<_, _> = factory.create_command_buffer().into();
    encoder.clear(&color_view, BACKGROUND_COLOR);
    encoder.flush(&mut device);
    window.swap_buffers().unwrap();
    device.cleanup();
    let document = std::fs::read("examples/html/document.html").unwrap_or_else(|error| {
        eprintln!("Failed to read HTML document: {}", error);
        std::process::exit(1);
    });
    let document = std::str::from_utf8(&document).unwrap_or_else(|error| {
        eprintln!("Failed to parse HTML document: {}", error);
        std::process::exit(1);
    });
    let load_font = |path| {
        let file_contents = std::fs::read(path).unwrap_or_else(|error| {
            eprintln!("Failed to read font the file {:?}: {}\nYou can change the path in main.rs to load fonts from another location.", path, error);
            std::process::exit(1);
        });
        gfx_glyph::Font::from_bytes(file_contents).unwrap_or_else(|error| {
            eprintln!("Failed to parse font from the file {:?}: {}", path, error);
            std::process::exit(1);
        })
    };
    let mut glyph_brush = gfx_glyph::GlyphBrushBuilder::using_fonts(vec![
        load_font("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        load_font("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
        load_font("/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf"),
        load_font("/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf"),
        load_font("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
    ])
    .build(factory.clone());
    let document = html_document::parse(&document);
    let display = display_document::display(&document, glyph_brush.fonts(), -270., MARGIN, 1.);
    let connection_number = {
        let plain_window = window.window();
        let connection = plain_window.get_xlib_xconnection().unwrap();
        let display = plain_window.get_xlib_display().unwrap();
        unsafe { (connection.xlib.XConnectionNumber)(display as _) }
    };
    let mut events = mio::Events::with_capacity(1);
    let poll = mio::Poll::new().unwrap();
    poll.register(
        &mio::unix::EventedFd(&connection_number),
        mio::Token(0),
        mio::Ready::readable(),
        mio::PollOpt::edge(),
    )
    .unwrap();
    let mut paint = true;
    let mut running = true;
    let mut scroll = 0.;
    loop {
        events_loop.poll_events(|event| match event {
            glutin::Event::WindowEvent {
                event,
                window_id: _,
            } => match event {
                WindowEvent::CloseRequested => running = false,
                WindowEvent::MouseWheel { delta, .. } => {
                    let new_scroll = (scroll
                        - match delta {
                            glutin::MouseScrollDelta::LineDelta(_, value) => {
                                64. * window.get_hidpi_factor() as f32 * value
                            }
                            glutin::MouseScrollDelta::PixelDelta(position) => {
                                position.to_physical(window.get_hidpi_factor()).y as f32
                            }
                        })
                    .min(display.bound_y_max() + MARGIN - color_view.get_dimensions().1 as f32)
                    .max(0.);
                    if new_scroll != scroll {
                        paint = true;
                        scroll = new_scroll;
                    }
                }
                WindowEvent::Refresh => paint = true,
                WindowEvent::Resized(_) => {
                    gfx_window_glutin::update_views(&window, &mut color_view, &mut depth_view);
                    scroll = scroll
                        .min(display.bound_y_max() + MARGIN - color_view.get_dimensions().1 as f32)
                        .max(0.);
                    paint = true;
                }
                _ => {}
            },
            _ => {}
        });
        if !running {
            break;
        }
        if !paint {
            let _ = poll.poll(&mut events, None);
            continue;
        }
        let scroll = scroll.round();
        let (window_size_x, window_size_y, _, _) = color_view.get_dimensions();
        let offset = gfx_glyph::Vector {
            x: (0.5 * window_size_x as f32).round(),
            y: -scroll,
        };
        glyph_brush.queue_section(gfx_glyph::Section {
            bounds: gfx_glyph::Rect {
                max: gfx_glyph::Point {
                    x: std::f32::INFINITY,
                    y: std::f32::INFINITY,
                },
                min: gfx_glyph::Point { x: 0., y: 0. },
            },
            glyphs: display
                .clip(scroll, scroll + window_size_y as f32)
                .iter()
                .map(|glyph| glyph.translated(offset))
                .collect(),
            z: 0.,
        });
        glyph_brush
            .draw_queued(&mut encoder, &color_view, &depth_view)
            .unwrap();
        encoder.flush(&mut device);
        window.swap_buffers().unwrap();
        device.cleanup();
        encoder.clear(&color_view, BACKGROUND_COLOR);
    }
}
