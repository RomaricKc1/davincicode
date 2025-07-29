use regex::Regex;
use std::error::Error;
use std::io::{self};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::{self, sleep, Duration};

use crossterm::event::poll;
use crossterm::event::Event::Key;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::{
    Alignment, Backend, Constraint, CrosstermBackend, Direction, Frame, Layout, Span,
};
use ratatui::widgets::*;

use ratatui::Terminal;

use clap::Parser;

/// The client to the davinci code game
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// User name
    #[arg(short, long)]
    name: String,

    /// Server address
    #[arg(short, long, default_value_t = String::from("127.0.0.1"))]
    addr: String,

    /// Server port
    #[arg(short, long, default_value_t = String::from("8079"))]
    port: String,
}

pub enum InputMode {
    Normal,
    Message,
}

pub struct App {
    pub logs: String,
    pub input: String,
    pub name: String,
    pub message: String,
    pub mode: InputMode,
    pub player: davincicode::Player,
    pub tmp_deck: Vec<davincicode::Card>,
    pub opp_deck: Vec<davincicode::Card>,
    pub log_scroll: u16,
}

const MAX_SCROLL: u16 = 65535;

impl App {
    pub fn new(name: String, ncards: u32) -> App {
        let none_deck: Vec<davincicode::Card> = Vec::new();

        App {
            input: String::new(),
            message: String::new(),
            name: String::new(),
            logs: String::new(),
            mode: InputMode::Normal,
            player: davincicode::Player::new(name, ncards),
            tmp_deck: none_deck.clone(),
            opp_deck: none_deck.clone(),
            log_scroll: 0,
        }
    }
    pub fn clear_msg_filed(&mut self) {
        self.input.clear();
    }

    pub fn log_add_top(&mut self, new_log: String) {
        let tmp = &self.logs;
        self.logs = new_log + "\n_____________________________\nOld msg\n" + tmp;
    }

    pub fn log_scroll_next(&mut self) {
        if self.log_scroll < MAX_SCROLL - 1 {
            self.log_scroll += 1;
            self.log_scroll %= 10;
        }
    }
    pub fn log_scroll_prev(&mut self) {
        if self.log_scroll >= 1 {
            self.log_scroll -= 1;
            self.log_scroll %= 10;
        }
    }
}
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let address = format!("{}:{}", args.addr, args.port);
    let name = args.name.trim();

    let mut app = App::new(name.to_string(), 4);
    app.name = name.to_string();

    let mut buffer = [0u8; 1024];
    let mut stream = TcpStream::connect(address.clone()).await.unwrap();
    app.log_add_top(format!("{} {}\n", "Connected to server at", address));

    let init_message = "init";
    stream.write_all(init_message.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    let bytes_read = stream.read(&mut buffer).await.unwrap();
    let response = String::from_utf8_lossy(&buffer[..bytes_read]);

    app.log_add_top(format!("{} {}\n\n\n", "Response from server:", response));

    stream.write_all(name.as_bytes()).await.unwrap();
    stream.flush().await.unwrap();

    app.log_add_top(format!("{}\n", "Sent name and init to server"));

    ///////////////////////////////////////////////////////////////////////////////////
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(&mut terminal, &mut stream, &mut app, name.to_string()).await;

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }
    println!("exit successfully");

    return Ok(());
}

