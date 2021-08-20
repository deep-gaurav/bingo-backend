use std::{collections::HashMap, sync::Arc};

use async_graphql::*;
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::{mpsc::Sender, RwLock};

use crate::{
    games::GameTrait,
    logic::{GameData, GameEvents, GamePlayer},
};

#[derive(Default)]
pub struct Storage {
    pub private_rooms: Arc<RwLock<HashMap<String, Room>>>,
}

#[derive(Serialize, SimpleObject, Clone)]
#[graphql(complex)]
pub struct Room {
    id: String,
    pub state: RoomState,
}

#[ComplexObject]
impl Room {
    pub async fn players(&self) -> Vec<CommonPlayer> {
        match &self.state {
            RoomState::Lobby(data) => data
                .players
                .iter()
                .cloned()
                .map(CommonPlayer::LobbyPlayer)
                .collect(),
            RoomState::Game(data) => data
                .players
                .iter()
                .cloned()
                .map(CommonPlayer::GamePlayer)
                .collect(),
        }
    }
}

impl Room {
    pub fn new(id: String, player: Player) -> Self {
        Self {
            id,
            state: RoomState::Lobby(LobbyData {
                players: vec![LobbyPlayer {
                    player,
                    send_channel: None,
                }],
                last_game: None,
            }),
        }
    }
}

#[derive(Union, Serialize, Clone)]
pub enum RoomState {
    Lobby(LobbyData),
    Game(GameData),
}

