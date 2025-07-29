use colored::Colorize;
use rand::{seq::SliceRandom, Rng};
use std::cmp::Ordering;

#[derive(Debug)]
pub struct Game {
    pub state: GameState,
    pub players: Vec<Player>,
    pub lost_players: Vec<Player>,
    pub winner: Option<Player>,
    pub card_avail: Vec<Card>,
    pub set_cards: u32,
    pub logs: String,
    pub err: String,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Player {
    pub name: String,
    pub ncards: u32,
    pub deck: Vec<Card>,
    pub status: PlayerStatus,
    pub side_card: Option<Card>,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Card {
    pub color: Color,
    pub value: u32,
    pub status: CardStatus,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum PlayerStatus {
    INIT,
    PLAYING,
    LOST,
    WON,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Color {
    BLACK,
    WHITE,
}
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CardStatus {
    HIDDEN,
    REVEALED,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum GameState {
    INIT,
    RUNNING,
    END,
}

///
///# Implementation of the Game struct
///
impl Game {
    pub fn new(set_number: u32) -> Game {
        let empty_players: Vec<Player> = Vec::new();
        let empty_cards: Vec<Card> = Vec::new();

        Game {
            state: GameState::INIT,
            players: empty_players.clone(),
            lost_players: empty_players.clone(),
            winner: None,
            card_avail: empty_cards,
            set_cards: set_number,
            logs: String::from(""),
            err: String::from(""),
        }
    }

    pub fn init_set(&mut self) {
        // creates the set of cards
        for c_val in 0..(self.set_cards) / 2 {
            let new_card_w = Card::new(c_val, Color::WHITE);
            let new_card_b = Card::new(c_val, Color::BLACK);

            self.card_avail.push(new_card_b);
            self.card_avail.push(new_card_w);
        }
        self.state = GameState::RUNNING;
        // println!("{:?}", self);

        // init the players
        self.init_players();
    }

    pub fn init_players(&mut self) {
        // check that players have different unames
        let mut players_name: Vec<String> = Vec::new();
        for p in self.players.iter() {
            players_name.push(p.name.clone());
        }
        let some =
            (1..players_name.len()).any(|i| players_name[i..].contains(&players_name[i - 1]));

        if some {
            panic!("Players should have different user names.");
        }

        // shuffle the deck here
        self.shuffle_avail_card();

        // init the game for the players internally
        for player in self.players.iter_mut() {
            player.init_game(&mut self.card_avail);
        }
    }

    pub fn shuffle_avail_card(&mut self) {
        let mut rng = rand::rng();
        self.card_avail.shuffle(&mut rng);
    }

    pub fn show_avail_cards(&self, hide_values: bool, colorize: bool) -> String {
        let mut game_set = String::new();

        for (elm_number, card) in self.card_avail.iter().enumerate() {
            game_set.push_str(&elm_number.to_string());
            game_set.push_str(": ");

            match card.color {
                Color::BLACK => {
                    if colorize {
                        game_set.push_str(&"B".blue());
                    } else {
                        game_set.push('B');
                    }

                    if hide_values {
                        match card.status {
                            CardStatus::HIDDEN => {
                                if colorize {
                                    game_set.push_str(&"?".blue());
                                } else {
                                    game_set.push('?');
                                }
                            }
                            CardStatus::REVEALED => {
                                if colorize {
                                    game_set.push_str(
                                        format!("{}", &card.value.to_string().blue()).as_str(),
                                    );
                                } else {
                                    game_set.push_str(&card.value.to_string());
                                }
                            }
                        };
                    } else if colorize {
                        game_set.push_str(&card.value.to_string().blue());
                    } else {
                        game_set.push_str(&card.value.to_string());
                    }
                }

                Color::WHITE => {
                    if colorize {
                        game_set.push_str(&"W".yellow());
                    } else {
                        game_set.push('W');
                    }

                    if hide_values {
                        match card.status {
                            CardStatus::HIDDEN => {
                                if colorize {
                                    game_set.push_str(&"?".yellow());
                                } else {
                                    game_set.push('?');
                                }
                            }
                            CardStatus::REVEALED => {
                                if colorize {
                                    game_set.push_str(
                                        format!("{}", &card.value.to_string().yellow()).as_str(),
                                    );
                                } else {
                                    game_set.push_str(&card.value.to_string());
                                }
                            }
                        };
                    } else if colorize {
                        game_set.push_str(&card.value.to_string().yellow());
                    } else {
                        game_set.push_str(&card.value.to_string());
                    }
                }
            };

            game_set.push_str(", ");
        }

        game_set
    }

    pub fn game_status(&mut self) -> bool {
        let mut to_remove: Vec<Player> = Vec::new();

        // check players to see if any has all the cards revealed
        for player in self.players.iter_mut() {
            // a given player: iterate through their deck
            let player_revealed_card = player
                .deck
                .iter()
                .filter(|card| card.status == CardStatus::REVEALED)
                .count();

            /*println!(
                "revealed player \"{}\" cards count: {}\n",
                player.name, player_revealed_card
            );*/

            if player_revealed_card as u32 == player.ncards {
                // player lost as all cards revealed
                player.status = PlayerStatus::LOST;
                self.lost_players.push(player.clone());
                to_remove.push(player.clone());
            }
        }

        // TODO: use retain
        // remove the players that lost from the vector
        for lost_player in to_remove.iter() {
            // println!("Removing {}\n", lost_player.name);

            let idx = self
                .players
                .iter_mut()
                .position(|player| player == lost_player)
                .expect("didn't find any?");
            self.players.remove(idx);
        }

        // remaining_players is just the current game players vec, since the ones that lost got
        // removed

        let mut game_ended: bool = false;

        if self.players.len() <= 1 {
            // only 1 player standing
            self.state = GameState::END;
            game_ended = true;

            // declare winner
            for p in self.players.iter() {
                if p.status == PlayerStatus::PLAYING {
                    self.winner = Some(p.clone());
                }
            }
        }

        game_ended
    }
}
///
///# Implementation of the Card struct
///
impl Card {
    pub fn new(value: u32, color: Color) -> Card {
        Card {
            color,
            value,
            status: CardStatus::HIDDEN,
        }
    }
}

///
///# Implementation of the Player struct
///
impl Player {
    pub fn new(name: String, ncards: u32) -> Player {
        let empty_vect: Vec<Card> = Vec::new();
        Player {
            name,
            ncards,
            deck: empty_vect,
            status: PlayerStatus::INIT,
            side_card: None,
        }
    }

    pub fn get_specific_card_value(&self, card_number: usize) -> Option<u32> {
        let picked_card = self.deck[card_number];
        if picked_card.status == CardStatus::REVEALED {
            // card were revaled,
            return None;
        }

        Some(picked_card.value)
    }

    pub fn deck_from_str(&mut self, deck_str: String) {
        //str_deck.push_str("0: W1, 1: B1, 2: W13, 3: B2");
        let card_items = deck_str.split(",");
        self.deck = Vec::new(); // reset current deck

        for card in card_items {
            let card: Vec<_> = card.split(":").collect();
            if let Some(card) = card.get(1) {
                if let Some(card_color) = card.get(0..2) {
                    if let Some(card_value) = card.get(2..) {
                        // println!("{} : {} => {}", card, card_color, card_value);
                        let card_color = match card_color.trim() {
                            "B" => Color::BLACK,
                            "W" => Color::WHITE,
                            _ => Color::BLACK,
                        };

                        if let Ok(value) = card_value.parse::<u32>() {
                            let a_card = Card::new(value, card_color);
                            self.deck.push(a_card);
                        }
                    }
                }
            }
        }
    }

    pub fn draw_specific_card(&mut self, avail_card: &mut Vec<Card>, card_number: usize) -> Card {
        let picked_card = avail_card[card_number];

        // remove the card from the set now
        let idx = avail_card
            .iter()
            .position(|card| (card.value == picked_card.value && card.color == picked_card.color))
            .unwrap();
        avail_card.remove(idx);

        // println!("Your picked {:?}, Setting it as side card.\n", picked_card);

        // set it as side card
        self.side_card = Some(picked_card);

        picked_card
    }

    pub fn draw_card(&mut self, avail_card: &mut Vec<Card>) {
        let mut rng = rand::rng();

        let avail_len: u32 = avail_card.len() as u32;
        let rand_pick: usize = rng.random_range(0..=avail_len - 1) as usize;

        // get the card
        let picked_card = avail_card[rand_pick];

        let idx = avail_card
            .iter()
            .position(|card| (card.value == picked_card.value && card.color == picked_card.color))
            .unwrap();
        avail_card.remove(idx);

        // set it as side card
        self.side_card = Some(picked_card);
    }

    pub fn draw_to_deck(&mut self, avail_card: &mut Vec<Card>) {
        let mut rng = rand::rng();

        let avail_len: u32 = avail_card.len() as u32;
        let rand_pick: usize = rng.random_range(0..=avail_len - 1) as usize;

        // get the card
        let picked_card = avail_card[rand_pick];

        let idx = avail_card
            .iter()
            .position(|card| (card.value == picked_card.value && card.color == picked_card.color))
            .unwrap();
        avail_card.remove(idx);

        self.deck.push(picked_card);
        // sort the deck
        self.sort_deck();
    }

    pub fn save_side_card(&mut self, hide_it: bool) {
        if hide_it {
            self.deck.push(self.side_card.unwrap());
        } else {
            // reveal and save
            let mut the_card = self.side_card.unwrap();
            the_card.status = CardStatus::REVEALED;

            self.deck.push(the_card);
        }
        self.ncards += 1;

        // sort the deck
        self.sort_deck();
    }

    pub fn show_hand(&self, opponent_view: bool, colorize: bool) -> String {
        let mut hand = String::new();

        for (elm_number, card) in self.deck.iter().enumerate() {
            hand.push_str(&elm_number.to_string());
            hand.push_str(": ");

            match card.color {
                Color::BLACK => {
                    if colorize {
                        hand.push_str(&"B".blue());
                    } else {
                        hand.push('B');
                    }
                    if opponent_view {
                        match card.status {
                            CardStatus::HIDDEN => {
                                if colorize {
                                    hand.push_str(&"?".blue());
                                } else {
                                    hand.push('?');
                                }
                            }
                            CardStatus::REVEALED => {
                                if colorize {
                                    hand.push_str(
                                        format!("{}", &card.value.to_string().blue()).as_str(),
                                    );
                                } else {
                                    hand.push_str(&card.value.to_string());
                                }
                            }
                        };
                    } else if colorize {
                        hand.push_str(&card.value.to_string().blue());
                    } else {
                        hand.push_str(&card.value.to_string());
                    }
                }

                Color::WHITE => {
                    if colorize {
                        hand.push_str(&"W".yellow());
                    } else {
                        hand.push('W');
                    }

                    if opponent_view {
                        match card.status {
                            CardStatus::HIDDEN => {
                                if colorize {
                                    hand.push_str(&"?".yellow());
                                } else {
                                    hand.push('?');
                                }
                            }
                            CardStatus::REVEALED => {
                                if colorize {
                                    hand.push_str(
                                        format!("{}", &card.value.to_string().yellow()).as_str(),
                                    );
                                } else {
                                    hand.push_str(&card.value.to_string());
                                }
                            }
                        };
                    } else if colorize {
                        hand.push_str(&card.value.to_string().yellow());
                    } else {
                        hand.push_str(&card.value.to_string());
                    }
                }
            };
            hand.push_str(", ");
        }
        // hand.push('\n');

        hand
    }

    fn sort_deck(&mut self) {
        // sort here by value and color
        self.deck.sort_by(|a, b| {
            if a.value != b.value {
                // sort by value in ascending order
                return a.value.cmp(&b.value);
            }

            // sort by color (Black first, then White)
            match (a.color, b.color) {
                (Color::BLACK, Color::WHITE) => Ordering::Less,
                (Color::WHITE, Color::BLACK) => Ordering::Greater,
                _ => Ordering::Equal,
            }
        });
    }

    pub fn init_game(&mut self, avail_card: &mut Vec<Card>) {
        for _ in 1..self.ncards + 1 {
            self.draw_to_deck(avail_card);
            // drop side card
            self.side_card = None;
        }
        self.status = PlayerStatus::PLAYING;

        // println!("{:?}\n", self);
    }

    pub fn reveal_card(&mut self, card_idx: usize) -> u32 {
        // reveal card
        let ret;

        if self.deck[card_idx].status == CardStatus::REVEALED {
            ret = 1; // card already revealed
        } else {
            self.deck[card_idx].status = CardStatus::REVEALED;
            ret = 0;
        }

        ret
    }

    pub fn reveal_card_2(&mut self, card_to_reveal: &Card) {
        // reveal card
        let idx = self
            .deck
            .iter()
            .position(|card| {
                card.value == card_to_reveal.value && card.color == card_to_reveal.color
            })
            .unwrap();

        self.deck[idx].status = CardStatus::REVEALED;
    }
}

///
///# Testing
///
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn test_uniq_unames() {
        const START_CARD_N: u32 = 24;

        let mut game = Game::new(START_CARD_N);

        let p1 = Player::new(String::from("me"), 4);
        let p2 = Player::new(String::from("me"), 4);

        game.players.push(p1);
        game.players.push(p2);

        game.init_set();
    }

    #[test]
    fn test_deck_from_str() {
        let mut p1 = Player::new(String::from("me"), 4);
        let mut str_deck = String::new();

        str_deck.push_str("0: W1, 1: B1, 2: W13, 3: B2");

        let c1 = Card::new(1, crate::Color::WHITE);
        let c2 = Card::new(1, crate::Color::BLACK);
        let c3 = Card::new(13, crate::Color::WHITE);
        let c4 = Card::new(2, crate::Color::BLACK);

        p1.deck_from_str(str_deck);

        let expected_deck: Vec<Card> = vec![c1, c2, c3, c4];

        assert_eq!(p1.deck, expected_deck);
    }

    #[test]
    fn test_set_shuffle() {
        const START_CARD_N: u32 = 24;

        let mut game = Game::new(START_CARD_N);

        let p1 = Player::new(String::from("me"), 4);
        let p2 = Player::new(String::from("me2"), 4);

        game.players.push(p1);
        game.players.push(p2);

        game.init_set();

        let prev_set_state = game.card_avail.clone();
        game.shuffle_avail_card();

        assert_ne!(game.card_avail, prev_set_state);
    }

    #[test]
    fn test_remove_lost_player() {
        const START_CARD_N: u32 = 24;

        let mut game = Game::new(START_CARD_N);

        let p1 = Player::new(String::from("me"), 4);
        let p2 = Player::new(String::from("me1"), 4);
        let mut p3 = Player::new(String::from("me2"), 4);

        p3.status = PlayerStatus::PLAYING;

        game.players.push(p1);
        game.players.push(p2);
        game.players.push(p3.clone());

        game.init_set();

        // make player 3 lose aka all cards revealed
        for p in game.players.iter_mut() {
            if p.clone().name == p3.name {
                // should have check the == of players, but I have to
                // clone the deck before checking this...
                for card in p.deck.iter_mut() {
                    card.status = CardStatus::REVEALED;
                }
            }
        }

        game.game_status(); // checking this status should eliminate player 3

        assert_eq!(game.state, GameState::RUNNING);
        assert_eq!(game.players.len(), 2);
    }

    #[test]
    fn test_game_winner() {
        const START_CARD_N: u32 = 24;

        let mut game = Game::new(START_CARD_N);

        let p1 = Player::new(String::from("me"), 4);
        let p2 = Player::new(String::from("me1"), 4);
        let p3 = Player::new(String::from("me2"), 4);

        game.players.push(p1.clone());
        game.players.push(p2.clone());
        game.players.push(p3.clone());

        game.init_set();

        assert_eq!(game.state, GameState::RUNNING);
        assert_eq!(game.players.len(), 3);

        // make player 3 & 2 lose aka all cards revealed
        for p in game.players.iter_mut() {
            if (p.clone().name == p2.name) || (p.clone().name == p3.name) {
                // should have check the == of players, but I have to
                // clone the deck before checking this...
                for card in p.deck.iter_mut() {
                    card.status = CardStatus::REVEALED;
                }
            }
        }
        game.game_status(); // checking this status should eliminate player 3 and 2, so only one
                            // remaining so declare the winner

        assert_eq!(game.winner.unwrap().name, p1.name);
    }

    #[test]
    fn test_internal_init() {
        const START_CARD_N: u32 = 24;

        let mut game = Game::new(START_CARD_N);

        let p1 = Player::new(String::from("me"), 4);
        let p2 = Player::new(String::from("me1"), 4);

        game.players.push(p1);
        game.players.push(p2);

        game.init_set();

        assert_eq!(game.state, GameState::RUNNING);
        assert_eq!(game.players.len(), 2);

        // check if players init is fine
        for player in game.players.iter() {
            assert_eq!(player.deck.len() as u32, player.ncards);
        }
    }

    #[test]
    fn test_game_status_running() {
        const START_CARD_N: u32 = 24;

        let mut game = Game::new(START_CARD_N);
        game.init_set();

        assert_eq!(game.state, GameState::RUNNING);
    }

    #[test]
    fn test_sort_deck() {
        let mut p1 = Player::new(String::from("me"), 4);

        let c1 = Card::new(1, crate::Color::WHITE);
        let c2 = Card::new(1, crate::Color::BLACK);
        let c3 = Card::new(13, crate::Color::WHITE);
        let c4 = Card::new(2, crate::Color::BLACK);

        p1.deck.push(c1);
        p1.deck.push(c2);
        p1.deck.push(c3);
        p1.deck.push(c4);

        println!("\n{:?}\n", p1.deck);
        p1.sort_deck();
        println!("{:?}\n", p1.deck);

        let expected_deck: Vec<Card> = vec![c2, c1, c4, c3];
        assert_eq!(p1.deck, expected_deck);
    }

    #[test]
    fn test_game_end_card_revealed_all() {
        const START_CARD_N: u32 = 24;

        let mut game = Game::new(START_CARD_N);
        game.init_set();

        for i in 1..=4 {
            let mut name = String::new();
            name.push_str("me");
            name.push_str(&i.to_string());
            let mut tmp_p = Player::new(name, 4);

            tmp_p.init_game(&mut game.card_avail);
        }

        for player in &mut game.players {
            for card in 0..player.deck.len() {
                player.reveal_card(card);
            }
        }

        for player in &game.players {
            for card in &player.deck {
                assert_eq!(card.status, CardStatus::REVEALED);
            }
        }

        game.game_status();

        assert_eq!(START_CARD_N - 16, game.card_avail.len() as u32);
        assert_eq!(game.state, GameState::END);
    }

    #[test]
    fn test_init_payer_deck() {
        const START_CARD_N: u32 = 24;

        let mut game = Game::new(START_CARD_N);
        game.init_set();

        let mut p1 = Player::new(String::from("me"), 4);
        p1.init_game(&mut game.card_avail);

        println!("{:?}", p1.deck);
        assert_eq!(p1.ncards, p1.deck.len() as u32);

        // avail card reduced
        assert_ne!(START_CARD_N, game.card_avail.len() as u32);
        assert_eq!(START_CARD_N, game.card_avail.len() as u32 + 4);
    }

    #[test]
    fn test_reduced_game_set() {
        const START_CARD_N: u32 = 24;

        let mut game = Game::new(START_CARD_N);
        game.init_set();

        let mut p1 = Player::new(String::from("me"), 4);
        p1.init_game(&mut game.card_avail);

        println!("avail_len: {}\n", game.card_avail.len());
        // avail card reduced
        assert_eq!(START_CARD_N, game.card_avail.len() as u32 + 4);
    }
}
