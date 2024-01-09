use colored::Colorize;
use rand::Rng;
use std::collections::HashMap;
use std::process;
use std::sync::Arc;
use std::{error::Error, io};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::{sleep, Duration};

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

const GAME_END_CODE: i32 = -44;
const CARD_MAX_VAL: u32 = 11;
const CARD_PER_PLAYER: u32 = 4;
const START_CARD_N: u32 = 24;

use clap::Parser;

/// The Server of the davinci code game
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Server address
    #[arg(short, long, default_value_t = String::from("8079"))]
    addr: String,

    /// Server port
    #[arg(short, long, default_value_t = String::from("127.0.0.1"))]
    port: String,

    /// Number of players
    #[arg(short, long, default_value_t = 2)]
    nplayers: u16,
}

async fn loop_read_uint(stream: &mut TcpStream, msg: String, range: Vec<u32>) -> Option<u32> {
    let result: u32;

    if range.len() > 2 {
        println!("Only 2 value allowed (min-max).");
        return None;
    }

    let max = range[1];
    loop {
        let mut to_send = String::new();
        let msg = format!("{} (0-{})", msg, max);

        to_send.push_str(format!("{}\n", msg).as_str());
        if send_something(stream, &to_send).await {
            return None;
        }

        let response = recv_something(stream).await?;
        let parsed_response = response.parse::<u32>();

        match parsed_response {
            Ok(value) => {
                if value <= max {
                    result = value;
                    break;
                }
            }
            Err(_) => {}
        }
    }

    Some(result)
}

async fn loop_read_str(
    stream: &mut TcpStream,
    msg: String,
    variant: Vec<String>,
) -> Option<String> {
    if variant.len() > 2 {
        println!("Only 2 variant allowed.");
        return None;
    }
    let v0 = variant[0].clone();
    let v1 = variant[1].clone();

    loop {
        let mut to_send = String::new();
        let msg = format!("{} ({}/{})", msg, variant[0], variant[1]);

        to_send.push_str(format!("{}\n", msg).as_str());
        if send_something(stream, &to_send).await {
            return None;
        }

        let response = recv_something(stream).await?;
        match response.trim() {
            value => {
                if value == v0 || value == v1 {
                    return Some(response);
                }
            }
        }
    }
}

async fn yes_variant_guess<B: Backend>(
    terminal: &mut Terminal<B>,
    stream: &mut TcpStream,
    player_name: String,
    the_game: &mut davincicode::Game,
    dialog_status: &mut i32,
) -> i32 {
    // loop to keep guessing the opponent card
    loop {
        // println!("Continuing with guess");

        let game_ended = the_game.game_status(); // probably modified the players vec

        if game_ended {
            // game has ended
            *dialog_status = GAME_END_CODE;

            the_game
                .logs
                .push_str(format!("{}", "No more opponent\n").as_str());
            let _ = update_ui(terminal, the_game).await;

            return -1;
        }

        // update the opponents_names with the new players (if any player lost and got removed)
        let mut opponents_names: Vec<String> = Vec::new();
        for p in the_game.players.iter() {
            if p.name != String::from(player_name.clone()) {
                opponents_names.push(String::from(&p.name));
            }
        }

        the_game
            .logs
            .push_str(format!("{} {:?}\n", "The opponents_names:", opponents_names).as_str());
        let _ = update_ui(terminal, the_game).await;

        let mut continue_guess = false;

        let returned_val = guess_opponent_card_loop(
            terminal,
            stream,
            the_game,
            opponents_names.clone(),
            &player_name,
        )
        .await;

        if returned_val == 1 {
            // good guess, either continue or break and save hidden card
            let response = loop_read_str(
                stream,
                "It's your turn: Would you like to make another guess?".to_string(),
                vec!["yes".to_string(), "no".to_string()],
            )
            .await
            .expect("Got no response");

            match response.as_str() {
                "yes" => {
                    // continue here
                    continue_guess = true;
                }
                "no" => {
                    *dialog_status = 1;
                }
                _ => {}
            }
        } else {
            // guessed the opponent card and lost, turn lost also, and card getting revealed
            *dialog_status = 2;
            break;
        }

        if continue_guess == true {
            continue;
        } else {
            break;
        }
    }

    0
}

