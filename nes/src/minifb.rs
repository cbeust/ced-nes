use crate::app::{launch_emulator, ToEmulatorMessage, ToUiMessage};
use crate::constants::*;
use crate::joypad::Button;
use crate::Args;
use fast_image_resize as fr;
use fast_image_resize::images::Image;
use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};
use tokio::sync::broadcast::{channel, Receiver, Sender};

pub fn main_minifb(args: Args) {
    // Create a new Tokio runtime to execute the async code
    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Use the runtime to block on the async function
    runtime.block_on(run_minifb_async(args));
}

async fn run_minifb_async(args: Args) {
    let (sender_to_ui, _receiver_from_ui): (Sender<ToUiMessage>, Receiver<ToUiMessage>)
        = channel(10);
    let (_sender_to_emulator, receiver_from_emulator):  (Sender< ToEmulatorMessage >, Receiver<ToEmulatorMessage >)
        = channel(10);
    let d = RomInfo::default().file_name.clone();
    let file_name = args.rom_names.first().unwrap_or(&d);
    let rom_info = RomInfo::n(0, file_name);
    let (shared_state2, joypad) = launch_emulator(args, rom_info,
        sender_to_ui.clone(), receiver_from_emulator);

    //
    // minifb
    //
    let mut window = Window::new(
        "Test - ESC to exit",
        WIDTH * 4,
        HEIGHT * 4,
        WindowOptions {
            resize: false,
            scale: Scale::X1,
            scale_mode: ScaleMode::UpperLeft,
            ..Default::default()
        },
    )
        .unwrap_or_else(|e| {
            panic!("{}", e);
        });
    window.set_position(500, 0);

    // Limit to max ~60 fps update rate
    // window.set_target_fps(60);

    let map = [
        (Key::Space, Button::Start),
        (Key::Enter, Button::Select),
        (Key::Right, Button::Right),
        (Key::Left, Button::Left),
        (Key::Up, Button::Up),
        (Key::Down, Button::Down),
        (Key::A, Button::A),
        (Key::B, Button::B),
    ];
    let mut run = true;

    let buffer: Vec<u32> = vec![0; WIDTH * HEIGHT];
    let mut src_rgba = vec![0u8; WIDTH * HEIGHT * 4];
    let mut dst_rgba = vec![0u8; WIDTH * HEIGHT * 4 * 16];
    let mut scaled_buffer = vec![0u32; WIDTH * HEIGHT * 16];
    let mut src_image = Image::from_vec_u8(WIDTH as u32, HEIGHT as u32, src_rgba.clone(), fr::PixelType::U8x4).unwrap();
    let mut dst_image = Image::from_vec_u8((WIDTH * 4) as u32, (HEIGHT * 4) as u32, dst_rgba.clone(), fr::PixelType::U8x4).unwrap();
    let mut resizer = fr::Resizer::new();
    let resize_options = fr::ResizeOptions::new()
        .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Bilinear));
    while run {
        run = window.is_open() && !window.is_key_down(Key::Escape);
        for (key, button) in &map {
            let down = window.is_key_down(*key);
            joypad.write().unwrap().set_button_status(*button, down);
        }

        // let b = FRAME.read().unwrap();
        // for index in 0..HEIGHT * WIDTH {
        //     buffer[index] = PALETTE_U32[b[index] as usize];
        // }

        // while receiver.len() > 0 {
        //     use ToUiMessage::*;
        //     match receiver.recv().await {
        //         Ok(m) => {
        //             match m {
        //                 Update(frequency, fps) => {
        //                     let rom_name = shared_state2.read().unwrap().rom_name.clone();
        //                     shared_state2.write().unwrap().title
        //                         = format!("{} - {frequency:0.2} Mhz {fps} FPS - {rom_name}",
        //                         WINDOW_TITLE);
        //                 }
        //             }
        //         }
        //         Err(_) => {}
        //     }
        // }
        // We unwrap here as we want this code to exit if it fails. Real applications may want to handle this in a different way
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
        resizer.resize(&src_image, &mut dst_image, Some(&resize_options)).unwrap();
        dst_rgba.copy_from_slice(dst_image.buffer());

        for (i, px) in dst_rgba.chunks_exact(4).enumerate() {
            let r = px[0] as u32;
            let g = px[1] as u32;
            let b = px[2] as u32;
            scaled_buffer[i] = (r << 16) | (g << 8) | b;
        }

        window
            .update_with_buffer(&scaled_buffer, WIDTH * 4, HEIGHT * 4)
            .unwrap();
        window.set_title(shared_state2.read().unwrap().title.as_str());
    }
}