async fn update_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &App,
) -> Result<(), std::io::Error> {
    terminal.draw(|f| ui2(f, app))?;
    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    stream: &mut TcpStream,
    app: &mut App,
    _name: String,
) -> Result<(), std::io::Error> {
    let mut awaiting_msg_transfer: bool = false;

    loop {
        let _ = update_ui(terminal, app).await;
        let mut buffer = [0u8; 1024];

        if poll(Duration::from_millis(500))? {
            if let Key(key) = event::read()? {
                match app.mode {
                    InputMode::Normal => match key.code {
                        KeyCode::Char('q') => {
                            return Ok(());
                        }
                        KeyCode::Char('i') => {
                            app.mode = InputMode::Message;
                        }
                        KeyCode::Char('j') => {
                            app.log_scroll_next();
                        }
                        KeyCode::Char('k') => {
                            app.log_scroll_prev();
                        }

                        _ => {}
                    },
                    InputMode::Message => match key.code {
                        KeyCode::Esc => {
                            app.clear_msg_filed();
                            app.mode = InputMode::Normal;
                        }
                        KeyCode::Char(c) => {
                            app.input.push(c);
                        }
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Enter => {
                            app.message.clear();
                            app.message.push_str(app.input.trim());
                            app.input.clear();
                            awaiting_msg_transfer = true;
                        }
                        _ => {}
                    },
                }
            }
        } else {
            if awaiting_msg_transfer {
                // flush this message to the server
                if app.message != String::new() {
                    stream.write_all(app.message.as_bytes()).await.unwrap();
                    stream.flush().await.unwrap();

                    awaiting_msg_transfer = false;
                    app.message.clear();
                }
            }
            let read_timeout =
                time::timeout(Duration::from_millis(200), stream.read(&mut buffer)).await;

            if let Ok(value) = read_timeout {
                let bytes_read = value.unwrap();
                let response = String::from_utf8_lossy(&buffer[..bytes_read]);

                app.log_add_top(format!("{} {}\n\n\n", "Response from server:", response));

                if let Some(deck) = parse_responses(&response, "##") {
                    app.player.deck_from_str(deck);
                }

                if let Some(deck) = parse_responses(&response, "**") {
                    app.tmp_deck = deck_from_str(deck);
                }

                if let Some(deck) = parse_responses(&response, "++") {
                    app.opp_deck = deck_from_str(deck);
                }
                if let Some(won_player) = parse_responses(&response, "||") {
                    if won_player != app.name {
                        app.log_add_top(format!(
                            "{} {} is the winner\n\n\n",
                            "You lost. :(", won_player
                        ));
                    }
                    let _ = update_ui(terminal, app).await;

                    sleep(Duration::from_secs(10)).await;
                    break Ok(());
                }

                if response.trim().contains("It's your turn") {
                    app.mode = InputMode::Message;
                    // app.log_add_top(format!("{}\n", "Enter something"));
                    // let _ = update_ui(terminal, &app).await;
                } else if response.trim().starts_with("You won! Congrats!") {
                    app.mode = InputMode::Normal;
                    app.log_add_top(format!("{}\n", "Nice, You're the winner. Exiting."));
                    let _ = update_ui(terminal, app).await;

                    sleep(Duration::from_secs(20)).await;

                    break Ok(());
                } else {
                    app.mode = InputMode::Normal;
                }

                if bytes_read == 0 {
                    break Ok(());
                }
            }
        }
    }
}