async fn player_move<B: Backend>(
    terminal: &mut Terminal<B>,
    player_name: String,
    player_tcp_name: &mut HashMap<String, TcpStream>,
    the_game: &mut davincicode::Game,
) -> i32 {
    let mut dialog_status: i32 = -1;
    let stream = player_tcp_name.get_mut(&(player_name.clone())).unwrap();

    // get current players card both own view and opponent view and show it to them
    let mut game_context = String::new();
    let max_card_avail_value = the_game.card_avail.len() as u32;
    let mut can_t_draw_any: bool = false;

    if max_card_avail_value < 1 {
        can_t_draw_any = true;
    }

    the_game.shuffle_avail_card();

    let current_player = the_game
        .players
        .iter()
        .find(|player| player.name == player_name.to_string())
        .expect("No player found\n");

    if can_t_draw_any {
        // no more cards on the set to draw, so take turn guessing op card
        game_context
            .push_str(format!("\n{}\n", "No more cards avail. Only guessing now.\n").as_str());
    } else {
        game_context.push_str(format!("{}", "All avail cards: **").as_str());
        game_context.push_str(&the_game.show_avail_cards(true, false));
        game_context.push_str("**");
    }
    let _ = update_ui(terminal, the_game).await;

    // build the opponents_names with the new players (if any player lost and got removed)
    let mut opponents_names: Vec<String> = Vec::new();
    for p in the_game.players.iter() {
        if p.name != String::from(player_name.clone()) {
            opponents_names.push(String::from(&p.name));
        }
    }
    game_context.push_str(format!("\n{}", "Your opponents deck: ").as_str());
    for opponent in the_game.players.iter() {
        if opponent.name == String::from(opponents_names.get(0).unwrap()) {
            game_context.push_str(format!("\n{}", "Player: ").as_str());
            game_context.push_str(&opponent.name);
            game_context.push_str(format!("{}", " ").as_str());
            game_context.push_str(&opponent.show_hand(true, false));
        }
    }
    if send_something(stream, &game_context).await {
        return 1;
    }

    game_context.clear();
    game_context.push_str(format!("{}", "Your deck: ##").as_str());
    game_context.push_str(&current_player.show_hand(false, false));
    game_context.push_str("##");
    if send_something(stream, &game_context).await {
        return 1;
    }

    game_context.clear();
    game_context.push_str(format!("{}", "What your opponents see: ").as_str());
    game_context.push_str(&current_player.show_hand(true, false));

    game_context.push_str("\n");
    if send_something(stream, &game_context).await {
        return 1;
    }

    // read user input (card to pick)
    let picked_card_number: usize;
    let mut current_player_side_card: Option<davincicode::Card> = None;

    if can_t_draw_any == false {
        let to_send = format!("{} {}\n", "It's your turn", player_name,);
        if send_something(stream, &to_send).await {
            return 1;
        }

        let value = loop_read_uint(
            stream,
            "Enter card number to draw it".to_string(),
            vec![0, max_card_avail_value - 1],
        )
        .await
        .expect("Got no response");

        picked_card_number = value as usize;
        // draw the card here
        for p in the_game.players.iter_mut() {
            if p.name == player_name.clone() {
                current_player_side_card = Some(
                    p.draw_specific_card(&mut the_game.card_avail, picked_card_number as usize),
                );
            }
        }

        let _ = update_ui(terminal, the_game).await;
    }

    // continue to ask if they want to keep it hidden now or guess opponent card
    // with the risk of making a bad guess and getting it revealed
    loop {
        // if can't draw any, change the message
        if can_t_draw_any == false {
            let mut to_send = String::new();

            to_send.push_str(format!("{}", "You picked a ").as_str());
            let the_card = current_player_side_card.expect("No card picked");
            match the_card.color {
                davincicode::Color::BLACK => {
                    to_send.push_str(format!("{}{}", "B", the_card.value.to_string()).as_str());
                }
                davincicode::Color::WHITE => {
                    to_send.push_str(format!("{}{}", "W", the_card.value.to_string()).as_str());
                }
            }
            to_send.push_str(format!("\n{}\n", "Saving it as side card.").as_str());

            to_send.push_str(format!("{}", "All avail cards: **").as_str());
            to_send.push_str(&the_game.show_avail_cards(true, false));
            to_send.push_str("**");

            if send_something(stream, &to_send).await {
                return 1;
            }

            // player drawn a card, so they can decide not to make a guess
            let response = loop_read_str(
                stream,
                "It's your turn: Would you like to make a guess?".to_string(),
                vec!["yes".to_string(), "no".to_string()],
            )
            .await
            .expect("Got no response");

            match response.as_str() {
                "yes" => {
                    // continue with this logic
                    yes_variant_guess(
                        terminal,
                        stream,
                        player_name.to_string(),
                        the_game,
                        &mut dialog_status,
                    )
                    .await;
                }
                "no" => {
                    dialog_status = 1;
                    the_game.logs.push_str("Nah, exit\n");
                    let _ = update_ui(terminal, the_game).await;
                    // println!("Nah, exit");
                }
                _ => {}
            }
            break;
        } else {
            // no more cards to draw, meaning that the only way to play is to make a guess. No
            // more choice. So spawn the yes_variant_guess.
            yes_variant_guess(
                terminal,
                stream,
                player_name.to_string(),
                the_game,
                &mut dialog_status,
            )
            .await;
            break; // yes variant
        }
    }

    match dialog_status {
        1 => {
            // player picked a card, and decided to keep it
            // or picked a card, guessed and won too and refused to keep guessing
            for p in the_game.players.iter_mut() {
                if p.name == player_name.clone() {
                    p.save_side_card(true);
                }
            }

            let mut to_send = format!("Okay, saving your side card as hidden.\n",);

            to_send.push_str(format!("{}", "Your new deck: ##").as_str());
            for p in the_game.players.iter() {
                if p.name == player_name.clone() {
                    to_send.push_str(&p.show_hand(false, false));
                }
            }
            to_send.push_str("##");
            if send_something(stream, &to_send).await {
                return 1;
            }

            to_send.clear();
            to_send.push_str(format!("\n{}", "What your opponents see:").as_str());
            to_send.push_str("");
            for p in the_game.players.iter() {
                if p.name == player_name.clone() {
                    to_send.push_str(&p.show_hand(true, false));
                }
            }
            if send_something(stream, &to_send).await {
                return 1;
            }
        }
        2 => {
            // player picked a card, guess and lost, revealing their card
            for p in the_game.players.iter_mut() {
                if p.name == player_name.clone() {
                    p.save_side_card(false);
                }
            }

            let mut to_send = String::new();

            to_send.push_str(
                format!("\n{}\n", "You made a wrong guess, I'm revealing your card.").as_str(),
            );

            to_send.push_str(format!("{}", "Your new deck: ##").as_str());
            for p in the_game.players.iter() {
                if p.name == player_name.clone() {
                    to_send.push_str(&p.show_hand(false, false));
                }
            }
            to_send.push_str("##");
            if send_something(stream, &to_send).await {
                return 1;
            }

            to_send.push_str(format!("{}", "What your opponents see: ").as_str());

            for p in the_game.players.iter() {
                if p.name == player_name.clone() {
                    to_send.push_str(&p.show_hand(true, false));
                }
            }

            if send_something(stream, &to_send).await {
                return 1;
            }
        }
        GAME_END_CODE => {
            // game ended
            // announce this to the remaining player
            let to_send = format!("{}", "You won! Congrats!",);
            if send_something(stream, &to_send).await {
                return 1;
            }

            the_game
                .logs
                .push_str(format!("We got a winner: {:?}\n", the_game.winner).as_str());
            let _ = update_ui(terminal, the_game).await;
            // println!("We got a winner: {:?}\n", the_game.winner);

            return 0;
        }
        _ => {
            // what??
        }
    }
    return 2;
    //
}

