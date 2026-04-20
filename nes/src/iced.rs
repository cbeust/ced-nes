use crate::color::PALETTE_TUPLES;
use crate::app::launch_emulator;
use crate::constants::*;
use crate::emulator::FRAME;
use crate::{app, Args};
use fast_image_resize as fr;
use fast_image_resize::images::Image;
use ::iced::{application, settings, window, Size, Task};
use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use tokio::sync::broadcast::channel;

const UPSCALE_ALGORITHM_COUNT: usize = 5;
static MINIFB_UPSCALE_ALGORITHM_INDEX: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn cycle_minifb_upscale_algorithm() {
    let next = (MINIFB_UPSCALE_ALGORITHM_INDEX.load(Ordering::Relaxed) + 1) % UPSCALE_ALGORITHM_COUNT;
    MINIFB_UPSCALE_ALGORITHM_INDEX.store(next, Ordering::Relaxed);
}

pub(crate) fn next_minifb_upscale_algorithm_name() -> &'static str {
    match (MINIFB_UPSCALE_ALGORITHM_INDEX.load(Ordering::Relaxed) + 1) % UPSCALE_ALGORITHM_COUNT {
        0 => "Nearest",
        1 => "Box",
        2 => "Bilinear",
        3 => "Bicubic",
        4 => "Lanczos3",
        _ => "Nearest",
    }
}

fn minifb_resize_algorithm() -> fr::ResizeAlg {
    match MINIFB_UPSCALE_ALGORITHM_INDEX.load(Ordering::Relaxed) {
        0 => fr::ResizeAlg::Nearest,
        1 => fr::ResizeAlg::Convolution(fr::FilterType::Box),
        2 => fr::ResizeAlg::Convolution(fr::FilterType::Bilinear),
        3 => fr::ResizeAlg::Convolution(fr::FilterType::CatmullRom),
        4 => fr::ResizeAlg::Convolution(fr::FilterType::Lanczos3),
        _ => fr::ResizeAlg::Nearest,
    }
}

pub fn main_iced(args: Args, roms: Vec<RomInfo>, rom_info: RomInfo) {
    let (sender_to_ui, _receiver_from_ui) = channel(10);
    let (sender_to_emulator, receiver_from_emulator) = channel(10);
    let (shared_state2, joypad) = launch_emulator(args.clone(), rom_info.clone(),
        sender_to_ui.clone(), receiver_from_emulator);

    let selected_rom_id = roms.iter().position(|rom| rom.id == rom_info.id);
    let app = app::App::new(args, shared_state2, roms, selected_rom_id, sender_to_ui,
        sender_to_emulator, joypad);

   // launch_minifb_mirror();

    let window_settings = window::settings::Settings {
        size: Size::new(WIDTH as f32 * SCALE_X + 700.0, HEIGHT as f32 * SCALE_Y),  // Set a reasonable initial size
        resizable: true,  // Allow the user to resize the window
        ..window::settings::Settings::default()
    };
    let settings = settings::Settings {
        antialiasing: true,
        ..settings::Settings::default()
    };
    // let title = WindowTitle::new(receiver);
    let _ = application(app::App::title, app::App::update, app::App::view)
        .subscription(app::App::subscription)
        .settings(settings)
        .window(
            window_settings,
        )
        .run_with(move || {
            (app, Task::none())
        });
    // iced::daemon(app.clone(), App::update, App::view)
    //     .settings(settings)
    //     .subscription(App::subscription)
    //     .run_with(move || {
    //         (app, window::open(window_settings.clone()).1.map(AppMessage::MainWindowOpened))
    //     }).unwrap()
}

fn launch_minifb_mirror() {
    let _ = thread::Builder::new()
        .name("NES minifb mirror".to_string())
        .spawn(|| {
            let mut window = match Window::new(
                "NES minifb mirror - ESC to exit",
                WIDTH * 4,
                HEIGHT * 4,
                WindowOptions {
                    resize: false,
                    scale: Scale::X1,
                    scale_mode: ScaleMode::UpperLeft,
                    ..Default::default()
                },
            ) {
                Ok(window) => window,
                Err(_) => return,
            };
            window.set_position(300, 0);

            let mut buffer = vec![0_u32; WIDTH * HEIGHT];
            let mut src_rgba = vec![0_u8; WIDTH * HEIGHT * 4];
            let mut dst_rgba = vec![0_u8; WIDTH * HEIGHT * 4 * 16];
            let mut scaled_buffer = vec![0_u32; WIDTH * HEIGHT * 16];
            let mut src_image = Image::from_vec_u8(
                WIDTH as u32,
                HEIGHT as u32,
                src_rgba.clone(),
                fr::PixelType::U8x4,
            )
                .unwrap();
            let mut dst_image = Image::from_vec_u8(
                (WIDTH * 4) as u32,
                (HEIGHT * 4) as u32,
                dst_rgba.clone(),
                fr::PixelType::U8x4,
            )
                .unwrap();
            let mut resizer = fr::Resizer::new();
            while window.is_open() && !window.is_key_down(Key::Escape) {
                unsafe {
                    let frame = &raw const FRAME;
                    for (index, color) in (*frame).iter().enumerate() {
                        let (r, g, b) = PALETTE_TUPLES[*color as usize];
                        buffer[index] = ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
                    }
                }

                for (i, pixel) in buffer.iter().enumerate() {
                    let b = (*pixel & 0xFF) as u8;
                    let g = ((*pixel >> 8) & 0xFF) as u8;
                    let r = ((*pixel >> 16) & 0xFF) as u8;
                    let base = i * 4;
                    src_rgba[base] = r;
                    src_rgba[base + 1] = g;
                    src_rgba[base + 2] = b;
                    src_rgba[base + 3] = 255;
                }

                src_image.buffer_mut().copy_from_slice(&src_rgba);
                let resize_options = fr::ResizeOptions::new().resize_alg(minifb_resize_algorithm());
                resizer.resize(&src_image, &mut dst_image, Some(&resize_options)).unwrap();
                dst_rgba.copy_from_slice(dst_image.buffer());

                for (i, px) in dst_rgba.chunks_exact(4).enumerate() {
                    let r = px[0] as u32;
                    let g = px[1] as u32;
                    let b = px[2] as u32;
                    scaled_buffer[i] = (r << 16) | (g << 8) | b;
                }

                if window.update_with_buffer(&scaled_buffer, WIDTH * 4, HEIGHT * 4).is_err() {
                    break;
                }

                thread::sleep(Duration::from_millis(16));
            }
        });
}
