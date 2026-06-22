use crate::app::{launch_emulator, ToEmulatorMessage, ToUiMessage};
use crate::color::PALETTE_TUPLES;
use crate::config_file::EmulatorConfig;
use crate::constants::{RomInfo, HEIGHT, SCALE_X, SCALE_Y, WIDTH, WINDOW_TITLE};
use crate::emulator::FRAME;
use crate::joypad::{Button as JoyButton, Joypad};
use crate::Args;
use rand::Rng;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;
use tokio::sync::broadcast::error::TryRecvError;
use tokio::sync::broadcast::{channel, Sender};
use vizia::prelude::*;
use vizia::vg;

/// Constants for NES canvas dimensions.
#[allow(dead_code)]
pub const NES_WIDTH: f32 = WIDTH as f32;

/// Constants for NES canvas dimensions.
#[allow(dead_code)]
pub const NES_HEIGHT: f32 = HEIGHT as f32;
pub const SOUND_WAVEFORM_WIDTH: f32 = 300.0;
pub const SOUND_WAVEFORM_HEIGHT: f32 = 110.0;

const PANEL_BACKGROUND: Color = Color::rgb(64, 89, 115);
const VGAP: Units = Pixels(16.0);
const GAP_BETWEEN_TITLE_AND_VIEWS: Units = Pixels(40.0);

/// App model to hold application state.
pub struct AppModel {
    show_grid: Signal<bool>,
    grid_hover_text: Signal<String>,
    selected_rom_index: Signal<Option<usize>>,
    selected_rom_name: Signal<String>,
    selected_rom_mapper: Signal<String>,
    roms: Vec<RomInfo>,
    title_state: Signal<Arc<RwLock<(f32, u16, String)>>>,
    filtered_rom_indices: Signal<Vec<usize>>,
    is_paused: Signal<bool>,
    rom_filter: Signal<String>,
    canvas_width: f32,
    canvas_height: f32,
    sender_to_emulator: Sender<ToEmulatorMessage>,
}

impl AppModel {
    #[allow(dead_code)]
    fn default() -> Self {
        let (sender, _receiver) = channel(10);
        Self::new(vec![], Signal::new(Arc::new(RwLock::new((0.0, 0, "".to_string())))), 0.0, 0.0,
            sender)
    }

    fn new(roms: Vec<RomInfo>, title_state: Signal<Arc<RwLock<(f32, u16, String)>>>,
           canvas_width: f32, canvas_height: f32, sender_to_emulator: Sender<ToEmulatorMessage>)
    -> Self {
        let filtered_rom_indices = Signal::new((0..roms.len()).collect::<Vec<usize>>());
        Self {
            show_grid: Signal::new(false),
            grid_hover_text: Signal::new(String::new()),
            selected_rom_index: Signal::new(None),
            selected_rom_name: Signal::new("".to_string()),
            selected_rom_mapper: Signal::new("".to_string()),
            roms,
            title_state,
            filtered_rom_indices,
            is_paused: Signal::new(false),
            rom_filter: Signal::new(String::new()),
            canvas_width,
            canvas_height,
            sender_to_emulator,
        }
    }
}

impl Model for AppModel {}

fn key_code_to_button(code: Code) -> Option<JoyButton> {
    match code {
        Code::Enter | Code::NumpadEnter => Some(JoyButton::Select),
        Code::Space => Some(JoyButton::Start),
        Code::ArrowUp => Some(JoyButton::Up),
        Code::ArrowDown => Some(JoyButton::Down),
        Code::ArrowLeft => Some(JoyButton::Left),
        Code::ArrowRight => Some(JoyButton::Right),
        Code::KeyA => Some(JoyButton::A),
        Code::KeyB => Some(JoyButton::B),
        _ => None,
    }
}

fn persist_sound_config(
    config: &Arc<RwLock<EmulatorConfig>>,
    sound_config: &SoundConfig,
) {
    if let Ok(mut cfg) = config.write() {
        cfg.sound_all_enabled = sound_config.all_enabled.get();
        cfg.sound_triangle_enabled = sound_config.triangle_enabled.get();
        cfg.sound_pulse1_enabled = sound_config.pulse1_enabled.get();
        cfg.sound_pulse2_enabled = sound_config.pulse2_enabled.get();
        cfg.sound_noise_enabled = sound_config.noise_enabled.get();
        cfg.sound_dmc_enabled = sound_config.dmc_enabled.get();
        let _ = cfg.save();
    }
}