async fn guess_opponent_card_loop<B: Backend>(
    terminal: &mut Terminal<B>,
    stream: &mut TcpStream,
    the_game: &mut davincicode::Game,
    opponents_names: Vec<String>,
    player_name: &String,
) -> i32 {
    let mut picked_card_number: usize;
    let mut golden_value: u32 = u32::MAX;
    let mut correct_guess: bool = false;
    let mut opponent_deck_len: u32 = 0;

    let mut skip_chose_op = false;

    loop {
        if opponents_names.len() == 1 {
            skip_chose_op = true;
        }

        // show opponents deck so the player can decice which one to guess
        let mut to_send = String::new();
        to_send.push_str(format!("\n{}", "Your opponents deck: ").as_str());

        for (idx, a_player) in the_game.players.iter().enumerate() {
            if a_player.name == player_name.clone() {
                continue;
            }
            to_send.push_str(
                format!(
                    "{}: {} => {}",
                    idx,
                    (&a_player.name),
                    (&a_player.show_hand(true, false)) // opponent view = true
                )
                .as_str(),
            );
        }

        if send_something(stream, &to_send).await {
            return -2;
        }

        let mut op_idx = 0;
        if skip_chose_op == false {
            // pick the opponent
            op_idx = loop_read_uint(
                stream,
                "It's your turn! Pick current opponent for this guess: \n".to_string(),
                vec![0, opponents_names.len() as u32 - 1],
            )
            .await
            .expect("Got no response");
        } // else, skip, and the op_idx will be 0

        let opponent_name_ = String::from(opponents_names.get(op_idx as usize).unwrap()); //.unwrap());

        let to_send = format!("{} {}", "The chosen opponent name is:", opponent_name_,);
        if send_something(stream, &to_send).await {
            return -2;
        }

        the_game
            .logs
            .push_str(format!("{} {}\n", "The chosen opponent name is:", opponent_name_).as_str());
        let _ = update_ui(terminal, the_game).await;

        // show this current opponent deck
        let mut to_send = format!("{}", "Their deck: ++",);
        for opponent in the_game.players.iter() {
            if opponent.name == opponent_name_ {
                to_send.push_str(&opponent.show_hand(true, false));
                opponent_deck_len = opponent.deck.len() as u32;
            }
        }
        to_send.push_str("++");
        if send_something(stream, &to_send).await {
            return -2;
        }

        to_send.clear();
        to_send.push_str(format!("\n{}", "Your current deck: ##").as_str());
        for p in the_game.players.iter() {
            if p.name != opponent_name_ {
                // so it gotta be the current player
                to_send.push_str(&p.show_hand(false, false));
            }
        }
        to_send.push_str("##");
        if send_something(stream, &to_send).await {
            return -2;
        }

        to_send.clear();
        to_send.push_str(format!("{}", "What your opponents see: ").as_str());
        to_send.push_str("");
        for p in the_game.players.iter() {
            if p.name != opponent_name_ {
                // so it gotta be the current player
                to_send.push_str(&p.show_hand(true, false));
            }
        }

        if send_something(stream, &to_send).await {
            return -2;
        }

        // request the player which opponent card they want to guess the value
        let value = loop_read_uint(
            stream,
            "It's your turn: Which card would you like to guess".to_string(),
            vec![0, opponent_deck_len - 1],
        )
        .await
        .expect("Got no response");

        picked_card_number = value as usize;

        the_game
            .logs
            .push_str(format!("\n{} {}\n", "Picked card number:", picked_card_number).as_str());
        let _ = update_ui(terminal, the_game).await;

        // only 1 opponent, read their specific card value
        for opponent in the_game.players.iter() {
            if opponent.name == opponent_name_ {
                golden_value = match opponent.get_specific_card_value(picked_card_number) {
                    None => u32::MAX,
                    Some(val) => val,
                };
            }
        }
        // valid pick?
        if golden_value == u32::MAX {
            // card was not hidden, restart the process
            continue;
        }
        // request the player to give their guessed value of the card
        let guessed_value = loop_read_uint(
            stream,
            "It's your turn: Enter your guess: value between".to_string(),
            vec![0, CARD_MAX_VAL],
        )
        .await
        .expect("Got no response");

        // evaluate the guess
        if golden_value == guessed_value {
            correct_guess = true;
            the_game
                .logs
                .push_str(format!("{}\n", "Good guess!").as_str());
            let _ = update_ui(terminal, the_game).await;

            // ack correct guess
            let mut to_send = String::new();
            to_send.push_str(format!("{}", "You got it right! Guessed card revealed\n").as_str());
            if send_something(stream, &to_send).await {
                return -2;
            }

            // mutate the opponent card to revealed
            for opponent in the_game.players.iter_mut() {
                if opponent.name == opponent_name_ {
                    // the opponent card get's revealed
                    opponent.reveal_card(picked_card_number);
                }
            }

            let mut to_send = String::new();
            to_send.push_str("Here's the new opponent deck: ++");
            // show the player the new opponent deck
            for opponent in the_game.players.iter() {
                if opponent.name == opponent_name_ {
                    to_send.push_str(&opponent.show_hand(true, false));
                }
            }
            to_send.push_str("++");
            if send_something(stream, &to_send).await {
                return -2;
            }
            break;
        } else {
            // else we just break and return 0 meaning it was a wrong guess
            break;
        }
    }
    //
    if correct_guess == true {
        return 1;
    }
    return 0;
}

