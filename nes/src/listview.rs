use iced::{
    widget::{button, column, container, row, scrollable, text},
    Alignment, Background, Border, Color, Element, Length, Padding, Size, Task, Theme, border,
};

#[derive(Debug, Clone)]
pub enum Message {
    ItemClicked(usize),
    SetTheme(ListTheme),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ListTheme {
    Default,
    Dark,
    Minimal,
    Cards,
    Gaming,
    Professional,
    Colorful,
    Terminal,
}

#[derive(Debug, Clone)]
pub struct ListItem {
    id: usize,
    name: String,
    description: String,
}

pub struct ThemedListView {
    items: Vec<ListItem>,
    current_theme: ListTheme,
    selected_item_id: Option<usize>,
}

impl ThemedListView {
    pub fn new() -> (Self, Task<Message>) {
        let items = vec![
            ListItem {
                id: 1,
                name: "System Process".to_string(),
                description: "Core system process running in background".to_string(),
            },
            ListItem {
                id: 2,
                name: "User Application".to_string(),
                description: "Application started by user with normal priority".to_string(),
            },
            ListItem {
                id: 3,
                name: "Network Service".to_string(),
                description: "Service handling network communications".to_string(),
            },
            ListItem {
                id: 4,
                name: "Background Task".to_string(),
                description: "Scheduled task performing maintenance operations".to_string(),
            },
            ListItem {
                id: 5,
                name: "Media Player".to_string(),
                description: "Audio/video playback service with high priority".to_string(),
            },
        ];

        (
            ThemedListView {
                items,
                current_theme: ListTheme::Default,
                selected_item_id: None,
            },
            Task::none(),
        )
    }

    pub fn title(&self) -> String {
        "Themed ListView Examples".to_string()
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ItemClicked(id) => {
                println!("Item {} clicked!", id);
                self.selected_item_id = Some(id);
            }
            Message::SetTheme(theme) => {
                self.current_theme = theme;
            }
        }
        Task::none()
    }

    pub fn view(&self) -> Element<Message> {
        let theme_selector = row![
            button("Default").on_press(Message::SetTheme(ListTheme::Default)),
            button("Dark").on_press(Message::SetTheme(ListTheme::Dark)),
            button("Minimal").on_press(Message::SetTheme(ListTheme::Minimal)),
            button("Cards").on_press(Message::SetTheme(ListTheme::Cards)),
            button("Gaming").on_press(Message::SetTheme(ListTheme::Gaming)),
            button("Pro").on_press(Message::SetTheme(ListTheme::Professional)),
            button("Colorful").on_press(Message::SetTheme(ListTheme::Colorful)),
            button("Terminal").on_press(Message::SetTheme(ListTheme::Terminal)),
        ]
            .spacing(5);

        let title = text(format!("ListView Theme: {:?}", self.current_theme))
            .size(20);

        let items_list = self.items.iter().fold(
            column![].spacing(self.get_item_spacing()),
            |col, item| col.push(self.create_themed_item(item))
        );

        let list_container = scrollable(items_list)
            .width(Length::Fill)
            .height(Length::Fixed(400.0));

        let content = column![
            title,
            theme_selector,
            list_container
        ]
            .spacing(15)
            .padding(20);

        self.create_themed_container(content.into())
    }

    fn get_item_spacing(&self) -> u16 {
        match self.current_theme {
            ListTheme::Minimal => 2,
            ListTheme::Cards => 10,
            ListTheme::Gaming => 8,
            _ => 5,
        }
    }

