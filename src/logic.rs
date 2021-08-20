use async_graphql::*;

use serde::Serialize;

use crate::{
    data::{Player, Rank, Room, RoomState, ServerResponse},
    games::{Game, GameTrait, PlayerGameData, PlayerMessages, StartMessages},
};
use tokio::sync::mpsc::Sender;

#[derive(Serialize, Union, Clone)]
pub enum GameEvents {
    GameStarted(GameStarted),
    RoomUpdate(RoomUpdate),
}

#[derive(Serialize, SimpleObject, Clone)]
pub struct GameStarted {
    pub game: Game,
}

#[derive(Serialize, SimpleObject, Clone)]
pub struct RoomUpdate {
    pub room: Room,
}

pub enum PlayerEvents {
    StartGame(StartMessages),
    GameMessage(PlayerMessages),
}

#[derive(SimpleObject, Serialize, Clone)]
#[graphql(complex)]
pub struct GameData {
    pub players: Vec<GamePlayer>,
    pub game: Game,
}

#[ComplexObject]
impl GameData {
    pub async fn leaderboard(&self) -> Vec<Rank> {
        self.get_rankings()
    }
}

impl GameData {
    pub fn get_rankings(&self) -> Vec<Rank> {
        self.game.get_rankings(&self.players)
    }

    pub fn change_turn(&mut self) {
        if let Some(player_id) = self.game.get_next_turn_player(&self.players) {
            self.game.change_turn(&player_id);
        }
    }

    pub fn is_game_end(&self) -> bool {
        self.game.is_game_end(&self.players)
    }
}

#[derive(Serialize, SimpleObject, Clone)]
pub struct GamePlayer {
    pub player: Player,
    pub data: PlayerGameData,

    #[serde(skip_serializing)]
    #[graphql(skip)]
    pub send_channel: Option<Sender<ServerResponse>>,
}

///////////////////////////LOGIC////////////////////////////////

impl Room {
    pub async fn handle_player_message(
        &mut self,
        player_id: &str,
        player_message: PlayerEvents,
    ) -> Result<(), anyhow::Error> {
        match player_message {
            PlayerEvents::StartGame(start_message) => match &self.state {
                crate::data::RoomState::Lobby(data) => {
                    let (game, player_data) = Game::start_game(start_message);
                    let players = data
                        .players
                        .iter()
                        .cloned()
                        .map(|p| GamePlayer {
                            player: p.player,
                            data: player_data.clone(),
                            send_channel: p.send_channel,
                        })
                        .collect::<Vec<_>>();
                    self.state = RoomState::Game(GameData { players, game });
                }
                crate::data::RoomState::Game(_) => {
                    return Err(anyhow::anyhow!("Game Already Started"))
                }
            },
            PlayerEvents::GameMessage(message) => match &mut self.state {
                RoomState::Lobby(_) => return Err(anyhow::anyhow!("Game Not Started")),
                RoomState::Game(game) => {
                    game.game
                        .handle_player_message(player_id, &mut game.players, message)?
                }
            },
        }
        self.state.handle_game_end();
        Ok(())
    }
}
