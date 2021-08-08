use std::collections::HashSet;

use async_graphql::*;
use ndarray::{Array2, Axis};
use serde::Serialize;

use crate::data::{GameMessage, Player, Room, RoomState, ServerResponse, Storage};
use tokio::sync::mpsc::Sender;

#[derive(Serialize, Union, Clone)]
pub enum GameEvents {
    GameStarted(GameStarted),
    RoomUpdate(RoomUpdate),
}

#[derive(Serialize, SimpleObject, Clone)]
pub struct GameStarted {
    game_state: GameState,
}

#[derive(Serialize, SimpleObject, Clone)]
pub struct RoomUpdate {
    pub room: Room,
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
        let room = {
            let data = ctx.data::<Storage>()?;

            let mut rooms = data.private_rooms.write().await;
            let room = rooms
                .get_mut(&self.room_id)
                .ok_or(async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(&self.player_id, PlayerEvents::StartGame(board_size))
                .await?;
            room.clone()
        };

        room.clone()
            .state
            .broadcast(ServerResponse::GameMessage(GameMessage {
                event: GameEvents::GameStarted(GameStarted {
                    game_state: room.state.as_game().ok_or("Not game")?.game_state.clone(),
                }),
                room: room.clone(),
            }))
            .await;
        Ok(true)
    }

    pub async fn ready_board<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        board: Vec<Vec<u32>>,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let room = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&self.room_id)
                .ok_or(async_graphql::Error::from("Room does not exis"))?;
            let board_size = room
                .state
                .as_game()
                .ok_or(async_graphql::Error::from("Game not running"))?
                .board_size;

            room.handle_player_message(
                &self.player_id,
                PlayerEvents::ReadyBoard(Board::new(board, board_size)?),
            )
            .await?;
            room.clone()
        };

        room.clone()
            .state
            .broadcast(ServerResponse::GameMessage(GameMessage {
                event: GameEvents::RoomUpdate(RoomUpdate { room: room.clone() }),
                room: room.clone(),
            }))
            .await;
        Ok(true)
    }

    pub async fn player_move<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        number: u32,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let room = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&self.room_id)
                .ok_or(async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(&self.player_id, PlayerEvents::Move(number))
                .await?;
            room.clone()
        };

        room.clone()
            .state
            .broadcast(ServerResponse::GameMessage(GameMessage {
                event: GameEvents::RoomUpdate(RoomUpdate { room: room.clone() }),
                room: room.clone(),
            }))
            .await;
        Ok(true)
    }
}

#[derive(Serialize, SimpleObject, Clone)]
#[graphql(complex)]

pub struct Board {
    pub numbers: Vec<Vec<Cell>>,
}

#[ComplexObject]
impl Board {
    pub async fn score<'ctx>(
        &self,
        ctx: &Context<'ctx>,
        room_id: String,
    ) -> Result<u32, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        log::info!("Trying to Get read for room board");
        let rooms = data.private_rooms.read().await;
        log::info!("Got read for room board");
        let room = rooms.get(&room_id).ok_or("Room Not found")?;
        let state = &room
            .state
            .as_game()
            .ok_or("Not game")?
            .game_state;
        match state{
            GameState::BoardCreation(_) => Ok(0),
            GameState::GameRunning(state) => Ok(
                self.get_score(&state.selected_numbers)
            ),
        }
    }
}

impl Board {
    pub fn new(numbers: Vec<Vec<Cell>>, board_size: u16) -> Result<Self, anyhow::Error> {
        let all_num = numbers
            .clone()
            .join(&[][..])
            .into_iter()
            .collect::<HashSet<_>>();
        if all_num.len() == (board_size * board_size) as usize {
            if all_num.iter().min().unwrap_or(&0_u32) < &1_u32
                || all_num
                    .iter()
                    .max()
                    .unwrap_or(&((board_size * board_size + 1) as u32))
                    > &((board_size * board_size) as u32)
            {
                Err(anyhow::anyhow!("Invalid value of board"))
            } else {
                Ok(Self { numbers })
            }
        } else {
            Err(anyhow::anyhow!("Invalid Board"))
        }
    }

    pub fn get_score(&self, selected_cells: &Vec<SelectedCell>) -> u32 {
        let mut ndarr = Array2::<u32>::default((self.numbers.len(), self.numbers.len()));
        let n = self.numbers.len();
        for (i, mut row) in ndarr.axis_iter_mut(Axis(0)).enumerate() {
            for (j, col) in row.iter_mut().enumerate() {
                let val = self.numbers[i][j];
                if selected_cells.iter().any(|cell| cell.cell_value == val) {
                    *col = 0;
                } else {
                    *col = val;
                }
            }
        }
        let mut points = 0;
        for row in ndarr.rows().into_iter().chain(ndarr.columns()) {
            if !row.iter().any(|val| *val != 0) {
                points += 1;
            }
        }

        let d1 = (0..n).zip(0..n);
        let d2 = (0..n).zip((0..n).rev());
        if !d1
            .into_iter()
            .chain(d2)
            .any(|i| ndarr.get(i).unwrap_or(&0) != &0)
        {
            points += 1;
        }
        points
    }