fn build_window_title(frequency: f32, fps: u16, rom_name: &str) -> String {
    format!("{WINDOW_TITLE} - {frequency:.02} Mhz - {fps} FPS - {rom_name}")
}

/// A custom Vizia view that paints the current NES frame.
pub struct EmulatorCanvas {
    joypad: Arc<RwLock<Joypad>>,
}

impl EmulatorCanvas {
    pub fn new(
        cx: &mut Context,
        joypad: Arc<RwLock<Joypad>>,
    ) -> Handle<'_, Self> {
        Self { joypad }.build(cx, |_| {})
    }
}

impl View for EmulatorCanvas {
    fn element(&self) -> Option<&'static str> {
        Some("emulator-canvas")
    }

    fn event(&mut self, _cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| match window_event {
            WindowEvent::KeyDown(code, _) => {
                if let Some(button) = key_code_to_button(*code) {
                    if let Ok(mut joypad) = self.joypad.write() {
                        joypad.set_button_status(button, true);
                    }
                }
            }
            WindowEvent::KeyUp(code, _) => {
                if let Some(button) = key_code_to_button(*code) {
                    if let Ok(mut joypad) = self.joypad.write() {
                        joypad.set_button_status(button, false);
                    }
                }
            }
            _ => {}
        });
    }

    fn draw(&self, _cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = _cx.bounds();
        if bounds.w == 0.0 || bounds.h == 0.0 {
            return;
        }

        let mut paint = vg::Paint::default();
        paint.set_color(Color::rgb(0, 0, 0));
        canvas.draw_rect(vg::Rect::new(bounds.left(), bounds.top(), bounds.right(), bounds.bottom()), &paint);

        let scale_x = SCALE_X;
        let scale_y = SCALE_Y;

        if let Ok(frame_buffer) = FRAME.read() {
            for (index, color) in frame_buffer.iter().enumerate() {
                let (r, g, b) = PALETTE_TUPLES[*color as usize];
                paint.set_color(Color::rgb(r, g, b));

                let x = bounds.left() + (index % WIDTH) as f32 * scale_x;
                let y = bounds.top() + (index / WIDTH) as f32 * scale_y;
                canvas.draw_rect(vg::Rect::new(x, y, x + scale_x, y + scale_y), &paint);
            }
        }

        if _cx.data::<AppModel>().show_grid.get() {
            paint.set_color(Color::rgba(204, 204, 204, 115));
            let grid_x_step = 8.0 * scale_x;
            let grid_y_step = 8.0 * scale_y;
            let start_x = bounds.left();
            let start_y = bounds.top();
            let max_x = start_x + WIDTH as f32 * scale_x;
            let max_y = start_y + NES_HEIGHT * scale_y;

            let mut x = start_x;
            while x <= max_x {
                canvas.draw_rect(vg::Rect::new(x, start_y, x + 1.0, max_y), &paint);
                x += grid_x_step;
            }

            let mut y = start_y;
            while y <= max_y {
                canvas.draw_rect(vg::Rect::new(start_x, y, max_x, y + 1.0), &paint);
                y += grid_y_step;
            }
        }
    }
}

/// A custom Vizia view that paints the current audio waveform.
pub struct SoundWaveformCanvas {
    samples: Arc<RwLock<Vec<f32>>>,
}

impl SoundWaveformCanvas {
    pub fn new(cx: &mut Context, samples: Arc<RwLock<Vec<f32>>>) -> Handle<'_, Self> {
        Self { samples }.build(cx, |_| {})
    }
}