fn ui2(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(f.area());

    let inner_layout = Layout::new(
        Direction::Vertical,
        [Constraint::Percentage(50), Constraint::Percentage(50)],
    )
    .split(chunks[0]);

    let inner_layout2 = Layout::new(
        Direction::Vertical,
        [
            Constraint::Percentage(50),
            Constraint::Percentage(40),
            Constraint::Percentage(10),
        ],
    )
    .split(chunks[1]);

    ////////////////////////////////////////////////////////////////////////////////////////////////
    // input layout
    let user_input = Paragraph::new(app.input.to_owned())
        .block(
            Block::default()
                .title("Commands")
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded),
        )
        .style(ratatui::prelude::Style::default());

    f.render_widget(user_input, inner_layout2[2]);

    // Logs Paragraph
    let logs = ratatui::prelude::Text::from(app.logs.clone());
    let log_p = Paragraph::new(logs)
        .block(
            Block::new()
                .title("Logs (reverse order)")
                .borders(Borders::ALL),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .scroll((app.log_scroll, 0));

    f.render_widget(log_p, inner_layout2[0]);

    /////////////////////////////////////////////////////
    // card view player
    let card_grid_layout = Layout::new(
        Direction::Vertical,
        [
            Constraint::Percentage(5),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
        ],
    )
    .split(inner_layout[0]);

    let a_box = Block::default()
        .title(format!("Your deck {}", app.name))
        .borders(Borders::ALL);
    f.render_widget(a_box, card_grid_layout[0]);

    let mut idx_ = 0;
    for (chunk_id, _) in card_grid_layout.iter().enumerate() {
        if chunk_id == 0 {
            continue;
        }
        let sub_inner_layout = Layout::new(
            Direction::Horizontal,
            [
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ],
        )
        .split(card_grid_layout[chunk_id]);
        for (chunk, _) in sub_inner_layout.iter().enumerate() {
            if let Some(card_item) = app.player.deck.get(idx_) {
                idx_ += 1;
                let s = match card_item.color {
                    davincicode::Color::BLACK => {
                        ratatui::prelude::Style::default().fg(ratatui::prelude::Color::Blue)
                    }
                    davincicode::Color::WHITE => {
                        ratatui::prelude::Style::default().fg(ratatui::prelude::Color::Yellow)
                    }
                };

                let card_color = match card_item.color {
                    davincicode::Color::BLACK => "B",
                    davincicode::Color::WHITE => "W",
                };

                let card_p = Paragraph::new(Span::styled(
                    format!("{} {}", card_color, card_item.value,),
                    s,
                ))
                .block(Block::new().title("card").borders(Borders::ALL))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });

                f.render_widget(card_p, sub_inner_layout[chunk]);
            }
        }
    }
    /////////////////////////////////////////////////////
    // card view opponents
    let what_opponents_see = Layout::new(
        Direction::Vertical,
        [
            Constraint::Percentage(5),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
        ],
    )
    .split(inner_layout2[1]);

    let a_box = Block::default()
        .title("Opponents view")
        .borders(Borders::ALL);
    f.render_widget(a_box, what_opponents_see[0]);

    let mut idx_ = 0;
    for (chunk_id, _) in what_opponents_see.iter().enumerate() {
        if chunk_id == 0 {
            continue;
        }

        let sub_inner_layout = Layout::new(
            Direction::Horizontal,
            [
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ],
        )
        .split(what_opponents_see[chunk_id]);

        for (chunk, _) in sub_inner_layout.iter().enumerate() {
            if let Some(card_item) = app.player.deck.get(idx_) {
                idx_ += 1;
                let s = match card_item.color {
                    davincicode::Color::BLACK => {
                        ratatui::prelude::Style::default().fg(ratatui::prelude::Color::Blue)
                    }
                    davincicode::Color::WHITE => {
                        ratatui::prelude::Style::default().fg(ratatui::prelude::Color::Yellow)
                    }
                };

                let card_color = match card_item.color {
                    davincicode::Color::BLACK => "B",
                    davincicode::Color::WHITE => "W",
                };

                let card_value = if card_item.status == davincicode::CardStatus::HIDDEN {
                    "?".to_owned()
                } else {
                    format!("{}", card_item.value)
                };
                let card_p =
                    Paragraph::new(Span::styled(format!("{} {}", card_color, card_value), s))
                        .block(Block::new().title("card").borders(Borders::ALL))
                        .alignment(Alignment::Center)
                        .wrap(Wrap { trim: true });

                f.render_widget(card_p, sub_inner_layout[chunk]);
            }
        }
    }

    /////////////////////////////////////////////////////
    // tmp deck either current opponent or game set
    let opponent_card_grid_layout = Layout::new(
        Direction::Vertical,
        [
            Constraint::Percentage(5),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
            Constraint::Percentage(21),
        ],
    )
    .split(inner_layout[1]);
    let a_box = Block::default()
        .title("Current opponent cards / game set")
        .borders(Borders::ALL);
    f.render_widget(a_box, opponent_card_grid_layout[0]);

    let mut idx_ = 0;
    for (chunk_id, _) in opponent_card_grid_layout.iter().enumerate() {
        if chunk_id == 0 {
            continue;
        }

        let sub_inner_layout = Layout::new(
            Direction::Horizontal,
            [
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ],
        )
        .split(opponent_card_grid_layout[chunk_id]);

        for (chunk, _) in sub_inner_layout.iter().enumerate() {
            if let Some(card_item) = app.tmp_deck.get(idx_) {
                idx_ += 1;
                let s = match card_item.color {
                    davincicode::Color::BLACK => {
                        ratatui::prelude::Style::default().fg(ratatui::prelude::Color::Blue)
                    }
                    davincicode::Color::WHITE => {
                        ratatui::prelude::Style::default().fg(ratatui::prelude::Color::Yellow)
                    }
                };

                let card_color = match card_item.color {
                    davincicode::Color::BLACK => "B",
                    davincicode::Color::WHITE => "W",
                };

                let card_value = if card_item.status == davincicode::CardStatus::HIDDEN {
                    "?".to_owned()
                } else {
                    format!("{}", card_item.value)
                };
                let card_p =
                    Paragraph::new(Span::styled(format!("{} {}", card_color, card_value), s))
                        .block(Block::new().title("card").borders(Borders::ALL))
                        .alignment(Alignment::Center)
                        .wrap(Wrap { trim: true });

                f.render_widget(card_p, sub_inner_layout[chunk]);
            }
        }
    }
}

