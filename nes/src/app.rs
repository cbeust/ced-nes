use crate::color::PALETTE_TUPLES;
use crate::config_file::EmulatorConfig;
use crate::constants::{RomInfo, *};
use crate::emulator::{CpuInterface, Emulator, FRAME};
use crate::joypad::{Button, Joypad};
use crate::rom_list::create_rom_item;
use crate::Args;
use gilrs::{Axis, Button as GilButton, EventType, Gilrs};
use iced::alignment::Horizontal;
use iced::keyboard::Key;
use iced::mouse::Cursor;
use iced::widget::canvas::{Cache, Fill, Geometry, Path, Program, Stroke};
use iced::widget::Id;
use iced::widget::{button, column, container, mouse_area, row, scrollable, text, text_input};
use iced::widget::{checkbox, Canvas, Column, Row};
use iced::*;
use iced_futures::backend::default::time::every;
use iced_futures::core::SmolStr;
use iced_futures::Subscription;
use rand::Rng;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::broadcast::{error::RecvError, Receiver, Sender};
use tracing::{info, warn};
use crate::bk2::Bk2Movie;
use crate::fm2::Fm2Movie;
use crate::rom::Rom;

/// Mutable global debug flag – toggled by the Debug button in the UI.
pub static mut DEBUG_BUTTON: bool = false;

#[derive(Clone)]
struct CpuSubscriptionData {
    id: u8,
    sender_to_ui: Sender<ToUiMessage>,
}

impl Hash for CpuSubscriptionData {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

fn cpu_subscription_stream(
    data: &CpuSubscriptionData,
) -> impl iced_futures::futures::Stream<Item = AppMessage> {
    let sender_to_ui = data.sender_to_ui.clone();
    let rec = sender_to_ui.subscribe();

    iced_futures::futures::stream::unfold(
        (rec, sender_to_ui),
        |(mut receiver, sender_to_ui)| async move {
            let mut message = match receiver.recv().await {
                Ok(ToUiMessage::Update(frequency, fps)) => {
                    AppMessage::Update(frequency, fps)
                }
                Ok(ToUiMessage::SoundSamples(samples)) => {
                    AppMessage::SoundSamples(samples)
                }
                Err(RecvError::Lagged(_)) => AppMessage::Ignored,
                Err(RecvError::Closed) => {
                    receiver = sender_to_ui.subscribe();
                    AppMessage::Ignored
                }
            };

            while let Ok(next) = receiver.try_recv() {
                message = match next {
                    ToUiMessage::Update(frequency, fps) => {
                        AppMessage::Update(frequency, fps)
                    }
                    ToUiMessage::SoundSamples(samples) => {
                        AppMessage::SoundSamples(samples)
                    }
                };
            }

            Some((message, (receiver, sender_to_ui)))
        },
    )
}

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
    config: EmulatorConfig,
    all_enabled: bool,
    triangle_enabled: bool,
    pulse1_enabled: bool,
    pulse2_enabled: bool,
    noise_enabled: bool,
    dmc_enabled: bool,
    is_paused: bool,
    show_grid: bool,
    hover_tile: Option<(u16, u16)>,
    debug_active: bool,
    waveform_samples: Vec<f32>,
    /// Cached from the last Update message so title() can rebuild the string at any time.
    last_frequency: f32,
    last_fps: u16,
}

pub struct SharedState {
    pub title: String,
    pub _joypad1: String,
    pub rom_name: String,
    pub paused_cycle_count: u128,
}

impl Default for SharedState {
    fn default() -> Self {
        Self {
            title: String::from(WINDOW_TITLE),
            _joypad1: String::from("Joypad 1"),
            rom_name: "".into(),
            paused_cycle_count: 0,
        }
    }
}

// Then in your main code:

impl App {
    const STICK_DEADZONE: f32 = 0.25;