impl View for SoundWaveformCanvas {
    fn element(&self) -> Option<&'static str> {
        Some("sound-waveform-canvas")
    }

    fn draw(&self, _cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = _cx.bounds();
        if bounds.w == 0.0 || bounds.h == 0.0 {
            return;
        }

        let left = bounds.left();
        let top = bounds.top();
        let right = bounds.right();
        let bottom = bounds.bottom();

        let mut paint = vg::Paint::default();
        paint.set_color(Color::rgb(0, 0, 0));
        canvas.draw_rect(vg::Rect::new(left, top, right, bottom), &paint);

        let Ok(samples_guard) = self.samples.read() else {
            return;
        };
        if samples_guard.len() < 2 {
            return;
        }

        let samples = &*samples_guard;
        let margin = 5.0f32;
        let plot_width = (bounds.w - 2.0 * margin).max(1.0);
        let plot_height = (bounds.h - 2.0 * margin).max(1.0);
        let center_y = top + margin + plot_height / 2.0;
        let amplitude = plot_height / 2.0;

        let min_sample = samples.iter().copied().fold(f32::INFINITY, f32::min);
        let max_sample = samples.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        let sample_mid = (min_sample + max_sample) * 0.5;
        let half_range = ((max_sample - min_sample) * 0.5).max(1e-6);

        paint.set_color(Color::rgb(51, 51, 51));
        canvas.draw_rect(
            vg::Rect::new(left + margin, center_y, left + margin + plot_width, center_y + 1.0),
            &paint,
        );

        paint.set_color(Color::rgb(0, 255, 0));

        let max_points = plot_width.max(1.0) as usize;
        let stride = (samples.len() / max_points).max(1);
        let max_index = (samples.len() - 1) as f32;
        let mut previous: Option<(f32, f32)> = None;

        for index in (0..samples.len()).step_by(stride) {
            let x = left + margin + (index as f32 / max_index) * plot_width;
            let centered = ((samples[index] - sample_mid) / half_range).clamp(-1.0, 1.0);
            let y = center_y - centered * amplitude;

            canvas.draw_rect(vg::Rect::new(x, y, x + 1.5, y + 1.5), &paint);

            if let Some((prev_x, prev_y)) = previous {
                let min_y = prev_y.min(y);
                let max_y = prev_y.max(y);
                canvas.draw_rect(vg::Rect::new(prev_x, min_y, prev_x + 1.0, max_y + 1.0), &paint);
            }
            previous = Some((x, y));
        }
    }
}