async fn game_process<B: Backend>(
    terminal: &mut Terminal<B>,
    the_game: &mut davincicode::Game,
    selected_player_name: String,
    player_tcp_name: &mut HashMap<String, TcpStream>,
) -> i32 {
    let mut player_order: Vec<String> = Vec::new();
    let mut current_player = selected_player_name.clone();

    // push the selected player first
    player_order.push(current_player.clone());

    loop {
        if the_game.game_status() == true {
            break;
        }
        // report to lost players
        for lost_player in the_game.lost_players.iter() {
            let to_send = format!("{}", "You lost. Sorry".red());

            if player_tcp_name.contains_key(&lost_player.name.clone()) {
                let stream = player_tcp_name.get_mut(&lost_player.name.clone()).unwrap();
                send_something(stream, &to_send).await;
            }
        }

        // get next player
        let next_player = match player_order.last() {
            Some(player) => player_tcp_name.keys().find(|&name| name != player),
            None => player_tcp_name
                .keys()
                .find(|&name| name != selected_player_name.as_str()),
        };

        // check if there are no more players or if the current player has been dropped
        if next_player.is_none() || !player_tcp_name.contains_key(&current_player) {
            break;
        }

        // update the current player and add it to the order
        current_player = next_player.unwrap().to_string();
        player_order.push(current_player.clone());

        // iterate over the player order and perform turn-taking logic
        for player in player_order.iter() {
            let cnt = the_game
                .players
                .iter()
                .filter(|p| p.name.as_str() == player)
                .count();
            if cnt == 0 {
                // the current player has been dropped cause they lost
                continue;
            }
            if player_move(terminal, player.to_string(), player_tcp_name, the_game).await == 0 {
                break;
            }
        }
    }

    the_game
        .logs
        .push_str(format!("{}\n", "left game_process").as_str());
    let _ = update_ui(terminal, the_game).await;

    return 0;
}