pub fn deck_from_str(deck_str: String) -> Vec<davincicode::Card> {
    //str_deck.push_str("0: W1, 1: B1, 2: W13, 3: B2");
    let card_items = deck_str.split(",");
    let mut ret: Vec<davincicode::Card> = Vec::new();

    for card in card_items {
        let card: Vec<_> = card.split(":").collect();

        if let Some(card) = card.get(1) {
            if let Some(card_color) = card.get(0..2) {
                if let Some(card_value) = card.get(2..) {
                    let card_color = match card_color.trim() {
                        "B" => davincicode::Color::BLACK,
                        "W" => davincicode::Color::WHITE,
                        _ => davincicode::Color::BLACK,
                    };

                    match card_value.parse::<u32>() {
                        Ok(value) => {
                            let a_card = davincicode::Card::new(value, card_color);
                            ret.push(a_card);
                        }
                        Err(_) => {
                            // prolly a hidden card, so value is a str "?"
                            let mut a_card = davincicode::Card::new(0, card_color);
                            a_card.status = davincicode::CardStatus::HIDDEN;
                            ret.push(a_card);
                        }
                    }
                }
            }
        }
    }

    ret
}

fn parse_responses(input: &str, pattern: &str) -> Option<String> {
    let ret: String;

    let re_hashtags = Regex::new(r"##([^#]+)##").unwrap();
    let re_asterisks = Regex::new(r"\*\*([^*]+)\*\*").unwrap();
    let re_plus = Regex::new(r"\+\+([^+]+)\+\+").unwrap();
    let re_pipe = Regex::new(r"\|\|([^|]+)\|\|").unwrap();

    let hashtags: Vec<&str> = re_hashtags
        .captures_iter(input)
        .map(|caps| caps.get(1).unwrap().as_str())
        .collect();

    let asterisks: Vec<&str> = re_asterisks
        .captures_iter(input)
        .map(|caps| caps.get(1).unwrap().as_str())
        .collect();

    let plus: Vec<&str> = re_plus
        .captures_iter(input)
        .map(|caps| caps.get(1).unwrap().as_str())
        .collect();

    let pipe: Vec<&str> = re_pipe
        .captures_iter(input)
        .map(|caps| caps.get(1).unwrap().as_str())
        .collect();

    match pattern {
        "##" => {
            if let Some(res) = hashtags.first() {
                ret = res.to_string();
                return Some(ret);
            }
        }
        "**" => {
            if let Some(res) = asterisks.first() {
                ret = res.to_string();
                return Some(ret);
            }
        }

        "++" => {
            if let Some(res) = plus.first() {
                ret = res.to_string();
                return Some(ret);
            }
        }

        "||" => {
            if let Some(res) = pipe.first() {
                ret = res.to_string();
                return Some(ret);
            }
        }

        _ => {
            //
        }
    }

    None
}
