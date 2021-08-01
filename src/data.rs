use std::{collections::HashMap, sync::Arc};

use async_graphql::*;
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::{mpsc::Sender, RwLock};

use crate::logic::{GameData, GameEvents, GamePlayer};

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
                .map(|p| CommonPlayer::LobbyPlayer(p))
                .collect(),
            RoomState::Game(data) => data
                .players
                .iter()
                .cloned()
                .map(|p| CommonPlayer::GamePlayer(p))
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
                    Err(anyhow::anyhow!("Player already exists"))
                } else {
                    lobbydata.players.push(LobbyPlayer {
                        player,
                        send_channel: None,
                    });

                    Ok(())
                }
            }
            RoomState::Game(_) => Err(anyhow::anyhow!("Game already running")),
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

    pub async fn broadcast(&self, message: ServerResponse) {
        match self {
            RoomState::Lobby(data) => data.broadcast(message).await,
            RoomState::Game(data) => data.broadcast(message).await,
        }
    }
    pub fn remove_player(&mut self, player_id: &str) -> Result<(), anyhow::Error> {
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
}

#[derive(Debug, SimpleObject, Serialize, Clone)]
pub struct LobbyData {
    pub players: Vec<LobbyPlayer>,
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
        for p in self.get_player() {
            p.send(message.clone()).await;
        }
    }
}

#[async_trait]
impl ChannelPlayer for GamePlayer {
    fn get_channel(&self) -> Option<Sender<ServerResponse>> {
        self.send_channel.clone()
    }
}
#[async_trait]
impl ChannelPlayer for LobbyPlayer {
    fn get_channel(&self) -> Option<Sender<ServerResponse>> {
        self.send_channel.clone()
    }
}

#[async_trait]
trait ChannelPlayer {
    fn get_channel(&self) -> Option<Sender<ServerResponse>>;

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
#[derive(Serialize, Union, Clone)]
pub enum ServerResponse {
    PlayerJoined(PlayerJoined),
    PlayerConnected(PlayerConnected),
    PlayerLeft(PlayerLeft),

    GameMessage(GameMessage),
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct GameMessage {
    pub event: GameEvents,
    pub room: Room,
}