async fn game_run(
    player_names: HashMap<u32, String>,
    selected_player_name: String,
    player_tcp_name: &mut HashMap<String, TcpStream>,
) -> Result<(), Box<dyn Error>> {
    let mut the_game = davincicode::Game::new(START_CARD_N);

    for player in player_names.iter() {
        let p = davincicode::Player::new(String::from(player.1), CARD_PER_PLAYER);
        the_game.players.push(p);
    }
    the_game.init_set();

    ///////////////////////////////////////////////////////////////////////////////////
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_app(
        &mut terminal,
        &mut the_game,
        selected_player_name,
        player_tcp_name,
    )
    .await;

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

fn ui2(f: &mut Frame, the_game: &davincicode::Game) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(f.size());

    let inner_layout = Layout::new(
        Direction::Vertical,
        [Constraint::Percentage(50), Constraint::Percentage(50)],
    )
    .split(chunks[0]);

    let players: Vec<ListItem> = the_game
        .players
        .iter()
        .rev()
        .map(|player| {
            let header = ratatui::prelude::Line::from(vec![Span::styled(
                format!("{}", player.name),
                ratatui::prelude::Style::default(),
            )]);

            ListItem::new(vec![
                ratatui::prelude::Line::from("-".repeat(chunks[1].width as usize)),
                header,
                ratatui::prelude::Line::from(""),
            ])
        })
        .collect();

    let players_list = List::new(players)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Current players"),
        )
        .direction(ListDirection::BottomToTop);
    f.render_widget(players_list, inner_layout[0] /*chunks[0]*/);

    // Logs Paragraph
    let logs = ratatui::prelude::Text::from(the_game.logs.clone());
    let log_p = Paragraph::new(logs)
        .block(Block::new().title("Logs").borders(Borders::ALL))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(log_p, chunks[1]);

    /*
    let avail_cards_set: Vec<ListItem> = the_game
        .card_avail
        .iter()
        .rev()
        .map(|card| {
            let s = match card.color {
                davincicode::Color::BLACK => {
                    ratatui::prelude::Style::default().fg(ratatui::prelude::Color::Blue)
                }
                davincicode::Color::WHITE => {
                    ratatui::prelude::Style::default().fg(ratatui::prelude::Color::Yellow)
                }
            };

            let header =
                ratatui::prelude::Line::from(vec![Span::styled(format!("{}", card.value), s)]);
            ListItem::new(vec![
                ratatui::prelude::Line::from("-".repeat(inner_layout[1].width as usize)),
                header,
            ])
        })
        .collect();

    let card_len = avail_cards_set.len();
    let card_set_list = List::new(avail_cards_set)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("Cards available {card_len}")),
        )
        .direction(ListDirection::BottomToTop);
    f.render_widget(card_set_list, inner_layout[1]);*/

    let card_grid_layout = Layout::new(
        Direction::Horizontal,
        [
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ],
    )
    .split(inner_layout[1]);

    let mut idx_ = 0;
    for (chunk_id, _) in card_grid_layout.iter().enumerate() {
        // for each layout, /4
        let sub_inner_layout = Layout::new(
            Direction::Vertical,
            [
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ],
        )
        .split(card_grid_layout[chunk_id]);

        for (chunk, _) in sub_inner_layout.iter().enumerate() {
            if let Some(card_item) = the_game.card_avail.get(idx_) {
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
                    format!("{} {}", card_color, card_item.value),
                    s,
                ))
                .block(Block::new().title("card").borders(Borders::ALL))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true });

                f.render_widget(card_p, sub_inner_layout[chunk]);
            }
        }
    }
}