fn checkbox(cx: &mut Context, label: String,
    value: Signal<bool>, on_toggle: impl Fn(&mut EventContext) + 'static)
-> Handle<'_, HStack>
{
    HStack::new(cx, |cx| {
        Checkbox::new(cx, value).on_toggle(on_toggle)
            .background_color(Color::white());
        Label::new(cx, label);
    })
    .class("nes-checkbox")
}

// // pub fn create_vizia_app(args: Args, roms: Vec<RomInfo>, rom_info: RomInfo, config: EmulatorConfig) {
// pub fn _main2() {
//     let _ = Application::new(move |cx| {
//         cx.add_stylesheet(include_style!("nes.css")).expect("nes.css should exist");
//         AppModel::default().build(cx);
//
//         HStack::new(cx, |cx| {
//             let value = Signal::new(true);
//             checkbox(cx, "Sound".to_string(), Signal::new(true), move |_| {
//                 value.set(!value.get());
//             });
//         })
//         .gap(Pixels(10.0))
//         .width(Auto)
//         .height(Auto)
//         ;
//     })
//     .run();
// }

/// Creates the Vizia app and launches the NES emulator.
#[allow(dead_code)]
pub fn create_vizia_app(args: Args, roms: Vec<RomInfo>, rom_info: RomInfo, config: EmulatorConfig) {
    let (sender_to_ui, _receiver_from_ui) = channel(10);
    let (sender_to_emulator, receiver_from_emulator) = channel(10);
    let (_shared_state, joypad) = launch_emulator(
        args.clone(),
        rom_info.clone(),
        sender_to_ui.clone(),
        receiver_from_emulator,
    );
    let waveform_samples = Arc::new(RwLock::new(Vec::<f32>::new()));
    let waveform_for_ui = waveform_samples.clone();
    let title_state = Arc::new(RwLock::new((0.0f32, 0u16, rom_info.name())));
    let title_state2 = title_state.clone();
    let title_state3 = title_state.clone();
    let mut ui_receiver = sender_to_ui.subscribe();
    thread::spawn(move || {
        loop {
            match ui_receiver.try_recv() {
                Ok(ToUiMessage::SoundSamples(samples)) => {
                    if let Ok(mut dst) = waveform_for_ui.write() {
                        *dst = samples;
                    }
                }
                Ok(ToUiMessage::Update(frequency, fps)) => {
                    if let Ok(mut title) = title_state2.write() {
                        title.0 = frequency;
                        title.1 = fps;
                    };
                }
                Err(TryRecvError::Empty) => thread::sleep(Duration::from_millis(8)),
                Err(TryRecvError::Lagged(_)) => {}
                Err(TryRecvError::Closed) => break,
            }
        }
    });


    let canvas_width = NES_WIDTH * SCALE_X;
    let canvas_height = NES_HEIGHT * SCALE_Y;
    let window_width = 1260;
    let window_height = (canvas_height + 32.0) as u32;

    let _ = Application::new(move |cx| {
        cx.add_stylesheet(include_style!("nes.css")).expect("nes.css should exist");
        AppModel::new(roms.clone(), Signal::new(title_state), canvas_width, canvas_height,
            sender_to_emulator
        ).build(cx);

        let index = cx.data::<AppModel>().selected_rom_index;
        Binding::new(cx, index, move |cx| {
            let roms = &cx.data::<AppModel>().roms;
            let rom = index.get()
                .and_then(|index| roms.get(index).cloned())
                .unwrap_or_else(|| RomInfo::default());
            cx.data::<AppModel>().selected_rom_name.set(rom.name().clone());
            cx.data::<AppModel>().selected_rom_mapper.set(rom.mapper_number().to_string());
            println!("Index changed to: {} rom:{rom:#?}", index.get().unwrap_or(0));
        });
        let timer = cx.add_timer(Duration::from_millis(16), None, move |cx, action| {
            if matches!(action, TimerAction::Start | TimerAction::Tick(_)) {
                if let Ok(title) = title_state3.read() {
                    cx.emit(WindowEvent::SetTitle(build_window_title(title.0, title.1, &title.2)));
                }
                cx.needs_redraw();
            }
        });
        cx.start_timer(timer);
        let ui = build_ui(cx, config,
                          joypad, waveform_samples);

        ui
            .width(Stretch(1.0))
            .height(Stretch(1.0))
            .gap(Pixels(24.0))
        .padding(Pixels(10.0));
    })
    .title(WINDOW_TITLE)
    .inner_size((window_width, window_height))
    .run();
}

fn build_sound_checkbox_all(cx: &'_ mut Context, emulator_config: Arc<RwLock<EmulatorConfig>>,
    sound_config: SoundConfig) -> Handle<'_, HStack>
{
    HStack::new(cx, |cx| {
        let all_enabled = sound_config.all_enabled.clone();
        let triangle_enabled = sound_config.triangle_enabled.clone();
        let pulse1_enabled = sound_config.pulse1_enabled.clone();
        let pulse2_enabled = sound_config.pulse2_enabled.clone();
        let noise_enabled = sound_config.noise_enabled.clone();
        let dmc_enabled = sound_config.dmc_enabled.clone();

        let sound_config = sound_config.clone();
        let sender = cx.data::<AppModel>().sender_to_emulator.clone();
        let cfg = emulator_config.clone();
        checkbox(cx, "Sound".to_string(), all_enabled, move |_| {
            let enabled = !all_enabled.get();
            all_enabled.set(enabled);
            triangle_enabled.set(enabled);
            pulse1_enabled.set(enabled);
            pulse2_enabled.set(enabled);
            noise_enabled.set(enabled);
            dmc_enabled.set(enabled);
            let _ = sender.send(ToEmulatorMessage::SoundTriangle(enabled));
            let _ = sender.send(ToEmulatorMessage::SoundPulse1(enabled));
            let _ = sender.send(ToEmulatorMessage::SoundPulse2(enabled));
            let _ = sender.send(ToEmulatorMessage::SoundNoise(enabled));
            let _ = sender.send(ToEmulatorMessage::SoundDmc(enabled));
            persist_sound_config(&cfg, &sound_config);
        });
    })
    .class("nes-checkbox")
    .background_color(Color::rgb(80, 80, 80))
    // Space between the checkbox and "Sound"
    .gap(Pixels(8.0))
}

fn build_sound_checkbox(cx: &mut Context, emulator_config: &Arc<RwLock<EmulatorConfig>>,
    sound_config: &mut SoundConfig,
    is_enabled: Signal<bool>, name: String)
{
    let sender = cx.data::<AppModel>().sender_to_emulator.clone();
    let cfg = emulator_config.clone();
    let sound_config = sound_config.clone();
    checkbox(cx, name, is_enabled, move |_| {
        let enabled = !is_enabled.get();
        is_enabled.set(enabled);
        sound_config.all_enabled.set(
            sound_config.triangle_enabled.get()
                && sound_config.pulse1_enabled.get()
                && sound_config.pulse2_enabled.get()
                && sound_config.noise_enabled.get()
                && sound_config.dmc_enabled.get(),
        );
        let _ = sender.send(ToEmulatorMessage::SoundTriangle(enabled));
        persist_sound_config(&cfg, &sound_config);
    });
}

#[derive(Copy, Clone)]
struct SoundConfig {
    all_enabled: Signal<bool>,
    triangle_enabled: Signal<bool>,
    pulse1_enabled: Signal<bool>,
    pulse2_enabled: Signal<bool>,
    noise_enabled: Signal<bool>,
    dmc_enabled: Signal<bool>,
}

impl SoundConfig {
}

fn build_panel_sound(cx: &mut Context,config: EmulatorConfig,
    waveform_samples: Arc<RwLock<Vec<f32>>>)
{
    let sound_config = SoundConfig {
        all_enabled: Signal::new(config.sound_all_enabled),
        triangle_enabled: Signal::new(config.sound_triangle_enabled),
        pulse1_enabled: Signal::new(config.sound_pulse1_enabled),
        pulse2_enabled: Signal::new(config.sound_pulse2_enabled),
        noise_enabled: Signal::new(config.sound_noise_enabled),
        dmc_enabled: Signal::new(config.sound_dmc_enabled),
    };
    let emulator_config = Arc::new(RwLock::new(config));

    let sc2 = sound_config.clone();
    VStack::new(cx, |cx| {
        Frame::with_title(cx,
            |cx| {
                build_sound_checkbox_all(cx, Arc::clone(&emulator_config), sc2)
            },
            |cx| {
            HStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        build_sound_checkbox(cx, &emulator_config,
                            &mut sound_config.clone(), sound_config.triangle_enabled,
                            "Triangle".into());
                        build_sound_checkbox(cx, &emulator_config,
                            &mut sound_config.clone(), sound_config.pulse1_enabled,
                            "Pulse 1".into());
                        build_sound_checkbox(cx, &emulator_config,
                            &mut sound_config.clone(),
                            sound_config.dmc_enabled,
                            "DMC".into());
                    })
                    .vertical_gap(VGAP)
                    .width(Percentage(50.0));

                    VStack::new(cx, |cx| {
                        build_sound_checkbox(cx, &emulator_config,
                            &mut sound_config.clone(),
                            sound_config.noise_enabled,
                            "Noise".into());
                        build_sound_checkbox(cx, &emulator_config,
                            &mut sound_config.clone(),
                            sound_config.pulse2_enabled,
                            "Pulse 2".into());
                    })
                    .vertical_gap(VGAP)
                    .width(Percentage(50.0));
                })
                ;
            })
            .gap(Pixels(20.0));

            SoundWaveformCanvas::new(cx, waveform_samples.clone())
                .width(Pixels(SOUND_WAVEFORM_WIDTH))
                .height(Pixels(SOUND_WAVEFORM_HEIGHT));
        })
        .title_position(FrameTitlePosition::TopCenter)
        .background_color(PANEL_BACKGROUND)
        .padding_top(Pixels(30.0))
        .gap(Pixels(20.0))
        .width(Stretch(1.0))
        .class("sound-panel")
        ;
    })
    .gap(Pixels(36.0));
}

