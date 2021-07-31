use async_graphql::*;
use serde::Serialize;

use crate::data::{Player, Room, ServerResponse, Storage};
use tokio::sync::mpsc::Sender;

#[derive(Serialize, Union, Clone)]
pub enum GameEvents {
    GameStarted(GameStated),
    RoomUpdate(Room),
}

#[derive(Serialize, SimpleObject, Clone)]
pub struct GameStated {
    game_state: GameState,
}

pub struct PlayerHandler {
    pub room_id: String,
    pub player_id: String,
}

pub enum PlayerEvents {
    StartGame(u16),
    ReadyBoard(Board),
    Move(Cell),
}

#[Object]
impl PlayerHandler {
    pub async fn start_game<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        board_size: u16,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let mut rooms = data.private_rooms.write().await;
        let room = rooms
            .get_mut(&self.room_id)
            .ok_or(async_graphql::Error::from("Room does not exis"))?;
        room.handle_player_message(&self.player_id, PlayerEvents::StartGame(board_size))
            .await?;
        Ok(true)
    }

    pub async fn ready_board<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        board: Vec<Vec<u32>>,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let mut rooms = data.private_rooms.write().await;
        let room = rooms
            .get_mut(&self.room_id)
            .ok_or(async_graphql::Error::from("Room does not exis"))?;
        room.handle_player_message(
            &self.player_id,
            PlayerEvents::ReadyBoard(Board { numbers: board }),
        )
        .await?;
        Ok(true)
    }

    pub async fn player_move<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        number: u32,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let mut rooms = data.private_rooms.write().await;
        let room = rooms
            .get_mut(&self.room_id)
            .ok_or(async_graphql::Error::from("Room does not exis"))?;
        room.handle_player_message(&self.player_id, PlayerEvents::Move(number))
            .await?;
        Ok(true)
    }
}

#[derive(Serialize, SimpleObject, Clone)]
pub struct Board {
    pub numbers: Vec<Vec<Cell>>,
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct SelectedCell {
    cell_value: u32,
    selected_by: String,
}
pub type Cell = u32;

#[derive(Serialize, Union, Clone)]
pub enum GameState {
    BoardCreation(BoardCreation),
    GameRunning(GameRunning),
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct BoardCreation {
    ready: Vec<String>,
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct GameRunning {
    turn: String,
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct GameData {
    pub players: Vec<GamePlayer>,
    pub board_size: u16,
    pub selected_numbers: Vec<SelectedCell>,
    pub game_state: GameState,
}
#[derive(Serialize, SimpleObject, Clone)]
pub struct GamePlayer {
    pub player: Player,
    pub board: Option<Board>,

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
            PlayerEvents::StartGame(board_size) => todo!(),
            PlayerEvents::ReadyBoard(_) => todo!(),
            PlayerEvents::Move(_) => todo!(),
        }
        Ok(())
    }
}
