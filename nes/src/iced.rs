use crate::app::launch_emulator;
use crate::constants::*;
use crate::{app, Args};
use ::iced::{application, settings, window, Size, Task};
use tokio::sync::broadcast::channel;

pub fn main_iced(args: Args, roms: Vec<RomInfo>, rom_info: RomInfo) {
    let (sender_to_ui, _receiver_from_ui) = channel(10);
    let (sender_to_emulator, receiver_from_emulator) = channel(10);
    let (shared_state2, joypad) = launch_emulator(args.clone(), rom_info.clone(),
        sender_to_ui.clone(), receiver_from_emulator);

    let selected_rom_id = roms.iter().position(|rom| rom.id == rom_info.id);
    let app = app::App::new(args, shared_state2, roms, selected_rom_id, sender_to_ui,
        sender_to_emulator, joypad);
    let window_settings = window::settings::Settings {
        size: Size::new(WIDTH as f32 * SCALE_X + 400.0, HEIGHT as f32 * SCALE_Y),  // Set a reasonable initial size
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
