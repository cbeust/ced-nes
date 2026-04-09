use crate::color::PALETTE_TUPLES;
use crate::constants::{RomInfo, *};
use crate::emulator::{Emulator, FRAME};
use crate::joypad::{Button, Joypad};
use crate::rom_list::create_rom_item;
use crate::Args;
use iced::alignment::Horizontal;
use iced::keyboard::Key;
use iced::mouse::Cursor;
use iced::widget::canvas::{Cache, Fill, Geometry, Program};
use iced::widget::scrollable::Id;
use iced::widget::{button, column, container, row, scrollable, text, text_input};
use iced::widget::{checkbox, Canvas, Column, Row};
use iced::*;
use iced_futures::backend::default::time::every;
use iced_futures::core::SmolStr;
use iced_futures::Subscription;
use rand::Rng;
use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::broadcast::{Receiver, Sender};
use tracing::{info};

pub struct App {
    args: Args,
    // Drawing
    cache: Cache,
    // State shared by the app and the emulator
    shared_state: Arc<RwLock<SharedState>>,
    // This sender is also used to create a receiver
    sender_to_ui: Sender<ToUiMessage>,
    sender_to_emulator: Sender<ToEmulatorMessage>,
    joypad: Arc<RwLock<Joypad>>,
    roms: Vec<RomInfo>,
    selected_rom_index: Option<usize>,
    filter_text: String,
    scroll_id: Id,
    triangle_enabled: bool,
    pulse1_enabled: bool,
    pulse2_enabled: bool,
    noise_enabled: bool,
}

pub struct SharedState {
    pub title: String,
    pub _joypad1: String,
    pub rom_name: String,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            title: String::from(WINDOW_TITLE),
            _joypad1: String::from("Joypad 1"),
            rom_name: "".into(),
        }
    }
}

// Then in your main code:

impl App {
    pub fn new(args: Args,
        shared_state: Arc<RwLock<SharedState>>,
        roms: Vec<RomInfo>, selected_rom_index: Option<usize>,
        sender_to_ui: Sender<ToUiMessage>,
        sender_to_emulator: Sender<ToEmulatorMessage>,
        joypad: Arc<RwLock<Joypad>>)
        -> Self
    {
        Self {
            args,
            shared_state,
            joypad,
            cache: Cache::default(),
            sender_to_ui,
            sender_to_emulator,
            roms,
            selected_rom_index,
            filter_text: String::new(),
            scroll_id: Id::unique(),
            triangle_enabled: true,
            pulse1_enabled: true,
            pulse2_enabled: true,
            noise_enabled: true,
        }
    }

    pub fn subscription(&self) -> Subscription<AppMessage> {
        let rec = self.sender_to_ui.clone().subscribe();
        let cpu = iced_futures::futures::stream::unfold(
            (rec, Arc::new(Mutex::new(String::new()))),
            |(mut receiver, state)| async move {
                let mut result = None;
                if let Ok(m) = receiver.recv().await {
                    use ToUiMessage::*;
                    match m {
                        Update(frequency, fps) => {
                            result = Some(AppMessage::Update(frequency, fps));
                        }
                    }
                }

                Some((result.unwrap_or(AppMessage::Ignored), (receiver, state)))
            }
        );
        let cpu2 = Subscription::run_with_id(42, cpu);

        let mut subscriptions = vec![
            // sub1,
            event::listen().map(AppMessage::GlobalEvent),
            cpu2,
            // every,
            // window::close_events().map(WindowClosed),
            // stream,
        ];

        if self.args.demo {
            let every = every(Duration::from_secs(DEMO_DELAY_SECONDS))
                .map(move |_| AppMessage::RebootRandom);
            subscriptions.push(every);
        }

        Subscription::batch(subscriptions)
    }
}

// impl iced::daemon::Title<App> for App {
//     fn title(&self, state: &App, _window_id: window::Id) -> String {
//         let title = self.shared_state.read().unwrap().title.clone();
//         println!("TITLE: {title}");
//         title
//     }
// }

#[derive(Debug, Clone)]
pub enum AppMessage {
    Ignored,
    GlobalEvent(Event),
    // Frequency, FPS
    Update(f32, u16),
    RomSelected(usize),
    Reboot,
    Debug,
    RebootRandom,
    FilterTextChanged(String),
    TriangleToggled(bool),
    Pulse1Toggled(bool),
    Pulse2Toggled(bool),
    NoiseToggled(bool),
}

