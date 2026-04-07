use crate::constants::RomInfo;
use crate::listview::Message;
use iced::widget::{button, container, text};
use iced::{Background, Border, Color, Element, Length};
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelIterator;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use tracing::info;
use walkdir::WalkDir;

pub fn create_rom_item(is_selected: bool, item: RomInfo) -> Element<'static, Message> {
    // let is_selected = selected_rom_id == item.id as usize;

    // Get the name as a String and then create a reference to it
    let name = item.name();

    let content = iced::widget::column![
            text(name)
                .size(12)
                .color(if is_selected {
                    Color::from_rgb(1.0, 1.0, 1.0) // Brighter text for selected items
                } else {
                    Color::from_rgb(0.9, 0.9, 1.0)
                }),
            // text(&item.description)
            //     .size(12)
            //     .color(if is_selected {
            //         Color::from_rgb(0.7, 1.0, 0.7) // Greenish description for selected items
            //     } else {
            //         Color::from_rgb(0.6, 0.8, 1.0)
            //     })
        ]
        .spacing(2)
        .padding(2);

    container(
        button(content)
            .on_press(Message::ItemClicked(item.id as usize))
            .width(Length::Fixed(300.0))
            .style(move |_theme, status| {
                // Define colors inside the closure so they're owned by the closure
                let accent_color = Color::from_rgb(0.0, 0.8, 1.0);
                let highlight_color = Color::from_rgb(0.0, 1.0, 0.5); // Neon green for selected items

                button::Style {
                    background: Some(Background::Color(match (status, is_selected) {
                        (_, true) => Color::from_rgb(0.15, 0.25, 0.2), // Darker green-blue for selected
                        (button::Status::Hovered, _) => Color::from_rgb(0.15, 0.15, 0.3),
                        (button::Status::Pressed, _) => Color::from_rgb(0.2, 0.2, 0.4),
                        _ => Color::from_rgb(0.1, 0.1, 0.2),
                    })),
                    border: Border {
                        color: match (status, is_selected) {
                            (_, true) => highlight_color,
                            (button::Status::Hovered, _) => accent_color,
                            _ => Color::from_rgb(0.3, 0.3, 0.5),
                        },
                        width: if is_selected { 2.0 } else { 1.0 },
                        radius: 4.0.into(),
                    },
                    shadow: if is_selected {
                        iced::Shadow {
                            color: Color::from_rgba(0.0, 1.0, 0.5, 0.3), // Neon glow
                            offset: iced::Vector::new(0.0, 0.0),
                            blur_radius: 8.0,
                        }
                    } else {
                        iced::Shadow::default()
                    },
                    ..Default::default()
                }
            })
    )
        .into()
}


pub fn _display_roms_by_mapper(path: &str) {
    let entries: Vec<_> = WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();

    let mut mapper_map: HashMap<u8, u16> = HashMap::new();

    entries
        // .into_par_iter()
        .iter()
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension() == Some(std::ffi::OsStr::new("nes")))
        .for_each(|e| {
            let path = e.path().to_str().unwrap();
            let n = mapper_number(path);
            let count = mapper_map.get(&n).unwrap_or(&0);
            mapper_map.insert(n, count + 1);
        });

    let mut list: Vec<(u8, u16)> = Vec::new();
    mapper_map.iter().for_each(|e| {
        list.push((*e.0, *e.1))
    });
    list.sort_by(|a, b| b.1.cmp(&a.1));
    list.iter().for_each(|e| {
        println!("Mapper {}: {} roms", e.0, e.1);
    });
}

/// Return all the .nes files using the provided mappers
pub fn find_roms_with_mappers(path: &str, mappers: Vec<u8>) -> Vec<RomInfo> {
    let entries: Vec<_> = WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();

    let mut result: Vec<RomInfo> = entries
        .into_par_iter()
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension() == Some(std::ffi::OsStr::new("nes")))
        .filter(|e| mappers.contains(&mapper_number(e.path().to_str().unwrap())))
        .map(|e| RomInfo::n(0, e.path().to_str().unwrap())).collect();

    let mut id = 100;
    for ri in &mut result {
        ri.id = id;
        id += 1;
    }

    info!("Found {} ROMs for mappers [{}]", result.len(),
        mappers.iter().map(|m| format!("{}", m)).collect::<Vec<String>>().join(", "));
    result
}

/// Extract the mapper number from the .nes file
pub fn mapper_number(path: &str) -> u8 {
    match check_first_bytes(path, 8) {
        Ok(bytes) => {
            if bytes.len() >= 8 {
                (bytes[6] & 0xf0) >> 4 | bytes[7] & 0xf0
            } else {
                // Default to mapper 0 if file is too small
                0
            }
        }
        Err(_) => {
            // Default to mapper 0 if file cannot be read
            0
        }
    }
}

/// Return the furst `num_bytes` of the file
fn check_first_bytes(path: &str, num_bytes: usize) -> std::io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut buffer = vec![0u8; num_bytes];
    let bytes_read = reader.read(&mut buffer)?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}