/// Main entry point that builds the whole GUI
fn build_ui(cx: &mut Context, config: EmulatorConfig, joypad: Arc<RwLock<Joypad>>,
        waveform_samples: Arc<RwLock<Vec<f32>>>)
-> Handle<'_, HStack>
{
    HStack::new(cx, |cx| {
        build_panel_emulator_canvas(cx, joypad);
        build_panel_rom(cx);
        VStack::new(cx, |cx| {
            build_panel_controls(cx);
            build_panel_sound(cx, config, waveform_samples);
        })
        .gap(Pixels(24.0))
        ;
    })
    .height(Stretch(1.0))
    .gap(Pixels(24.0))
    .background_color(Color::rgb(80, 80, 80))
    .padding(Pixels(10.0))
}

fn build_panel_rom(cx: &mut Context) {
    VStack::new(cx, |cx| {
        HStack::new(cx, |cx| {
            Label::new(cx, cx.data::<AppModel>().selected_rom_name).width(Stretch(1.0)).color(Color::white());
            Label::new(cx, cx.data::<AppModel>().selected_rom_mapper).color(Color::yellow());
        })
        .class("rom-name");
        
        let am = &cx.data::<AppModel>();
        let all_roms = am.roms.clone();
        let filtered_rom_indices = am.filtered_rom_indices;
        let rom_filter = am.rom_filter;
        VStack::new(cx, |cx| {
            Textbox::new(cx, rom_filter)
                .placeholder("Filter ROMs...")
                .on_edit({
                    move |_cx, text| {
                        rom_filter.set(text.clone());
                        let needle = text.to_lowercase();
                        if needle.is_empty() {
                            filtered_rom_indices
                                .set((0..all_roms.len()).collect::<Vec<usize>>());
                        } else {
                            filtered_rom_indices.set(
                                all_roms
                                    .iter()
                                    .enumerate()
                                    .filter(|(_, rom)| rom.name().to_lowercase().contains(&needle))
                                    .map(|(index, _)| index)
                                    .collect::<Vec<usize>>(),
                            );
                        }
                    }
                })
                .width(Stretch(1.0));

            let am = &cx.data::<AppModel>();
            let filtered_rom_indices = am.filtered_rom_indices;
            let all_roms = am.roms.clone();
            let selected_rom_index = am.selected_rom_index;
            let rom_name = am.selected_rom_name;
            let mapper_label = am.selected_rom_mapper;
            List::new(cx, filtered_rom_indices, {
                move |cx, _index, item| {
                    let row_hovered = Signal::new(false);
                    let row_background = row_hovered.map(|hovered| {
                        if *hovered {
                            Color::rgba(255, 255, 255, 28)
                        } else {
                            Color::rgba(0, 0, 0, 0)
                        }
                    });
                    let row_text = row_hovered.map(|hovered| {
                        if *hovered {
                            Color::rgb(255, 255, 210)
                        } else {
                            Color::white()
                        }
                    });
                    let rom_label = item.map({
                        let all_roms = all_roms.clone();
                        move |rom_index| all_roms[*rom_index].name()
                    });
                    Button::new(cx, move |cx| {
                        Label::new(cx, rom_label)
                            .width(Stretch(1.0))
                            .text_align(TextAlign::Left)
                            .color(row_text)
                    })
                        .on_press({
                            let all_roms = all_roms.clone();
                            let selected_rom_index = selected_rom_index;
                            // let rom_name = ui_data.rom_name;
                            // let mapper_label = ui_data.mapper_label;
                            move |_| {
                                let global_index = item.get();
                                selected_rom_index.set(Some(global_index));
                                if let Some(rom) = all_roms.get(global_index) {
                                    rom_name.set(rom.name());
                                    mapper_label.set(format!("{}", rom.mapper_number()));
                                }
                            }
                        })
                        .on_hover({
                            let row_hovered = row_hovered;
                            move |_| {
                                row_hovered.set(true);
                            }
                        })
                        .on_hover_out({
                            let row_hovered = row_hovered;
                            move |_| {
                                row_hovered.set(false);
                            }
                        })
                        .width(Stretch(1.0))
                        .height(Pixels(24.0))
                        .padding(Pixels(0.0))
                        .border_width(Pixels(0.0))
                        .corner_radius(Pixels(0.0))
                        .background_color(row_background);
                }
            })
            .width(Stretch(1.0))
            .height(Stretch(1.0))
            .show_horizontal_scrollbar(false)
            .show_vertical_scrollbar(true);
        })
        .class("rom-list");
    })
    .class("rom-panel");
}


