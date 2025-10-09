use crate::app::{launch_emulator, ToEmulatorMessage, ToUiMessage};
use crate::emulator::FRAME;
use crate::joypad::Button;
use minifb::Scale::X2;
use minifb::{Key, Window, WindowOptions};
use tokio::sync::broadcast::{channel, Receiver, Sender};
use crate::Args;
use crate::color::PALETTE_U32;
use crate::constants::*;

pub fn main_minifb(args: Args) {
    // Create a new Tokio runtime to execute the async code
    let runtime = tokio::runtime::Runtime::new().unwrap();

    // Use the runtime to block on the async function
    runtime.block_on(run_minifb_async(args));
}

async fn run_minifb_async(args: Args) {
    let (sender_to_ui, receiver_from_ui): (Sender<ToUiMessage>, Receiver<ToUiMessage>)
        = channel(10);
    let (sender_to_emulator, receiver_from_emulator):  (Sender< ToEmulatorMessage >, Receiver<ToEmulatorMessage >)
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
        WIDTH,
        HEIGHT,
        WindowOptions {
            scale: X2,
            ..Default::default()
        },
    )
        .unwrap_or_else(|e| {
            panic!("{}", e);
        });

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

    let mut buffer: Vec<u32> = vec![0; WIDTH * HEIGHT];
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
        window
            .update_with_buffer(&buffer, WIDTH, HEIGHT)
            .unwrap();
        window.set_title(shared_state2.read().unwrap().title.as_str());
    }
}