    pub fn new(args: Args,
        shared_state: Arc<RwLock<SharedState>>,
        roms: Vec<RomInfo>, selected_rom_index: Option<usize>,
        config: EmulatorConfig,
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
            config: config.clone(),
            all_enabled: config.sound_all_enabled,
            triangle_enabled: config.sound_triangle_enabled,
            pulse1_enabled: config.sound_pulse1_enabled,
            pulse2_enabled: config.sound_pulse2_enabled,
            noise_enabled: config.sound_noise_enabled,
            dmc_enabled: config.sound_dmc_enabled,
            is_paused: false,
            show_grid: false,
            hover_tile: None,
            debug_active: false,
            waveform_samples: Vec::new(),
            last_frequency: 0.0,
            last_fps: 0,
        }
    }

    pub fn subscription(&self) -> Subscription<AppMessage> {
        let cpu2 = Subscription::run_with(
            CpuSubscriptionData {
                id: 42,
                sender_to_ui: self.sender_to_ui.clone(),
            },
            cpu_subscription_stream,
        );

        let mut subscriptions = vec![
            // sub1,
            keyboard::listen().map(AppMessage::KeyboardEvent),
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
    KeyboardEvent(keyboard::Event),
    // Frequency, FPS
    Update(f32, u16),
    RomSelected(usize),
    Reboot,
    TogglePause,
    ToggleGrid,
    CanvasHovered(Point),
    CanvasLeft,
    ToggleDebug,
    Debug,
    RebootRandom,
    FilterTextChanged(String),
    AllToggled(bool),
    TriangleToggled(bool),
    Pulse1Toggled(bool),
    Pulse2Toggled(bool),
    NoiseToggled(bool),
    DmcToggled(bool),
    SoundSamples(Vec<f32>),
}

#[derive(Clone)]
pub enum ToUiMessage {
    // Frequency, FPS
    Update(f32, u16),
    SoundSamples(Vec<f32>),
}

#[derive(Clone)]
pub enum ToEmulatorMessage {
    Reboot(RomInfo),
    Pause(bool),
    SoundPulse1(bool),
    SoundPulse2(bool),
    SoundTriangle(bool),
    SoundNoise(bool),
    SoundDmc(bool),
    Debug,
}

#[derive(Default, Clone)]
pub struct CanvasState;

#[derive(Default)]
pub struct SoundWaveformCanvas {
    samples: Vec<f32>,
}

impl Program<AppMessage> for SoundWaveformCanvas {
    type State = CanvasState;

    fn draw(&self, _state: &Self::State, renderer: &Renderer, _theme: &Theme, bounds: Rectangle,
        _cursor: Cursor) -> Vec<Geometry<Renderer>>
    {
        let mut frame = widget::canvas::Frame::new(renderer, bounds.size());

        frame.fill_rectangle(Point::ORIGIN, bounds.size(), Fill::from(Color::BLACK));

        let samples = &self.samples;

        if samples.len() >= 2 {
            let margin = 5.0;
            let plot_width = (bounds.width - 2.0 * margin).max(1.0);
            let plot_height = (bounds.height - 2.0 * margin).max(1.0);
            let center_y = margin + plot_height / 2.0;
            let amplitude = plot_height / 2.0;

            let min_sample = samples
                .iter()
                .copied()
                .fold(f32::INFINITY, f32::min);
            let max_sample = samples
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            let sample_mid = (min_sample + max_sample) * 0.5;
            let half_range = ((max_sample - min_sample) * 0.5).max(1e-6);

            let baseline = Path::line(
                Point::new(margin, center_y),
                Point::new(margin + plot_width, center_y),
            );
            frame.stroke(
                &baseline,
                Stroke::default()
                    .with_color(Color::from_rgb(0.2, 0.2, 0.2))
                    .with_width(1.0),
            );

            let waveform = Path::new(|builder| {
                let max_index = (samples.len() - 1) as f32;
                for (index, sample) in samples.iter().enumerate() {
                    let x = margin + (index as f32 / max_index) * plot_width;
                    let centered = ((*sample - sample_mid) / half_range).clamp(-1.0, 1.0);
                    let y = center_y - centered * amplitude;

                    if index == 0 {
                        builder.move_to(Point::new(x, y));
                    } else {
                        builder.line_to(Point::new(x, y));
                    }
                }
            });

            frame.stroke(
                &waveform,
                Stroke::default()
                    .with_color(Color::from_rgb(0.0, 1.0, 0.0))
                    .with_width(1.5),
            );
        }

        vec![frame.into_geometry()]
    }
}

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
            if let Ok(frame_buffer) = FRAME.read() {
                for (index, color) in frame_buffer.iter().enumerate() {
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

            if self.show_grid {
                let grid_color = Color::from_rgba(0.8, 0.8, 0.8, 0.45);
                let grid_x_step = 8.0 * scale_x;
                let grid_y_step = 8.0 * scale_y;
                let max_x = WIDTH as f32 * scale_x;
                let max_y = HEIGHT as f32 * scale_y;

                let mut x = 0.0;
                while x <= max_x {
                    let line = Path::line(Point::new(x, 0.0), Point::new(x, max_y));
                    frame.stroke(&line, Stroke::default().with_color(grid_color).with_width(1.0));
                    x += grid_x_step;
                }

                let mut y = 0.0;
                while y <= max_y {
                    let line = Path::line(Point::new(0.0, y), Point::new(max_x, y));
                    frame.stroke(&line, Stroke::default().with_color(grid_color).with_width(1.0));
                    y += grid_y_step;
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
        let state = self.shared_state.read().unwrap();
        let rom_name = state.rom_name.clone();
        let base = format!("{} - {:.02} Mhz - {} FPS - {}",
            WINDOW_TITLE, self.last_frequency, self.last_fps, rom_name);
        if self.is_paused {
            return format!("{base} | Paused | Cycle {}", state.paused_cycle_count);
        }
        if self.show_grid {
            if let Some((tile_x, tile_y)) = self.hover_tile {
                return format!("{base} | Tile ({tile_x}, {tile_y})");
            }
        }
        base
    }

    pub fn update(&mut self, message: AppMessage) -> Task<AppMessage> {
        use AppMessage::*;
        match message {
            Ignored => {
            }
            KeyboardEvent(event) => {
                match event {
                    keyboard::Event::KeyPressed { key, .. } => {
                        self.key_pressed(key);
                    }
                    keyboard::Event::KeyReleased { key, .. } => {
                        self.key_released(key);
                    }
                    _ => {}
                }
            }
            Update(frequency, fps) => {
                self.last_frequency = frequency;
                self.last_fps = fps;
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
            TogglePause => {
                self.is_paused = !self.is_paused;
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::Pause(self.is_paused));
            }
            ToggleGrid => {
                self.show_grid = !self.show_grid;
                if !self.show_grid {
                    self.hover_tile = None;
                }
            }
            CanvasHovered(position) => {
                let max_x = WIDTH as f32 * SCALE_X;
                let max_y = HEIGHT as f32 * SCALE_Y;
                if position.x >= 0.0 && position.y >= 0.0 && position.x < max_x && position.y < max_y {
                    let tile_x = (position.x / (8.0 * SCALE_X)).floor() as u16;
                    let tile_y = (position.y / (8.0 * SCALE_Y)).floor() as u16;
                    self.hover_tile = Some((tile_x, tile_y));
                } else {
                    self.hover_tile = None;
                }
            }
            CanvasLeft => {
                self.hover_tile = None;
            }
            ToggleDebug => {
                self.debug_active = !self.debug_active;
                unsafe { DEBUG_BUTTON = self.debug_active; }
            }
            Debug => {
                crate::iced::cycle_minifb_upscale_algorithm();
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
            AllToggled(enabled) => {
                self.set_all_sound_channels(enabled);
            }
            TriangleToggled(enabled) => {
                self.triangle_enabled = enabled;
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundTriangle(enabled));
                self.refresh_all_checkbox();
                self.persist_sound_config();
            }
            Pulse1Toggled(enabled) => {
                self.pulse1_enabled = enabled;
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundPulse1(enabled));
                self.refresh_all_checkbox();
                self.persist_sound_config();
            }
            Pulse2Toggled(enabled) => {
                self.pulse2_enabled = enabled;
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundPulse2(enabled));
                self.refresh_all_checkbox();
                self.persist_sound_config();
            }
            NoiseToggled(enabled) => {
                self.noise_enabled = enabled;
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundNoise(enabled));
                self.refresh_all_checkbox();
                self.persist_sound_config();
            }
            DmcToggled(enabled) => {
                self.dmc_enabled = enabled;
                let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundDmc(enabled));
                self.refresh_all_checkbox();
                self.persist_sound_config();
            }
            SoundSamples(samples) => {
                self.waveform_samples = samples;
            }
        }

        Task::done(Ignored)
    }

    fn refresh_all_checkbox(&mut self) {
        self.all_enabled = self.triangle_enabled
            && self.pulse1_enabled
            && self.pulse2_enabled
            && self.noise_enabled
            && self.dmc_enabled;
    }

    fn persist_sound_config(&mut self) {
        self.config.sound_all_enabled = self.all_enabled;
        self.config.sound_triangle_enabled = self.triangle_enabled;
        self.config.sound_pulse1_enabled = self.pulse1_enabled;
        self.config.sound_pulse2_enabled = self.pulse2_enabled;
        self.config.sound_noise_enabled = self.noise_enabled;
        self.config.sound_dmc_enabled = self.dmc_enabled;
        let _ = self.config.save();
    }

    fn set_all_sound_channels(&mut self, enabled: bool) {
        self.all_enabled = enabled;
        self.triangle_enabled = enabled;
        self.pulse1_enabled = enabled;
        self.pulse2_enabled = enabled;
        self.noise_enabled = enabled;
        self.dmc_enabled = enabled;

        let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundTriangle(enabled));
        let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundPulse1(enabled));
        let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundPulse2(enabled));
        let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundNoise(enabled));
        let _ = self.sender_to_emulator.send(ToEmulatorMessage::SoundDmc(enabled));
        self.persist_sound_config();
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

        let canvas = mouse_area(canvas)
            .on_move(AppMessage::CanvasHovered)
            .on_exit(AppMessage::CanvasLeft);

        let canvas: Element<'_, AppMessage> = canvas.into();

        let buttons = container(Column::new()
            .spacing(10)
            .width(Length::Fixed(150.0))
            .push(m_button("Reboot", AppMessage::Reboot, None))
            .push(m_button("Random", AppMessage::RebootRandom, None))
            .push(m_button(
                if self.is_paused { "Resume" } else { "Pause" },
                AppMessage::TogglePause,
                Some(Color::from_rgb(0.6, 0.5, 0.0)),
            ))
            .push(m_button(
                "Debug",
                AppMessage::ToggleDebug,
                Some(if self.debug_active {
                    Color::from_rgb(0.1, 0.7, 0.2)   // active: green
                } else {
                    Color::from_rgb(0.3, 0.3, 0.3)   // inactive: dark grey
                }),
            ))
            // .push(m_button(crate::iced::next_minifb_upscale_algorithm_name(), AppMessage::Debug, Some(Color::from_rgb(0.2, 0.2, 0.8))))
            .push(m_button(
                "Grid",
                AppMessage::ToggleGrid,
                Some(if self.show_grid {
                    Color::from_rgb(0.65, 0.65, 0.65)
                } else {
                    Color::from_rgb(0.35, 0.35, 0.35)
                }),
            ))
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
            .push(canvas)
            .push(container(Row::new()
                .spacing(10)
                .push(buttons)
                .push(container(self.rom_panel()).width(Length::Fill))
                .push(
                    container(
                        Column::new()
                            .spacing(10)
                            .push(
                                container(
                                    Row::new()
                                        .spacing(20)
                                        .push(
                                            Column::new()
                                                .spacing(10)
                                                .push(
                                                    checkbox(self.all_enabled)
                                                        .label("All")
                                                        .font(Font {
                                                            weight: iced::font::Weight::Bold,
                                                            ..Default::default()
                                                        })
                                                        .style(|theme, status| {
                                                            let mut style = checkbox::primary(theme, status);
                                                            style.text_color = Some(Color::from_rgb(1.0, 1.0, 0.0));
                                                            style
                                                        })
                                                        .on_toggle(AppMessage::AllToggled)
                                                )
                                                .push(checkbox(self.triangle_enabled).label("Triangle").on_toggle(AppMessage::TriangleToggled))
                                                .push(checkbox(self.noise_enabled).label("Noise").on_toggle(AppMessage::NoiseToggled))
                                        )
                                        .push(
                                            Column::new()
                                                .spacing(10)
                                                .push(checkbox(self.pulse1_enabled).label("Pulse 1").on_toggle(AppMessage::Pulse1Toggled))
                                                .push(checkbox(self.pulse2_enabled).label("Pulse 2").on_toggle(AppMessage::Pulse2Toggled))
                                                .push(checkbox(self.dmc_enabled).label("DMC").on_toggle(AppMessage::DmcToggled))
                                        )
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
                                })
                            )
                            .push(
                                Canvas::new(SoundWaveformCanvas {
                                    samples: self.waveform_samples.clone(),
                                })
                                    .width(Length::Fixed(300.0))
                                    .height(Length::Fixed(110.0))
                            )
                    )
                )
            )
                .padding(10)
                .height(Length::Fixed(HEIGHT as f32 * SCALE_Y))
                .align_y(alignment::Vertical::Center))
            ;

        let mut column= Column::new();
        // column = column.push(text("NES Emulator").size(50));
        column = column.push(row);

        container(column)
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

    fn map_gilrs_button(button: GilButton) -> Option<Button> {
        match button {
            GilButton::South => Some(Button::A),
            GilButton::East => Some(Button::B),
            GilButton::Start => Some(Button::Start),
            GilButton::Select => Some(Button::Select),
            _ => None,
        }
    }

    fn apply_stick_x(joypad: &mut Joypad, value: f32) {
        if value > Self::STICK_DEADZONE {
            joypad.set_button_status(Button::Right, true);
            joypad.set_button_status(Button::Left, false);
        } else if value < -Self::STICK_DEADZONE {
            joypad.set_button_status(Button::Left, true);
            joypad.set_button_status(Button::Right, false);
        } else {
            joypad.set_button_status(Button::Left, false);
            joypad.set_button_status(Button::Right, false);
        }
    }

    fn apply_stick_y(joypad: &mut Joypad, value: f32) {
        if value < -Self::STICK_DEADZONE {
            joypad.set_button_status(Button::Down, true);
            joypad.set_button_status(Button::Up, false);
        } else if value > Self::STICK_DEADZONE {
            joypad.set_button_status(Button::Up, true);
            joypad.set_button_status(Button::Down, false);
        } else {
            joypad.set_button_status(Button::Up, false);
            joypad.set_button_status(Button::Down, false);
        }
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
    let fm2_movie = if let Some(fm2_file) = &args.fm2 {
        let movie = Fm2Movie::parse_file(fm2_file).unwrap();
        let rom = Rom::read_nes_file(&rom_info.file_name()).unwrap();
        if let Some(false) = movie.rom_checksum_matches(&rom) {
            warn!(
                target: "fm2",
                rom = %rom_info.file_name(),
                rom_checksum = %rom.checksum,
                movie_checksum = %movie.rom_checksum().unwrap_or_default(),
                "FM2 movie ROM checksum does not match the loaded ROM"
            );
        }
        Some(movie)
    } else {
        None
    };

    println!("Current directory: {}", std::env::current_dir().unwrap().display());
    let mut bk2_movie = if let Some(bk2_file) = &args.bk2 {
        let movie = Bk2Movie::parse_file(bk2_file).expect(&format!("File {bk2_file} should exist"));
        Some(movie)
    } else {
        None
    };

    let joypad = Arc::new(RwLock::new(Joypad::new()));
    let joypad2 = joypad.clone();
    let _ = thread::Builder::new()
        .name("NES emulator thread".to_string())
        .spawn(move|| {
        let mut reboot = false;
        let mut paused = false;
        let mut gilrs = Gilrs::new().ok();
        // Keep the APU alive across reboots so we never recreate the audio device.
        let mut carry_apu: Option<Arc<RwLock<crate::apu::Apu>>> = None;
        loop {
            let mut emulator = Emulator::new(rom_info.clone(),
                shared_state.clone(), joypad2.clone(), args.clone(), carry_apu.take());

            if let (Some(movie), Some(rom)) = (fm2_movie.as_ref(), emulator._rom.as_ref()) {
                if let Some(false) = movie.rom_checksum_matches(rom) {
                    warn!(
                        target: "fm2",
                        rom = %rom_info.file_name(),
                        rom_checksum = %rom.checksum,
                        movie_checksum = %movie.rom_checksum().unwrap_or_default(),
                        "FM2 movie ROM checksum does not match the loaded ROM"
                    );
                }
            }




            // Apply persisted channel toggles for each fresh emulator instance (including reboots).
            if let Ok(cfg) = EmulatorConfig::read_or_create() {
                let mut apu = emulator.apu.write().unwrap();
                apu.set_pulse1_enabled(cfg.sound_pulse1_enabled);
                apu.set_pulse2_enabled(cfg.sound_pulse2_enabled);
                apu.set_triangle_enabled(cfg.sound_triangle_enabled);
                apu.set_noise_enabled(cfg.sound_noise_enabled);
                apu.set_dmc_enabled(cfg.sound_dmc_enabled);
            }

            let mut one_second_start = Instant::now();
            let mut sound_flush_start = Instant::now();
            let mut one_second_cycles = 0;

            while ! reboot {
                while let Ok(m) = receiver.try_recv() {
                    match m {
                        ToEmulatorMessage::Reboot(ri) => {
                            info!("Emulator rebooting with {ri:#?})");
                            reboot = true;
                            paused = false;
                            rom_info = ri;
                        }
                        ToEmulatorMessage::Pause(value) => {
                            if paused != value {
                                paused = value;
                                one_second_cycles = 0;
                                one_second_start = Instant::now();
                                sound_flush_start = Instant::now();
                                emulator.frame_stats.clear();
                                emulator.frame_count.clear();
                                emulator.frame_count_last = Instant::now();

                                if let Ok(mut state) = shared_state.write() {
                                    if paused {
                                        state.paused_cycle_count = emulator.cpu.get_cycles();
                                        state.title = format!(
                                            "{} - Paused - Cycle {} - {}",
                                            WINDOW_TITLE,
                                            state.paused_cycle_count,
                                            state.rom_name
                                        );
                                    } else {
                                        state.title = format!("{} - {}", WINDOW_TITLE, state.rom_name);
                                    }
                                }
                            }
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
                        ToEmulatorMessage::SoundDmc(enabled) => {
                            emulator.apu.write().unwrap().set_dmc_enabled(enabled);
                        }
                    }
                }

                if let Some(g) = gilrs.as_mut() {
                    while let Some(event) = g.next_event() {
                        match event.event {
                            EventType::ButtonPressed(button, _) => {
                                if let Some(mapped) = App::map_gilrs_button(button) {
                                    joypad2.write().unwrap().set_button_status(mapped, true);
                                }
                            }
                            EventType::ButtonReleased(button, _) => {
                                if let Some(mapped) = App::map_gilrs_button(button) {
                                    joypad2.write().unwrap().set_button_status(mapped, false);
                                }
                            }
                            EventType::AxisChanged(Axis::LeftStickX, value, _) => {
                                let mut joypad = joypad2.write().unwrap();
                                App::apply_stick_x(&mut joypad, value);
                            }
                            EventType::AxisChanged(Axis::LeftStickY, value, _) => {
                                let mut joypad = joypad2.write().unwrap();
                                App::apply_stick_y(&mut joypad, value);
                            }
                            _ => {}
                        }
                    }
                }

                if reboot {
                    continue;
                }

                if paused {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }

                let (cycles, frame_completed) = emulator.tick();
                one_second_cycles += cycles;

                // fm2
                if let Some(movie) = &mut bk2_movie {
                    if frame_completed {
                        if let Some(frame) = movie.next_state() {
                            // Apply full controller state every frame so releases are honored.
                            // Using direct button mapping avoids stick helper deadzone/inversion logic.
                            let mut joypad = joypad2.write().unwrap();
                            joypad.set_button_status(Button::Up, frame.up);
                            joypad.set_button_status(Button::Down, frame.down);
                            joypad.set_button_status(Button::Left, frame.left);
                            joypad.set_button_status(Button::Right, frame.right);
                            joypad.set_button_status(Button::A, frame.a);
                            joypad.set_button_status(Button::B, frame.b);
                            joypad.set_button_status(Button::Start, frame.start);
                            joypad.set_button_status(Button::Select, frame.select);

                            if !frame.is_empty() {
                                info!("Event: {}", frame);
                            }
                        }
                    }
                }

                let elapsed = one_second_start.elapsed().as_millis();
                let sound_elapsed = sound_flush_start.elapsed().as_millis();

                if sound_elapsed > 100 && !emulator.sound_samples.is_empty() {
                    let samples = std::mem::take(&mut emulator.sound_samples);
                    let _ = sender.send(ToUiMessage::SoundSamples(samples));
                    sound_flush_start = Instant::now();
                }

                if elapsed > 1000 {
                    // Refresh the frequency display every second
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
                    let time_wait_ms = 1000 / divider;
                    let frame_cap_divided = cap as u128 / divider;
                    let frame_count = emulator.frame_count.len();
                    // let frame_count_divided = frame_count / divider as usize;
                    // info!("Frame count:{frame_count} time_wait:{time_wait_ms}");
                    if frame_count as u128 >= frame_cap_divided {
                        let elapsed = emulator.frame_count_last.elapsed().as_millis();
                        if elapsed >= time_wait_ms {
                            emulator.frame_count.drain(0..frame_cap_divided as usize);
                            emulator.frame_count_last = Instant::now();
                        } else {
                            let can_sleep_for_video_throttle = emulator.apu.read().unwrap()
                                .can_sleep_for_video_throttle();

                            if can_sleep_for_video_throttle {
                                let remaining_ms = (time_wait_ms - elapsed) as u64;
                                if remaining_ms > 1 {
                                    thread::sleep(Duration::from_millis(1));
                                } else {
                                    thread::yield_now();
                                }
                            }
                        }
                    }
                }

            }
            // Save the APU Arc so the audio device survives the reboot.
            carry_apu = Some(emulator.apu.clone());
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