fn boot_rom(cx: &EventContext) {
    let am = cx.data::<AppModel>();
    let rom = am.selected_rom_index.get()
        .and_then(|index| cx.data::<AppModel>().roms.get(index).cloned())
        .unwrap_or_else(|| RomInfo::default());

    let _ = am.sender_to_emulator.send(ToEmulatorMessage::Reboot(rom.clone()));
    if let Ok(mut t) = am.title_state.get().write() {
        t.2 = rom.name();
    }
}

fn boot_random_rom(cx: &mut EventContext) {
    let len = cx.data::<AppModel>().roms.len();
    let index = if len > 0 {
        let index = rand::thread_rng().gen_range(0..len);
        println!("Random index:{index} len:{len}");
        Some(index)
    } else {
        println!("len is 0, not booting a ROM");
        None
    };
    cx.data::<AppModel>().selected_rom_index.set(index);
    boot_rom(cx);
}

fn build_panel_controls(cx: &mut Context) {
    Frame::with_title(cx,
        |cx| {
            HStack::new(cx, |cx| {
                Label::new(cx, "Controls").background_color(Color::transparent());
            }).background_color(Color::rgb(80, 80, 80))
        },
        |cx| {
            VStack::new(cx, |cx| {
                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Button::new(cx, |cx| Label::new(cx, "Reboot")).on_press({
                            move |cx| {
                                boot_rom(cx);
                            }
                        })
                        .width(Pixels(100.0));

                        Button::new(cx, |cx| Label::new(cx, "Random")).on_press({
                            move |cx| {
                                boot_random_rom(cx);
                            }
                        })
                        .width(Pixels(100.0));
                    })
                    .width(Percentage(50.0))
                    // Vertical gap between the buttons
                    .gap(VGAP)
                    ;

                    VStack::new(cx, |cx| {
                        let is_paused = cx.data::<AppModel>().is_paused;
                        let sender_to_emulator = cx.data::<AppModel>().sender_to_emulator.clone();
                        checkbox(cx, "Pause".to_string(), is_paused,
                                 move |_| {
                                     is_paused.set(!is_paused.get());
                                     let _ = sender_to_emulator.send(ToEmulatorMessage::Pause(is_paused.get()));
                                 });
                        let show_grid = cx.data::<AppModel>().show_grid;
                        let grid_hover_text = cx.data::<AppModel>().grid_hover_text;
                        checkbox(cx, "Grid".to_string(), show_grid, move |_| {
                            show_grid.set(!show_grid.get());
                            if !show_grid.get() {
                                grid_hover_text.set(String::new());
                            }
                        });
                  })
                  .gap(VGAP);
                })
                ;
            })
            .corner_radius(Pixels(8.0))
            ;
        })
        .class("controls-panel")
        .background_color(PANEL_BACKGROUND)
        // .border_width(Pixels(1.0))
        // .border_color(Color::white())
        .space(Pixels(50.0))
        .title_position(FrameTitlePosition::TopCenter)
        .width(Stretch(1.0))
    ;
}