async fn init_players(client_streams_vec: Arc<Mutex<Vec<TcpStream>>>) {
    let mut streams = client_streams_vec.lock().await;
    let num_clients = streams.len();

    if num_clients == 0 {
        return;
    }

    let mut player_names: HashMap<u32, String> = HashMap::new();
    let mut player_tcp_name: HashMap<String, TcpStream> = HashMap::new();

    // rq to clients to identify themselves
    for (index, client_stream) in streams.iter_mut().enumerate() {
        let request = recv_something(client_stream).await.unwrap();

        let name = request.trim().to_string();
        player_names.insert(index as u32, name.clone());
    }

    let indices: Vec<usize> = (0..player_names.len()).rev().collect();
    for idx in indices {
        let name = player_names.get(&(idx as u32)).unwrap();
        let tcp_stream = streams.remove(idx);
        player_tcp_name.insert(name.to_string(), tcp_stream);
    }

    println!("Players {:?}", player_tcp_name);

    let mut rng = rand::thread_rng();
    let selected_player_index = rng.gen_range(0..player_tcp_name.len());
    let some_player_name = player_tcp_name
        .keys()
        .nth(selected_player_index)
        .unwrap()
        .clone();

    let some_player_stream = player_tcp_name.get_mut(&some_player_name).unwrap();
    let selected_player_name = some_player_name.clone();

    // tell the randomly selected player to move
    println!("Picked {:?} as the first to move.\n", some_player_stream);

    let mut to_send = String::new();
    to_send.push_str(format!("{}\n", "It's your turn, move").as_str());
    if send_something(some_player_stream, &to_send).await {
        return;
    }

    // send wait message to all players except the selected one
    for (name, client_stream) in player_tcp_name.iter_mut() {
        if name == &some_player_name {
            continue;
        }
        if send_something(
            client_stream,
            &format!("{} {}{}", "Wait for your turn, ", name, "\n"),
        )
        .await
        {
            continue;
        }
    }
    let _ = game_run(
        player_names.clone(),
        selected_player_name,
        &mut player_tcp_name,
    )
    .await;
}