    pub fn wining_points(&self) -> u32 {
        self.numbers.len() as u32
    }

    pub fn max_points(&self) -> u32 {
        (self.numbers.len() * 2 + 2) as u32
    }
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

impl GameState {
    pub fn as_game_running(&self) -> Option<&GameRunning> {
        if let Self::GameRunning(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct BoardCreation {
    ready: Vec<String>,
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct GameRunning {
    pub turn: String,
    pub selected_numbers: Vec<SelectedCell>,
}

#[derive(SimpleObject, Serialize, Clone)]
pub struct GameData {
    pub players: Vec<GamePlayer>,
    pub board_size: u16,
    pub game_state: GameState,
}

impl GameData {
    pub fn change_turn(&mut self) {
        if !self.players.iter().any(|p| p.send_channel.is_some()) {
            return;
        }
        if let GameState::GameRunning(data) = &mut self.game_state {
            let mut cycle_iter = self.players.iter().cycle();
            let current_player_position =
                self.players.iter().position(|p| p.player.id == data.turn);
            if let Some(position) = current_player_position {
                cycle_iter.nth(position);
                while let Some(player) = cycle_iter.next() {
                    if player.send_channel.is_some() {
                        data.turn = player.player.id.to_string();
                        break;
                    }
                }
            }
        }
    }
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
            PlayerEvents::StartGame(board_size) => match &self.state {
                crate::data::RoomState::Lobby(data) => {
                    let players = data
                        .players
                        .iter()
                        .cloned()
                        .map(|p| GamePlayer {
                            player: p.player,
                            board: None,
                            send_channel: p.send_channel,
                        })
                        .collect::<Vec<_>>();
                    let game_state = GameState::BoardCreation(BoardCreation { ready: vec![] });
                    self.state = RoomState::Game(GameData {
                        players,
                        board_size,
                        game_state: game_state.clone(),
                    });
                }
                crate::data::RoomState::Game(_) => Err(anyhow::anyhow!("Game Already Started"))?,
            },
            PlayerEvents::ReadyBoard(board) => match &mut self.state {
                RoomState::Lobby(_) => Err(anyhow::anyhow!("Game Not Started"))?,
                RoomState::Game(data) => match &mut data.game_state {
                    GameState::BoardCreation(board_creation) => {
                        if board_creation.ready.contains(&player_id.to_string()) {
                            Err(anyhow::anyhow!("Board already set"))?;
                        } else {
                            if let Some(player) =
                                data.players.iter_mut().find(|p| p.player.id == player_id)
                            {
                                player.board = Some(board);

                                board_creation.ready.push(player_id.into());
                                if board_creation.ready.len() == data.players.len() {
                                    data.game_state = GameState::GameRunning(GameRunning {
                                        turn: data
                                            .players
                                            .first()
                                            .ok_or(anyhow::anyhow!("No player"))?
                                            .player
                                            .id
                                            .clone(),
                                        selected_numbers: vec![],
                                    });
                                }
                            } else {
                                Err(anyhow::anyhow!("Player not found"))?;
                            }
                        }
                    }
                    GameState::GameRunning(_) => Err(anyhow::anyhow!("Game Already Running"))?,
                },
            },
            PlayerEvents::Move(mov) => match &mut self.state {
                RoomState::Lobby(_) => Err(anyhow::anyhow!("Game Not Started"))?,
                RoomState::Game(data) => match &mut data.game_state {
                    GameState::BoardCreation(_) => Err(anyhow::anyhow!("Game Not Running"))?,
                    GameState::GameRunning(running_data) => {
                        if &running_data.turn == player_id {
                            if running_data
                                .selected_numbers
                                .iter()
                                .any(|c| c.cell_value == mov)
                            {
                                Err(anyhow::anyhow!("Invalid move"))?;
                            } else {
                                running_data.selected_numbers.push(SelectedCell {
                                    selected_by: player_id.into(),
                                    cell_value: mov,
                                });
                                data.change_turn();
                            }
                        } else {
                            Err(anyhow::anyhow!("Not your turn"))?;
                        }
                    }
                },
            },
        }
        Ok(())
    }
}
