use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::{Context, Object, Subscription};
use futures::Stream;
use tokio::sync::mpsc::channel;
use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;

use crate::data::ChatMessage;
use crate::data::PlayerConnected;
use crate::data::PlayerJoined;
use crate::data::PlayerLeft;
use crate::data::PlayerRemoved;
use crate::data::RoomState;
use crate::data::ServerResponse;
use crate::games::Game;
use crate::games::GameInputs;
use crate::games::GameTrait;
use crate::{
    data::{Player, Room, Storage},
    utils::generate_rand_string,
};

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    pub async fn hello(&self) -> String {
        "hello world".to_string()
    }

    pub async fn game_event(&self, player_id: String, room_id: String) -> GameInputs {
        Game::input_handler(room_id, player_id)
    }

    pub async fn ping(&self) -> String {
        "pong".into()
    }

}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    pub async fn create_lobby<'ctx>(
        &self,
        ctx: &Context<'_>,
        player_id: String,
        player_name: String,
    ) -> Result<String, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let mut rooms = data.private_rooms.write().await;
        let room_id = generate_rand_string(6);
        if rooms.contains_key(&room_id) {
            Err("Cant create room".into())
        } else {
            rooms.insert(
                room_id.clone(),
                Room::new(
                    room_id.clone(),
                    Player {
                        id: player_id,
                        name: player_name,
                    },
                ),
            );
            Ok(room_id)
        }
    }

    pub async fn join_lobby<'ctx>(
        &self,
        ctx: &Context<'_>,
        player_id: String,
        player_name: String,
        room_id: String,
    ) -> Result<String, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let player = Player {
            id: player_id,
            name: player_name,
        };
        let room = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exist"))?;

            room.state.add_player(player.clone())?;
            room.clone()
        };

        room.clone()
            .state
            .broadcast(ServerResponse::PlayerJoined(PlayerJoined {
                player,

                room: room.clone(),
            }))
            .await;
        Ok(room_id)
    }

    pub async fn disconnect<'ctx>(
        &self,
        ctx: &Context<'_>,
        player_id: String,
        room_id: String,
    ) -> Result<String, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;

        let (room, player) = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exist"))?;

            let player = room.state.remove_player(&player_id)?;
            if let RoomState::Game(data) = &mut room.state {
                if data.game.can_change_turn(&player.id) {
                    data.change_turn();
                }
            }
            room.state.handle_game_end();

            (room.clone(), player)
        };

        room.clone()
            .state
            .broadcast(ServerResponse::PlayerRemoved(PlayerRemoved {
                player,

                room: room.clone(),
            }))
            .await;
        Ok("Disconnected".into())
    }


    pub async fn chat<'ctx>(
        &self,
        ctx: &Context<'_>,
        player_id: String,
        room_id: String,
        message:String,
    ) -> Result<String, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;

        let (room, player) = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exist"))?;
                
            let player = room.state.get_player(&player_id).ok_or("Player not in room")?.clone();
            (room.clone(), player)
        };

        room
            .state
            .broadcast(ServerResponse::ChatMessage(ChatMessage {
                player,
                message
            }))
            .await;
        Ok("Sucess".into())
    }
}

pub struct Subscription;

#[Subscription]
impl Subscription {
    async fn server_messages<'ctx>(
        &self,
        ctx: &Context<'_>,

        room_id: String,
        player_id: String,
    ) -> Result<impl Stream<Item = ServerResponse>, async_graphql::Error> {
        let (tx, rx) = channel::<ServerResponse>(2);

        let data = ctx.data::<Storage>()?;
        let room = {
            let mut rooms = data.private_rooms.write().await;
            let room = rooms
                .get_mut(&room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exist"))?;
            room.state.set_player_channel(player_id.clone(), tx)?;
            room.clone()
        };
        let player = room
            .state
            .get_player(&player_id)
            .ok_or("Player not found ")?
            .clone();
        room.clone()
            .state
            .broadcast(ServerResponse::PlayerConnected(PlayerConnected {
                player: player.clone(),

                room: room.clone(),
            }))
            .await;
        let player_dis = PlayerDisconnected {
            player,
            receiver_stream: rx,
            rooms: ctx.data::<Storage>()?.private_rooms.clone(),
            room_id,
        };
        Ok(player_dis)
    }
}

pub struct PlayerDisconnected {
    player: Player,
    receiver_stream: Receiver<ServerResponse>,
    rooms: Arc<RwLock<HashMap<String, Room>>>,
    room_id: String,
}

impl Drop for PlayerDisconnected {
    fn drop(&mut self) {
        let rooms = self.rooms.clone();
        let room_id = self.room_id.clone();
        let player = self.player.clone();
        tokio::spawn(async move {
            {
                log::info!("Taking room to remove player {:#?}", player);
                let mut rooms = rooms.write().await;
                log::info!("Removing player {:#?}", player);
                let mut remove = false;
                if let Some(room) = rooms.get_mut(&room_id) {
                    if let Err(er) = room.state.disconnect_player(&player.id) {
                        log::warn!("Could not remove player {:#?}", er)
                    } else {
                        log::info!("Player removed {:#?}", player);
                    }
                    if room.state.is_empty() {
                        remove = true;
                    } else {
                        log::info!("Sending broadcast PlayerLeft {:#?}", player);

                        log::info!("Updating Turn");

                        if let RoomState::Game(data) = &mut room.state {
                            if data.game.can_change_turn(&player.id) {
                                data.change_turn();
                            }
                        }

                        log::info!("Turn Updated")
                    }

                    room.state.handle_game_end();
                }
                if remove {
                    rooms.remove(&room_id);
                    log::info!("Deleting room {:#?}", room_id);
                }
            }
            {
                let rooms = rooms.read().await;
                if let Some(room) = rooms.get(&room_id) {
                    room.clone()
                        .state
                        .broadcast(ServerResponse::PlayerLeft(PlayerLeft {
                            player: player.clone(),
                            room: room.clone(),
                        }))
                        .await;
                }
            }
        });
    }
}

impl Stream for PlayerDisconnected {
    type Item = ServerResponse;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.receiver_stream.poll_recv(cx)
    }
}