async fn handle_client(client_id: &mut u32, client_streams_vec: Arc<Mutex<Vec<TcpStream>>>) {
    let mut buffer = [0u8; 1024];

    let mut streams = client_streams_vec.lock().await;
    let stream = &mut streams[*client_id as usize];
    let mut init_player: bool = false;

    loop {
        buffer.fill(0);
        let bytes_read = match stream.read(&mut buffer).await {
            Ok(bytes_read) => bytes_read,
            Err(error) => {
                eprintln!("Error reading from client {}: {}", client_id, error);
                break;
            }
        };

        if bytes_read == 0 {
            println!(
                "{} {} {}",
                "Client".red(),
                client_id,
                "has disconnected".red()
            );
            break;
        }

        let request = String::from_utf8_lossy(&buffer[..bytes_read]);
        println!(
            "{} {} {}",
            "Received from client".blue(),
            client_id,
            request
        );

        if request == "init".to_string() {
            if let Err(error) = stream
                .write_all(String::from("Init successfull").as_bytes())
                .await
            {
                eprintln!("Error writing response to client {}: {}", client_id, error);
                break;
            }

            init_player = true;
            break;
        }
    }

    if init_player == false {
        streams.remove(*client_id as usize);
        *client_id -= 1;
    }
}

async fn broadcast_msg(player_tcp_name: &mut HashMap<String, TcpStream>, cmd: &str) {
    for client_stream in player_tcp_name.values_mut() {
        if send_something(client_stream, &format!("{}", cmd)).await {
            continue;
        }
    }
}

async fn send_something(some_player: &mut TcpStream, cmd: &str) -> bool {
    let mut ret = false;

    if let Err(error) = some_player.write_all(cmd.as_bytes()).await {
        eprintln!("Error writing response to client: {}", error);
        ret = true;
    }
    if let Err(error) = some_player.flush().await {
        eprintln!("Error flushing response to client {}", error);
        ret = true;
    }

    ret
}