fn build_panel_emulator_canvas(cx: &mut Context, joypad: Arc<RwLock<Joypad>>) {
    println!("Canvas width: {}, height: {}", cx.data::<AppModel>().canvas_width, cx.data::<AppModel>().canvas_height);
    let show_grid = cx.data::<AppModel>().show_grid;
    let grid_hover_text = cx.data::<AppModel>().grid_hover_text;
    let width = cx.data::<AppModel>().canvas_width;
    let height = cx.data::<AppModel>().canvas_height;
    EmulatorCanvas::new(cx, joypad.clone())
        .class("canvas")
        .width(Pixels(width))
        .height(Pixels(height))
        .hoverable(true)
        .navigable(true)
        .on_mouse_down(|cx, _| {
            cx.focus();
        })
        .on_mouse_move({
            move |cx, x, y| {
                if !show_grid.get() {
                    grid_hover_text.set(String::new());
                    return;
                }
                let bounds = cx.bounds();
                let local_x = x - bounds.x;
                let local_y = y - bounds.y;
                let max_x = WIDTH as f32 * SCALE_X;
                let max_y = NES_HEIGHT * SCALE_Y;
                if local_x >= 0.0 && local_y >= 0.0 && local_x < max_x && local_y < max_y {
                    let tile_x = (local_x / (8.0 * SCALE_X)).floor() as u16;
                    let tile_y = (local_y / (8.0 * SCALE_Y)).floor() as u16;
                    grid_hover_text.set(format!("Tile ({tile_x}, {tile_y})"));
                } else {
                    grid_hover_text.set(String::new());
                }
            }
        })
        .on_hover_out({
            move |_| {
                grid_hover_text.set(String::new());
            }
        });

    let grid_hover_text = cx.data::<AppModel>().grid_hover_text;
    let show_grid = cx.data::<AppModel>().show_grid;
    Binding::new(cx, grid_hover_text, move |cx| {
        let text = grid_hover_text.get();
        if !text.is_empty() && show_grid.get() {
            Label::new(cx, text)
                .position_type(PositionType::Absolute)
                .left(Pixels(8.0))
                .top(Pixels(8.0))
                .padding(Pixels(10.0))
                .background_color(Color::rgb(20, 20, 20))
                .color(Color::white())
                .border_width(Pixels(1.0))
                .border_color(Color::rgb(180, 180, 180))
                .corner_radius(Pixels(4.0))
                .hoverable(false);
        }
    });
}