impl RoomState {
    pub fn add_player(&mut self, player: Player) -> Result<(), anyhow::Error> {
        match self {
            RoomState::Lobby(lobbydata) => {
                if lobbydata.players.iter().any(|p| p.player.id == player.id) {
                    Ok(())
                } else {
                    lobbydata.players.push(LobbyPlayer {
                        player,
                        send_channel: None,
                    });

                    Ok(())
                }
            }
            RoomState::Game(data) => {
                if data.players.iter().any(|p| p.player.id == player.id) {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Game already running"))
                }
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        match self {
            RoomState::Lobby(data) => !data.players.iter().any(|p| p.send_channel.is_some()),
            RoomState::Game(data) => !data.players.iter().any(|p| p.send_channel.is_some()),
        }
    }

    pub fn set_player_channel(
        &mut self,
        player_id: String,
        channel: Sender<ServerResponse>,
    ) -> Result<(), anyhow::Error> {
        match self {
            RoomState::Lobby(data) => {
                let pl = data.players.iter_mut().find(|p| p.player.id == player_id);
                if let Some(pl) = pl {
                    pl.send_channel = Some(channel);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Player does not exist"))
                }
            }
            RoomState::Game(data) => {
                let pl = data.players.iter_mut().find(|p| p.player.id == player_id);
                if let Some(pl) = pl {
                    pl.send_channel = Some(channel);
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Player does not exist"))
                }
            }
        }
    }

    pub fn get_player(&self, player_id: &str) -> Option<&Player> {
        match self {
            RoomState::Lobby(data) => data
                .players
                .iter()
                .find(|p| p.player.id == player_id)
                .map(|lp| &lp.player),
            RoomState::Game(data) => data
                .players
                .iter()
                .find(|p| p.player.id == player_id)
                .map(|lp| &lp.player),
        }
    }

    pub async fn broadcast(self, message: ServerResponse) {
        match self {
            RoomState::Lobby(data) => data.broadcast(message).await,
            RoomState::Game(data) => data.broadcast(message).await,
        }
    }
    pub fn disconnect_player(&mut self, player_id: &str) -> Result<(), anyhow::Error> {
        log::info!("Removing player {}", player_id);
        match self {
            RoomState::Lobby(data) => {
                if let Some(player) = data.players.iter_mut().find(|p| p.player.id == player_id) {
                    player.send_channel = None;
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Player does not exist"))
                }
            }
            RoomState::Game(data) => {
                if let Some(player) = data.players.iter_mut().find(|p| p.player.id == player_id) {
                    player.send_channel = None;
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Player does not exist"))
                }
            }
        }
    }

    pub fn as_game(&self) -> Option<&GameData> {
        if let Self::Game(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn handle_game_end(&mut self) {
        if let Self::Game(data) = self {
            if data.is_game_end() {
                let lobby_player = data
                    .players
                    .iter()
                    .cloned()
                    .map(|p| LobbyPlayer {
                        player: p.player,
                        send_channel: p.send_channel,
                    })
                    .collect();
                data.players.iter_mut().for_each(|p| p.send_channel = None);

                *self = Self::Lobby(LobbyData {
                    players: lobby_player,
                    last_game: {
                        if data.game.is_game_running() {
                            Some(LastGame {
                                last_game: data.clone(),
                                leader_board: data.get_rankings(),
                            })
                        } else {
                            None
                        }
                    },
                })
            }
        }
    }

    pub fn remove_player(&mut self, player_id: &str) -> Result<Player, anyhow::Error> {
        log::info!("Removing player {}", player_id);
        match self {
            RoomState::Lobby(data) => {
                let p_index = data
                    .players
                    .iter()
                    .position(|p| p.player.id == player_id)
                    .ok_or_else(|| anyhow::anyhow!("Player doesnt exist"))?;
                let player = data.players.remove(p_index);
                Ok(player.player)
            }
            RoomState::Game(data) => {
                let p_index = data
                    .players
                    .iter()
                    .position(|p| p.player.id == player_id)
                    .ok_or_else(|| anyhow::anyhow!("Player doesnt exist"))?;
                let player = data.players.remove(p_index);
                Ok(player.player)
            }
        }
    }
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct LobbyData {
    pub players: Vec<LobbyPlayer>,
    pub last_game: Option<LastGame>,
}
#[derive(SimpleObject, Serialize, Clone)]
pub struct LastGame {
    last_game: GameData,
    leader_board: Vec<Rank>,
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct Rank {
    pub rank: u32,
    pub player: Player,
}

#[derive(Debug, SimpleObject, Serialize, Clone)]
pub struct LobbyPlayer {
    pub player: Player,

    #[serde(skip_serializing)]
    #[graphql(skip)]
    pub send_channel: Option<Sender<ServerResponse>>,
}

impl BroadcastPlayers<GamePlayer> for GameData {
    fn get_player(&self) -> &Vec<GamePlayer> {
        &self.players
    }
}
impl BroadcastPlayers<LobbyPlayer> for LobbyData {
    fn get_player(&self) -> &Vec<LobbyPlayer> {
        &self.players
    }
}

#[async_trait]
trait BroadcastPlayers<T: ChannelPlayer + Send + Sync> {
    fn get_player(&self) -> &Vec<T>;

    async fn broadcast(&self, message: ServerResponse) {
        let futures = self.get_player().iter().map(|f| f.send(message.clone()));
        futures::future::join_all(futures).await;
    }
}

#[async_trait]
impl ChannelPlayer for GamePlayer {
    fn get_channel(&self) -> &Option<Sender<ServerResponse>> {
        &self.send_channel
    }
}
#[async_trait]
impl ChannelPlayer for LobbyPlayer {
    fn get_channel(&self) -> &Option<Sender<ServerResponse>> {
        &self.send_channel
    }
}

#[async_trait]
trait ChannelPlayer {
    fn get_channel(&self) -> &Option<Sender<ServerResponse>>;

    async fn send(&self, message: ServerResponse) {
        match self.get_channel() {
            Some(channel) => match channel.send(message).await {
                Ok(_) => {}
                Err(_er) => {
                    log::warn!("ERROR SENDING ")
                }
            },
            None => {}
        }
    }

    fn has_channel(&self) -> bool {
        self.get_channel().is_some()
    }

    async fn is_connected<'ctx>(&self, _ctx: &Context<'ctx>) -> Result<bool, async_graphql::Error> {
        Ok(self.get_channel().is_some())
    }
}

#[derive(Interface)]
#[graphql(field(name = "is_connected", type = "bool"))]
pub enum CommonPlayer {
    GamePlayer(GamePlayer),
    LobbyPlayer(LobbyPlayer),
}

#[derive(SimpleObject, Serialize, Clone, Debug)]
pub struct Player {
    pub id: String,
    pub name: String,
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct PlayerJoined {
    pub player: Player,
    pub room: Room,
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct PlayerLeft {
    pub player: Player,
    pub room: Room,
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct PlayerConnected {
    pub player: Player,
    pub room: Room,
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct PlayerRemoved {
    pub player: Player,
    pub room: Room,
}

#[derive(Serialize, Union, Clone)]
pub enum ServerResponse {
    PlayerJoined(PlayerJoined),
    PlayerConnected(PlayerConnected),
    PlayerLeft(PlayerLeft),
    PlayerRemoved(PlayerRemoved),

    GameMessage(GameMessage),
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct GameMessage {
    pub event: GameEvents,
    pub room: Room,
}