#[derive(Copy, Clone)]
pub enum ToUiMessage {
    // Frequency, FPS
    Update(f32, u16),
}

#[derive(Clone)]
pub enum ToEmulatorMessage {
    Reboot(RomInfo),
    SoundPulse1(bool),
    SoundPulse2(bool),
    SoundTriangle(bool),
    SoundNoise(bool),
    Debug,
}

#[derive(Default)]
pub struct CanvasState {}

impl Program<AppMessage> for App {
    type State = CanvasState;

    fn draw(&self, _state: &Self::State, renderer: &Renderer, _theme: &Theme, bounds: Rectangle,
        _cursor: Cursor) -> Vec<Geometry<Renderer>>
    {
        let mut result = Vec::new();

        // info!("DRAWING");
        // info!("Drawing, pixel at 1,6: {:#?}", self.frame.get_pixel(1,6));
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            // let fill = Fill::from(Color::from_rgb(1.0, 0.0, 0.0));
            // let bg = Path::rectangle(bounds.position(), bounds.size());
            // frame.fill(&bg, fill);

            // frame.scale(5.0);
            let scale_x = SCALE_X;
            let scale_y = SCALE_Y;
            unsafe {
                let f = &raw const FRAME;
                for (index, color) in (*f).iter().enumerate() {
                    let rgb = PALETTE_TUPLES[*color as usize];
                    let fill = Fill::from(Color::from_rgb8(rgb.0, rgb.1, rgb.2));
                    let x = (index % 256) as f32;
                    let y = (index / 256) as f32;
                    let xx = scale_x * x;
                    let yy = scale_y * y;

                    let size = Size::new(scale_x, scale_y);
                    let top_left = Point::new(xx, yy);
                    frame.fill_rectangle(top_left, size, fill);
                }
            }
        });

        result.push(geometry);
        self.cache.clear();
        result
    }
}

impl App {
    pub fn title(&self) -> String {
        self.shared_state.read().unwrap().title.clone()
    }

