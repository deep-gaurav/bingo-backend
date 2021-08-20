use crate::{data::Rank, logic::GamePlayer};

use self::bingo::{Bingo, BingoInputs, BingoPlayerData, BingoPlayerMessages, BingoStart};

use async_graphql::{Context, Object, ObjectType, Union};

use serde::Serialize;

pub mod bingo;

#[derive(Clone, Serialize, Union)]
pub enum Game {
    Bingo(Bingo),
}

pub enum PlayerMessages {
    BingoMessages(BingoPlayerMessages),
}

pub enum StartMessages {
    BingoStart(BingoStart),
}

impl PlayerMessages {
    pub fn as_bingo_messages(&self) -> Option<&BingoPlayerMessages> {
        if let Self::BingoMessages(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn try_into_bingo_messages(self) -> Result<BingoPlayerMessages, Self> {
        if let Self::BingoMessages(v) = self {
            Ok(v)
        } else {
            Err(self)
        }
    }
}

impl Game {
    pub fn as_bingo(&self) -> Option<&Bingo> {
        if let Self::Bingo(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

pub trait GameTrait
where
    Self: Sized,
{
    type PlayerMessage;
    type StartMessage;
    type PlayerGameData;
    type InputHandler: ObjectType;

    fn is_game_running(&self) -> bool;
    fn can_change_turn(&self, player_id: &str) -> bool;
    fn get_rankings(&self, players: &[GamePlayer]) -> Vec<Rank>;
    fn get_next_turn_player(&self, players: &[GamePlayer]) -> Option<String>;
    fn change_turn(&mut self, player_id: &str);
    fn handle_player_message(
        &mut self,
        player_id: &str,
        players: &mut [GamePlayer],
        message: Self::PlayerMessage,
    ) -> Result<(), anyhow::Error>;
    fn is_game_end(&self, players: &[GamePlayer]) -> bool;
    fn start_game(data: Self::StartMessage) -> (Self, Self::PlayerGameData);
    fn input_handler(room_id: String, player_id: String) -> Self::InputHandler;
}

impl GameTrait for Game {
    type PlayerMessage = PlayerMessages;
    type StartMessage = StartMessages;
    type PlayerGameData = PlayerGameData;
    type InputHandler = GameInputs;

    fn is_game_running(&self) -> bool {
        match self {
            Game::Bingo(b) => b.is_game_running(),
        }
    }

    fn can_change_turn(&self, player_id: &str) -> bool {
        match self {
            Game::Bingo(b) => b.can_change_turn(player_id),
        }
    }

    fn get_rankings(&self, players: &[GamePlayer]) -> Vec<Rank> {
        match self {
            Game::Bingo(b) => b.get_rankings(players),
        }
    }

    fn get_next_turn_player(&self, players: &[GamePlayer]) -> Option<String> {
        match self {
            Game::Bingo(b) => b.get_next_turn_player(players),
        }
    }

    fn change_turn(&mut self, player_id: &str) {
        match self {
            Game::Bingo(b) => b.change_turn(player_id),
        }
    }

    fn handle_player_message(
        &mut self,
        player_id: &str,
        players: &mut [GamePlayer],
        message: Self::PlayerMessage,
    ) -> std::result::Result<(), anyhow::Error> {
        match self {
            Game::Bingo(b) => {
                if let Ok(message) = message.try_into_bingo_messages() {
                    b.handle_player_message(player_id, players, message)
                } else {
                    Err(anyhow::anyhow!("Not Bingo message"))
                }
            }
        }
    }

    fn is_game_end(&self, players: &[GamePlayer]) -> bool {
        match self {
            Game::Bingo(b) => b.is_game_end(players),
        }
    }

    fn start_game(data: Self::StartMessage) -> (Self, Self::PlayerGameData) {
        match data {
            StartMessages::BingoStart(data) => {
                let bingostart = Bingo::start_game(data);
                (
                    Self::Bingo(bingostart.0),
                    PlayerGameData::BingoPlayerData(bingostart.1),
                )
            }
        }
    }

    fn input_handler(room_id: String, player_id: String) -> Self::InputHandler {
        GameInputs { room_id, player_id }
    }
}

#[derive(Clone, Serialize, Union)]
pub enum PlayerGameData {
    BingoPlayerData(BingoPlayerData),
}

impl PlayerGameData {
    pub fn as_bingo_player_data(&self) -> Option<&BingoPlayerData> {
        if let Self::BingoPlayerData(v) = self {
            Some(v)
        } else {
            None
        }
    }
    pub fn as_bingo_player_data_mut(&mut self) -> Option<&mut BingoPlayerData> {
        if let Self::BingoPlayerData(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

pub struct GameInputs {
    pub room_id: String,
    pub player_id: String,
}

#[Object]
impl GameInputs {
    pub async fn bingo_inputs<'ctx>(
        &self,
        ctx: &Context<'_>,
    ) -> Result<BingoInputs, async_graphql::Error> {
        Ok(BingoInputs {
            room_id: self.room_id.clone(),
            player_id: self.player_id.clone(),
        })
    }
}