async fn recv_something(stream: &mut TcpStream) -> Option<String> {
    let mut buffer = [0; 1024];

    let response_received = Mutex::new(false);

    loop {
        tokio::select! {
            bytes_read = stream.read(&mut buffer) => {

                let bytes_read = bytes_read.unwrap();
                let request = String::from_utf8_lossy(&buffer[..bytes_read]);

                return Some(request.to_string());
            }
            _ = event_task(&response_received) => {
                let flag = *response_received.lock().await;
                if flag {
                    break None;
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let address = format!("{}:{}", args.addr, args.port);
    let required_clients = args.nplayers;

    let listener = TcpListener::bind(address.clone()).await.unwrap();
    println!("{} {}", "Server listening on".green(), address);

    let client_streams_vec = Arc::new(Mutex::new(Vec::new()));
    let client_counter = Arc::new(Semaphore::new(0));
    let mut client_handles = Vec::new();

    let mut client_id = 0;

    while let Ok((stream, _)) = listener.accept().await {
        client_streams_vec.lock().await.push(stream);

        client_counter.add_permits(1);

        // spawn a task to handle new client
        let cloned_streams_vec = Arc::clone(&client_streams_vec);
        let handle = tokio::spawn(async move {
            handle_client(&mut client_id, cloned_streams_vec).await;
        });
        client_handles.push(handle);

        client_id += 1;
        println!("{} {}", "current clients:".blue(), client_id);

        if client_id >= required_clients as u32 {
            for handle in client_handles {
                handle.await.unwrap();
            }

            println!(
                "{} {} {}",
                "Starting the game with".green(),
                required_clients,
                "clients!".green()
            );
            init_players(Arc::clone(&client_streams_vec)).await;
            break;
        }
    }

    Ok(())
}

async fn update_ui<B: Backend>(
    terminal: &mut Terminal<B>,
    the_game: &davincicode::Game,
) -> Result<(), std::io::Error> {
    terminal.draw(|f| ui2(f, the_game))?;
    Ok(())
}

async fn event_task(response_received: &Mutex<bool>) -> Result<(), std::io::Error> {
    loop {
        if poll(Duration::from_millis(500))? {
            if let Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => {
                        let mut stdout = io::stdout();
                        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

                        let _ = disable_raw_mode();
                        let backend = CrosstermBackend::new(stdout);
                        let mut terminal = Terminal::new(backend)?;

                        // restore terminal
                        disable_raw_mode()?;
                        execute!(
                            terminal.backend_mut(),
                            LeaveAlternateScreen,
                            DisableMouseCapture
                        )?;
                        terminal.show_cursor()?;
                        println!("exiting.");

                        process::exit(0);
                    }
                    _ => {
                        let mut flag = response_received.lock().await;
                        *flag = true;
                        break;
                    }
                }
            }
        } else {
            break;
        }
    }

    Ok(())
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    the_game: &mut davincicode::Game,
    selected_player_name: String,
    player_tcp_name: &mut HashMap<String, TcpStream>,
) -> Result<(), std::io::Error> {
    the_game
        .logs
        .push_str(format!("{}\n", "Game started").as_str());
    let _ = update_ui(terminal, the_game).await;

    loop {
        let _ = update_ui(terminal, &the_game).await;

        // send each player their own view of their deck
        for (name, client_stream) in player_tcp_name.iter_mut() {
            let mut to_send = String::new();
            to_send.push_str(format!("{}", "Your deck ##").as_str());

            let current_player = the_game
                .players
                .iter()
                .find(|player| player.name == name.to_string())
                .expect("No player found\n");

            to_send.push_str(&current_player.show_hand(false, false));
            to_send.push_str("##");

            if send_something(client_stream, &to_send).await {
                return Ok(());
            }
        }

        // at each turn, show each others card
        let mut ret = String::new();
        broadcast_msg(player_tcp_name, "\n").await;

        ret.push_str("\n");
        for player in the_game.players.iter() {
            ret.push_str(&player.name);
            ret.push_str("'s cards: ");
            ret.push_str(&player.show_hand(true, false));
        }

        broadcast_msg(player_tcp_name, &ret).await;

        // process cmd of all clients
        game_process(
            terminal,
            the_game,
            selected_player_name.clone(),
            player_tcp_name,
        )
        .await;
        println!("{}", "\n\nGame over\n\n".green());

        let mut ret = String::new();
        ret.push_str(format!("||{}|| {}", the_game.players[0].name, " won. __exiting__.").as_str());
        broadcast_msg(player_tcp_name, &ret).await;

        sleep(Duration::from_secs(15)).await;

        return Ok(());
    }
}