    pub fn update(&mut self, message: AppMessage) -> Task<AppMessage> {
        use AppMessage::*;
        match message {
            Ignored => {
            }
            GlobalEvent(event) => {
                if let Event::Keyboard(keyboard_event) = event {
                    match keyboard_event {
                        // Handle key press events
                        keyboard::Event::KeyPressed { key, .. } => {
                            self.key_pressed(key);
                        }
                        keyboard::Event::KeyReleased { key, .. } => {
                            self.key_released(key);
                        }
                        _ => {}
                    }
                }
            }
            Update(frequency, fps) => {
                if let Ok(mut state) = self.shared_state.write() {
                    let rom_name = state.rom_name.clone();
                    state.title = format!("{} - {:.02} Mhz - {fps} FPS - {rom_name}",
                        WINDOW_TITLE, frequency);
                }
            }
            RomSelected(rom_id) => {
                self.selected_rom_index = Some(rom_id);
                info!("Selected ROM at index {}", rom_id);
                
                // Calculate scroll position to center the selected item
                // let filtered_count = self.roms.iter()
                //     .filter(|rom| {
                //         if self.filter_text.is_empty() {
                //             true
                //         } else {
                //             rom.name().to_lowercase().contains(&self.filter_text.to_lowercase())
                //         }
                //     })
                //     .count();
                
                // let selected_position = self.roms.iter().enumerate()
                //     .filter(|(_, rom)| {
                //         if self.filter_text.is_empty() {
                //             true
                //         } else {
                //             rom.name().to_lowercase().contains(&self.filter_text.to_lowercase())
                //         }
                //     })
                //     .position(|(index, _)| index == rom_id);
                //
                // if let Some(pos) = selected_position {
                //     // Calculate actual item height based on component dimensions
                //     let item_height = self.calculate_rom_item_height();
                //     let visible_height = 400.0; // Approximate visible height of the list
                //     let center_offset = pos as f32 * item_height - (visible_height / 2.0) + (item_height / 2.0);
                //     let scroll_offset = center_offset.max(0.0);
                //     info!("Scrolling to position {} with calculated height {} and offset {}", pos, item_height, scroll_offset);
                //
                //     return scrollable::scroll_to(
                //         self.scroll_id.clone(),
                //         AbsoluteOffset { x: 0.0, y: scroll_offset }
                //     );
                // }
            }
            Reboot => {
                // self.shared_state.write().unwrap().selected_rom_index = index;
                // let _ = self.sender.send(
                //     CpuMessage::Reboot(self.shared_state.write().unwrap().selected_rom_index));
                info!("Requesting reboot");
                if let Some(index) = self.selected_rom_index {
                    let rom_info = self.roms[index].clone();
                    let _ = self.sender_to_emulator.send(ToEmulatorMessage::Reboot(rom_info));
                }
            }
            Debug => {
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::Debug);
            }
            RebootRandom => {
                let index = rand::thread_rng().gen_range(0..self.roms.len());
                info!("Selected random ROM index {}/{}", index, self.roms.len());
                return self.update(RomSelected(index)).chain(self.update(Reboot));
            }
            FilterTextChanged(text) => {
                self.filter_text = text;
            }
            TriangleToggled(enabled) => {
                self.triangle_enabled = enabled;
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundTriangle(enabled));
            }
            Pulse1Toggled(enabled) => {
                self.pulse1_enabled = enabled;
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundPulse1(enabled));
            }
            Pulse2Toggled(enabled) => {
                self.pulse2_enabled = enabled;
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundPulse2(enabled));
            }
            NoiseToggled(enabled) => {
                self.noise_enabled = enabled;
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundNoise(enabled));
            }
        }

        Task::done(Ignored)
    }

    // fn create_list_item(item: &str, id: usize, is_selected: bool) -> Element<AppMessage> {
    //     let item_content = column![
    //         text(item)
    //             .size(14)
    //             // .style(Text::Color(Color::from_rgb(0.1, 0.1, 0.1))),
    //         ]
    //         .spacing(5);
    //
    //     let container_content = row![
    //         item_content.width(Length::FillPortion(3)),
    //     ]
    //     .align_y(Alignment::Center);
    //
    //     let container = container(container_content)
    //         // .padding(10)
    //         .width(Length::Fill);
    //
    //     // Apply different styling for selected items
    //     let styled_container = if is_selected {
    //         container.style(|theme| {
    //             container::Style {
    //                 text_color: Some(red().into()),
    //                 ..Default::default()
    //             }
    //         })
    //     } else {
    //         container
    //     };
    //
    //     // Make the container clickable
    //     button(styled_container)
    //         .on_press(AppMessage::RomSelected(id))
    //         .width(Length::Fill)
    //         .into()
    // }

    fn rom_info_box(&self) -> Element<'_, AppMessage> {
        let (name, mapper_number) = if let Some(index) = self.selected_rom_index {
            let current_rom = &self.roms[index];
            let mapper_num_str = current_rom.mapper_number().to_string();
            (current_rom.name(), mapper_num_str)
        } else {
            ("??".into(), "0".into())
        };
        
        let info_content = row![
            text(name)
                .size(14)
                .style(|_theme| text::Style { color: Some(Color::from_rgb(1.0, 1.0, 1.0)) })
                .width(Length::Fill),
            text(mapper_number)
                .size(12)
                .style(|_theme| text::Style { color: Some(Color::from_rgb(1.0, 1.0, 0.0)) })
        ]
        .align_y(Alignment::Start)
        .spacing(20)
        .padding(15);

        container(info_content)
            .style(|_theme| {
                container::Style {
                    background: Some(Color::from_rgb(0.25, 0.35, 0.45).into()),
                    border: Border {
                        color: Color::from_rgb(0.5, 0.6, 0.7),
                        width: 2.0,
                        radius: 8.0.into(),
                    },
                    ..Default::default()
                }
            })
            .width(Length::Fill)
            .into()
    }

    fn rom_list(&self) -> Element<'_, AppMessage> {
        let items_column = self.roms.iter().enumerate()
            .filter(|(_, rom)| {
                if self.filter_text.is_empty() {
                    true
                } else {
                    rom.name().to_lowercase().contains(&self.filter_text.to_lowercase())
                }
            })
            .fold(
                column![].spacing(2),
                |col, (index, it)| {
                    let item = it.clone();
                    let is_selected = if let Some(si) = self.selected_rom_index {
                        if si == index { true } else { false }
                    } else {
                        false
                    };
                    col.push(create_rom_item(is_selected, item).map(move |message| {
                        match message {
                            crate::listview::Message::ItemClicked(_) => AppMessage::RomSelected(index),
                        }
                    }))
                }
            );

        container(
            scrollable(items_column)
                .id(self.scroll_id.clone())
                .width(Length::Fill)
                .height(Length::Fill)
        )
        .style(|_theme| {
            container::Style {
                background: Some(Color::from_rgb(0.4, 0.4, 0.4).into()),
                ..Default::default()
            }
        })
        .into()
    }

    fn rom_panel(&self) -> Element<'_, AppMessage> {
        let filter_input = text_input("Filter ROMs...", &self.filter_text)
            .on_input(AppMessage::FilterTextChanged)
            .padding(10)
            .size(16)
            .style(|theme, status| {
                let mut style = text_input::default(theme, status);
                style.border.radius = 10.0.into();
                style
            });

        column![
            self.rom_info_box(),
            filter_input,
            self.rom_list()
        ]
        .spacing(10)
        .into()
    }

    pub fn view(&self) -> Element<'_, AppMessage> {
        let canvas = Canvas::new(self)
            .width(Length::Fixed(WIDTH as f32 * SCALE_X))
            .height(Length::Fixed(HEIGHT as f32 * SCALE_Y));

        let buttons = container(Column::new()
            .spacing(10)
            .width(Length::Fixed(150.0))
            .push(m_button("Reboot", AppMessage::Reboot, None))
            .push(m_button("Random", AppMessage::RebootRandom, None))
            .push(m_button("Debug", AppMessage::Debug, Some(Color::from_rgb(0.2, 0.2, 0.8))))
            .push(container(Column::new()
                .spacing(10)
                .push(checkbox("Triangle", self.triangle_enabled).on_toggle(AppMessage::TriangleToggled))
                .push(checkbox("Pulse1", self.pulse1_enabled).on_toggle(AppMessage::Pulse1Toggled))
                .push(checkbox("Pulse2", self.pulse2_enabled).on_toggle(AppMessage::Pulse2Toggled))
                .push(checkbox("Noise", self.noise_enabled).on_toggle(AppMessage::NoiseToggled))
            )
            .padding(10)
            .width(Length::Fill)
            .style(|_theme| {
                container::Style {
                    border: Border {
                        color: Color::BLACK,
                        width: 1.0,
                        radius: 5.0.into(),
                    },
                    ..Default::default()
                }
            }))
        )
            .style(|_theme| {
                container::Style {
                    background: Some(Color::from_rgb(0.2, 0.2, 0.2).into()),
                    ..Default::default()
                }
            })
            .width(Shrink)
            .height(Shrink);

        let row = Row::new()
            .spacing(10)
            .push(canvas)
            .push(buttons)
            .push(container(self.rom_panel()).width(Length::Fill))
            ;

        let mut column= Column::new();
        // column = column.push(text("NES Emulator").size(50));
        column = column.push(row);

        container(column)
            .padding(10)
            .style(|_theme| {
                container::Style {
                    background: Some(Color::from_rgb(0.2, 0.2, 0.2).into()),
                    ..Default::default()
                }
            })
            .into()
    }

    fn key_to_button(key: Key<SmolStr>) -> Option<Button> {
        let mut result = None;
        use iced::keyboard::key::Named;
        match key {
            Key::Named(named) => {
                match named {
                    Named::Enter => { result = Some(Button::Select) }
                    Named::Space => { result = Some(Button::Start) }
                    Named::ArrowUp => { result = Some(Button::Up) }
                    Named::ArrowDown => { result = Some(Button::Down) }
                    Named::ArrowLeft => { result = Some(Button::Left) }
                    Named::ArrowRight => { result = Some(Button::Right) }
                    _ => {}
                }
            }
            Key::Character(c) => {
                if c == "a" { result = Some(Button::A); }
                else if c == "b" { result = Some(Button::B); }
            }
            Key::Unidentified => {}
        }
        result
    }

    fn key_pressed(&mut self, key: Key<SmolStr>) {
        // info!("Key pressed: {key:#?}");
        if let Some(button) = Self::key_to_button(key) {
            self.joypad.write().unwrap().set_button_status(button, true);
        }
    }

    fn key_released(&mut self, key: Key<SmolStr>) {
        // info!("Key released: {key:#?}");
        if let Some(button) = Self::key_to_button(key) {
            self.joypad.write().unwrap().set_button_status(button, false);
        }
    }
}

