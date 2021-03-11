use iced::{executor, Application, Command, Element, Settings};

use crate::{cli::CacheMode, error::prelude::*};

struct Gui;

#[derive(Debug)]
enum Message {}

impl Application for Gui {
    type Executor = executor::Default;
    type Flags = ();
    type Message = Message;

    fn new((): ()) -> (Self, Command<Message>) { (Self, Command::none()) }

    fn title(&self) -> String { "disson".into() }

    fn update(&mut self, msg: Message) -> Command<Message> { match msg {} }

    fn view(&mut self) -> Element<Message> { iced::Column::new().into() }
}

pub fn run(cache_mode: CacheMode) -> Result<()> {
    Gui::run(Settings {
        antialiasing: true,
        ..Settings::default()
    })
    .map_err(|e| anyhow!("iced failed to initialize: {}", e))?;

    Ok(())
}