    fn create_themed_container<'a>(&self, content: Element<'a, Message>) -> Element<'a, Message> {
        match self.current_theme {
            ListTheme::Dark => {
                container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color::from_rgb(0.1, 0.1, 0.1))),
                        ..Default::default()
                    })
                    .into()
            }
            ListTheme::Gaming => {
                container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color::from_rgb(0.05, 0.05, 0.15))),
                        ..Default::default()
                    })
                    .into()
            }
            ListTheme::Terminal => {
                container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_theme| container::Style {
                        background: Some(Background::Color(Color::BLACK)),
                        ..Default::default()
                    })
                    .into()
            }
            _ => {
                container(content)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .into()
            }
        }
    }

    fn create_themed_item<'a>(&'a self, item: &'a ListItem) -> Element<'a, Message> {
        match self.current_theme {
            ListTheme::Default => self.create_default_item(item),
            ListTheme::Dark => self.create_dark_item(item),
            ListTheme::Minimal => self.create_minimal_item(item),
            ListTheme::Cards => self.create_card_item(item),
            ListTheme::Gaming => self.create_gaming_item(item),
            ListTheme::Professional => self.create_professional_item(item),
            ListTheme::Colorful => self.create_colorful_item(item),
            ListTheme::Terminal => self.create_terminal_item(item),
        }
    }

    fn create_default_item<'a>(&'a self, item: &'a ListItem) -> Element<'a, Message> {
        let is_selected = self.selected_item_id == Some(item.id);

        let content = column![
            text(&item.name).size(16),
            text(&item.description).size(12)
        ]
            .spacing(3);

        button(content)
            .on_press(Message::ItemClicked(item.id))
            .width(Length::Fill)
            .style(move |_theme, status| button::Style {
                background: Some(Background::Color(match (status, is_selected) {
                    (_, true) => Color::from_rgb(0.9, 0.9, 1.0),
                    (button::Status::Hovered, _) => Color::from_rgb(0.95, 0.95, 0.95),
                    _ => Color::WHITE,
                })),
                border: if is_selected {
                    Border {
                        color: Color::from_rgb(0.4, 0.4, 0.8),
                        width: 2.0,
                        radius: 4.0.into(),
                    }
                } else {
                    Border::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn create_dark_item<'a>(&'a self, item: &'a ListItem) -> Element<'a, Message> {
        let is_selected = self.selected_item_id == Some(item.id);

        let content = column![
            text(&item.name)
                .size(16)
                .color(Color::from_rgb(0.9, 0.9, 0.9)),
            text(&item.description)
                .size(12)
                .color(Color::from_rgb(0.7, 0.7, 0.7))
        ]
            .spacing(5);

        container(
            button(content)
                .on_press(Message::ItemClicked(item.id))
                .width(Length::Fill)
                .style(move |_theme, status| button::Style {
                    background: Some(Background::Color(match (status, is_selected) {
                        (_, true) => Color::from_rgb(0.35, 0.35, 0.5),
                        (button::Status::Hovered, _) => Color::from_rgb(0.3, 0.3, 0.3),
                        (button::Status::Pressed, _) => Color::from_rgb(0.4, 0.4, 0.4),
                        _ => Color::from_rgb(0.2, 0.2, 0.2),
                    })),
                    border: if is_selected {
                        Border {
                            color: Color::from_rgb(0.6, 0.6, 1.0),
                            width: 2.0,
                            radius: 5.0.into(),
                        }
                    } else {
                        border::rounded(5)
                    },
                    ..Default::default()
                })
        )
            .padding(2)
            .into()
    }

    fn create_minimal_item<'a>(&'a self, item: &'a ListItem) -> Element<'a, Message> {
        let is_selected = self.selected_item_id == Some(item.id);

        let content = row![
            text(&item.name)
                .size(14)
                .width(Length::FillPortion(2)),
            text(&item.description)
                .size(12)
                .color(Color::from_rgb(0.5, 0.5, 0.5))
                .width(Length::FillPortion(3))
        ]
            .align_y(Alignment::Center);

        button(content)
            .on_press(Message::ItemClicked(item.id))
            .width(Length::Fill)
            .style(move |_theme, status| button::Style {
                background: match (status, is_selected) {
                    (_, true) => Some(Background::Color(Color::from_rgb(0.9, 0.95, 1.0))),
                    (button::Status::Hovered, _) => Some(Background::Color(Color::from_rgb(0.95, 0.95, 0.95))),
                    _ => None,
                },
                border: if is_selected {
                    Border {
                        color: Color::from_rgb(0.7, 0.8, 0.9),
                        width: 1.0,
                        radius: 0.0.into(),
                    }
                } else {
                    Border::default()
                },
                ..Default::default()
            })
            .into()
    }

    fn create_card_item<'a>(&'a self, item: &'a ListItem) -> Element<'a, Message> {
        let is_selected = self.selected_item_id == Some(item.id);

        let content = column![
            text(&item.name)
                .size(16)
                .color(Color::from_rgb(0.2, 0.2, 0.2)),
            text(&item.description)
                .size(13)
                .color(Color::from_rgb(0.4, 0.4, 0.4))
        ]
            .spacing(8)
            .padding(15);

        container(
            button(content)
                .on_press(Message::ItemClicked(item.id))
                .width(Length::Fill)
                .style(move |_theme, status| button::Style {
                    background: Some(Background::Color(match (status, is_selected) {
                        (_, true) => Color::from_rgb(0.95, 0.97, 1.0),
                        (button::Status::Hovered, _) => Color::from_rgb(0.97, 0.97, 1.0),
                        (button::Status::Pressed, _) => Color::from_rgb(0.95, 0.95, 1.0),
                        _ => Color::WHITE,
                    })),
                    border: Border {
                        color: if is_selected {
                            Color::from_rgb(0.4, 0.6, 0.9)
                        } else {
                            Color::from_rgb(0.9, 0.9, 0.9)
                        },
                        width: if is_selected { 2.0 } else { 1.0 },
                        radius: 8.0.into(),
                    },
                    shadow: iced::Shadow {
                        color: if is_selected {
                            Color::from_rgba(0.0, 0.0, 0.8, 0.15)
                        } else {
                            Color::from_rgba(0.0, 0.0, 0.0, 0.1)
                        },
                        offset: iced::Vector::new(0.0, if is_selected { 3.0 } else { 2.0 }),
                        blur_radius: if is_selected { 6.0 } else { 4.0 },
                    },
                    ..Default::default()
                })
        )
            .into()
    }

    fn create_gaming_item<'a>(&'a self, item: &'a ListItem) -> Element<'a, Message> {
        let is_selected = self.selected_item_id == Some(item.id);
        let accent_color = Color::from_rgb(0.0, 0.8, 1.0);
        let highlight_color = Color::from_rgb(0.0, 1.0, 0.5); // Neon green for selected items

        let content = column![
            text(&item.name)
                .size(16)
                .color(if is_selected {
                    Color::from_rgb(1.0, 1.0, 1.0) // Brighter text for selected items
                } else {
                    Color::from_rgb(0.9, 0.9, 1.0)
                }),
            text(&item.description)
                .size(12)
                .color(if is_selected {
                    Color::from_rgb(0.7, 1.0, 0.7) // Greenish description for selected items
                } else {
                    Color::from_rgb(0.6, 0.8, 1.0)
                })
        ]
            .spacing(5)
            .padding(12);

        container(
            button(content)
                .on_press(Message::ItemClicked(item.id))
                .width(Length::Fill)
                .style(move |_theme, status| button::Style {
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
                })
        )
            .into()
    }

    fn create_professional_item<'a>(&'a self, item: &'a ListItem) -> Element<'a, Message> {
        let is_selected = self.selected_item_id == Some(item.id);

        // Professional blue color for the dot indicator
        let dot_color = if is_selected {
            Color::from_rgb(0.1, 0.4, 0.8) // Darker, more saturated blue for selected
        } else {
            Color::from_rgb(0.2, 0.6, 0.8)
        };

        let content = row![
            container(text(if is_selected { "►" } else { "●" })) // Change indicator for selected item
                .width(Length::Fixed(20.0))
                .style(move |_theme| container::Style {
                    text_color: Some(dot_color),
                    ..Default::default()
                }),
            column![
                text(&item.name)
                    .size(15)
                    .color(if is_selected {
                        Color::from_rgb(0.0, 0.0, 0.0) // Darker text for selected
                    } else {
                        Color::from_rgb(0.1, 0.1, 0.1)
                    }),
                text(&item.description)
                    .size(12)
                    .color(if is_selected {
                        Color::from_rgb(0.2, 0.4, 0.6) // Blueish description for selected
                    } else {
                        Color::from_rgb(0.4, 0.4, 0.4)
                    })
            ]
            .spacing(3)
        ]
            .align_y(Alignment::Start)
            .spacing(10)
            .padding(Padding::new(12.0));

        button(content)
            .on_press(Message::ItemClicked(item.id))
            .width(Length::Fill)
            .style(move |_theme, status| button::Style {
                background: match (status, is_selected) {
                    (_, true) => Some(Background::Color(Color::from_rgb(0.94, 0.97, 1.0))),
                    (button::Status::Hovered, _) => Some(Background::Color(Color::from_rgb(0.96, 0.98, 1.0))),
                    _ => None,
                },
                border: Border {
                    color: if is_selected {
                        Color::from_rgb(0.7, 0.8, 0.9) // Light blue border for selected
                    } else {
                        Color::from_rgb(0.9, 0.9, 0.9)
                    },
                    width: match (status, is_selected) {
                        (_, true) => 1.0,
                        (button::Status::Hovered, _) => 0.0,
                        _ => 0.5,
                    },
                    radius: 2.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn create_colorful_item<'a>(&'a self, item: &'a ListItem) -> Element<'a, Message> {
        let is_selected = self.selected_item_id == Some(item.id);

        let colors = [
            Color::from_rgb(1.0, 0.6, 0.6),   // Red
            Color::from_rgb(0.6, 1.0, 0.6),   // Green
            Color::from_rgb(0.6, 0.6, 1.0),   // Blue
            Color::from_rgb(1.0, 1.0, 0.6),   // Yellow
            Color::from_rgb(1.0, 0.6, 1.0),   // Magenta
        ];
        let color = colors[item.id % colors.len()];

        // Make the color more vibrant for selected items
        let selected_color = Color::from_rgb(
            (color.r * 1.2).min(1.0),
            (color.g * 1.2).min(1.0),
            (color.b * 1.2).min(1.0)
        );

        let content = row![
            container(text(" "))
                .width(Length::Fixed(if is_selected { 10.0 } else { 6.0 })) // Wider indicator when selected
                .height(Length::Shrink)
                .style(move |_theme| container::Style {
                    background: Some(Background::Color(if is_selected { selected_color } else { color })),
                    border: border::rounded(3),
                    ..Default::default()
                }),
            column![
                text(&item.name)
                    .size(16)
                    .color(if is_selected {
                        // Text color that matches the item's color theme
                        Color::from_rgb(
                            0.1 + color.r * 0.3,
                            0.1 + color.g * 0.3,
                            0.1 + color.b * 0.3
                        )
                    } else {
                        Color::from_rgb(0.2, 0.2, 0.2)
                    }),
                text(&item.description)
                    .size(13)
                    .color(if is_selected {
                        // Description color that matches the item's color theme but lighter
                        Color::from_rgb(
                            0.3 + color.r * 0.3,
                            0.3 + color.g * 0.3,
                            0.3 + color.b * 0.3
                        )
                    } else {
                        Color::from_rgb(0.5, 0.5, 0.5)
                    })
            ]
            .spacing(5)
            .padding(Padding {
                top: 8.0,
                right: 12.0,
                bottom: 8.0,
                left: 12.0,
            })
        ];

        button(content)
            .on_press(Message::ItemClicked(item.id))
            .width(Length::Fill)
            .style(move |_theme, status| button::Style {
                background: match (status, is_selected) {
                    (_, true) => Some(Background::Color(Color::from_rgb(
                        0.97 + color.r * 0.03,
                        0.97 + color.g * 0.03,
                        0.97 + color.b * 0.03
                    ))),
                    (button::Status::Hovered, _) => Some(Background::Color(Color::from_rgb(0.98, 0.98, 0.98))),
                    _ => Some(Background::Color(Color::WHITE)),
                },
                border: Border {
                    color: if is_selected { 
                        // Border color that matches the item's color
                        Color::from_rgb(
                            (color.r * 0.8).max(0.7),
                            (color.g * 0.8).max(0.7),
                            (color.b * 0.8).max(0.7)
                        )
                    } else {
                        Color::from_rgb(0.9, 0.9, 0.9)
                    },
                    width: if is_selected { 2.0 } else { 1.0 },
                    radius: 6.0.into(),
                },
                ..Default::default()
            })
            .into()
    }

    fn create_terminal_item<'a>(&'a self, item: &'a ListItem) -> Element<'a, Message> {
        let is_selected = self.selected_item_id == Some(item.id);

        let content = row![
            text(if is_selected { "#" } else { "$" }) // Different prompt for selected items
                .size(14)
                .color(if is_selected {
                    Color::from_rgb(1.0, 0.8, 0.0) // Yellow for selected
                } else {
                    Color::from_rgb(0.0, 1.0, 0.0) // Green for normal
                }),
            text(&item.name)
                .size(14)
                .color(if is_selected {
                    Color::from_rgb(1.0, 1.0, 0.0) // Bright yellow for selected
                } else {
                    Color::from_rgb(0.8, 0.8, 0.8) // Light gray for normal
                }),
            text(if is_selected { ">>" } else { "//" })
                .size(12)
                .color(if is_selected {
                    Color::from_rgb(0.8, 0.8, 0.0) // Darker yellow for selected
                } else {
                    Color::from_rgb(0.5, 0.5, 0.5) // Gray for normal
                }),
            text(&item.description)
                .size(12)
                .color(if is_selected {
                    Color::from_rgb(0.9, 0.9, 0.0) // Yellow for selected
                } else {
                    Color::from_rgb(0.6, 0.6, 0.6) // Light gray for normal
                })
        ]
            .spacing(8)
            .align_y(Alignment::Center)
            .padding(Padding::new(8.0));

        button(content)
            .on_press(Message::ItemClicked(item.id))
            .width(Length::Fill)
            .style(move |_theme, status| button::Style {
                background: match (status, is_selected) {
                    (_, true) => Some(Background::Color(Color::from_rgb(0.2, 0.2, 0.0))), // Dark yellow background
                    (button::Status::Hovered, _) => Some(Background::Color(Color::from_rgb(0.1, 0.1, 0.1))),
                    _ => None,
                },
                border: if is_selected {
                    Border {
                        color: Color::from_rgb(0.5, 0.5, 0.0), // Yellow border
                        width: 1.0,
                        radius: 0.0.into(), // No rounded corners for terminal style
                    }
                } else {
                    Border::default()
                },
                ..Default::default()
            })
            .into()
    }
}

pub fn main() -> iced::Result {
    iced::application(
        "Themed ListView Examples",
        ThemedListView::update,
        ThemedListView::view
    )
        .settings(iced::Settings::default())
        .run_with(ThemedListView::new)
}