pub fn launch_emulator(args: Args, mut rom_info: RomInfo,
    sender: Sender<ToUiMessage>, mut receiver: Receiver<ToEmulatorMessage>) ->
    (Arc<RwLock<SharedState>>, Arc<RwLock<Joypad>>)
{
    let shared_state = Arc::new(RwLock::new(SharedState::default()));
    let shared_state2 = shared_state.clone();

    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let joypad2 = joypad.clone();
    let _ = thread::Builder::new()
        .name("NES emulator thread".to_string())
        .spawn(move|| {
        let mut reboot = false;
        loop {
            let mut emulator = Emulator::new(rom_info.clone(),
                shared_state.clone(), joypad2.clone(), args.clone());
            let mut one_second_start = Instant::now();
            let mut one_second_cycles = 0;

            while ! reboot {
                let cycles = emulator.tick();
                one_second_cycles += cycles;

                // Refresh the frequency display every second
                let elapsed = one_second_start.elapsed().as_millis();
                if elapsed > 1000 {
                    let frames = emulator.frame_stats.len();
                    let frequency = one_second_cycles as f32 / (elapsed as f32 * 1000.0);
                    let _ = sender.send(ToUiMessage::Update(frequency, frames as u16));
                    emulator.frame_stats.clear();
                    one_second_cycles = 0;
                    one_second_start = Instant::now();
                }

                if let Some(cap) = CAP_FPS {
                    // If CAP_FPS is set to 60 and the divider is 10, we want
                    // to run 6 (FPS / divider) frames every 100 (1000 / 10) milliseconds
                    // The higher the divider, the smoother the scrolling, up to a point
                    // (if the divider is too high, it makes the emulator uncapped)
                    let divider = 30_u128;
                    // Divider = 10, caps = 40 fps, need to run 4 frames every 100ms
                    let frame_cap_divided = cap as u128 / divider;
                    let time_wait_ms = 1000 / divider;
                    let frame_count = emulator.frame_count.len();
                    // let frame_count_divided = frame_count / divider as usize;
                    // info!("Frame count:{frame_count} time_wait:{time_wait_ms}");
                    if frame_count as u128 >= frame_cap_divided {
                        let elapsed = emulator.frame_count_last.elapsed().as_millis();
                        // info!("  elapsed: {elapsed}");
                        if elapsed < time_wait_ms {
                            let sleep_ms = time_wait_ms - elapsed;
                            // info!("  sleeping {sleep_ms}");
                            thread::sleep(Duration::from_millis(sleep_ms as u64));
                            emulator.frame_count.drain(0..frame_cap_divided as usize);
                            emulator.frame_count_last = Instant::now();
                        }
                    }
                }

                while let Ok(m) = receiver.try_recv() {
                    match m {
                        ToEmulatorMessage::Reboot(ri) => {
                            info!("Emulator rebooting with {ri:#?})");
                            reboot = true;
                            rom_info = ri;
                        }
                        ToEmulatorMessage::Debug => {
                            emulator.debug();
                        }
                        ToEmulatorMessage::SoundPulse1(enabled) => {
                            emulator.apu.write().unwrap().set_pulse1_enabled(enabled);
                        }
                        ToEmulatorMessage::SoundPulse2(enabled) => {
                            emulator.apu.write().unwrap().set_pulse2_enabled(enabled);
                        }
                        ToEmulatorMessage::SoundTriangle(enabled) => {
                            emulator.apu.write().unwrap().set_triangle_enabled(enabled);
                        }
                        ToEmulatorMessage::SoundNoise(enabled) => {
                            emulator.apu.write().unwrap().set_noise_enabled(enabled);
                        }
                    }
                }
            }
            reboot = false;
        }
    });

    (shared_state2, joypad)
}

/// A bigger and round button
pub fn m_button(label: &str, message: AppMessage, color: Option<Color>) -> widget::Button<'_, AppMessage> {
    button(
        text(label)
            .align_x(Horizontal::Center)
            .size(20.0)
            .width(Length::Fixed(150.0)))
        .on_press(message)
        .style(move |_theme, status| {
            let base_color = color.unwrap_or(Color::from_rgb(0.8, 0.2, 0.2));
            let hover_color = Color::from_rgb(
                (base_color.r + 0.1).min(1.0),
                (base_color.g + 0.1).min(1.0),
                (base_color.b + 0.1).min(1.0),
            );

            let background = match status {
                button::Status::Hovered | button::Status::Pressed => hover_color,
                _ => base_color,
            };

            button::Style {
                background: Some(background.into()),
                text_color: Color::WHITE,
                border: Border {
                    color: Color::BLACK,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            }
        })
}
