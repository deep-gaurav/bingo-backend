use std::collections::HashSet;

use async_graphql::{ComplexObject, Context, Object, Result, SimpleObject, Union};

use ndarray::{Array2, Axis};
use serde::Serialize;

use crate::{
    data::{GameMessage, Rank, ServerResponse, Storage},
    logic::{GameEvents, GamePlayer, GameStarted, PlayerEvents, RoomUpdate},
};

use super::{GameTrait, PlayerMessages, StartMessages};

#[derive(Clone, Serialize, SimpleObject)]
pub struct Bingo {
    pub game_state: GameState,
    pub board_size: u16,
}

pub enum BingoPlayerMessages {
    ReadyBoard(Board),
    Move(Cell),
}

pub struct BingoStart {
    pub board_size: u16,
}

impl GameTrait for Bingo {
    type PlayerMessage = BingoPlayerMessages;
    type StartMessage = BingoStart;
    type PlayerGameData = BingoPlayerData;
    type InputHandler = BingoInputs;

    fn is_game_running(&self) -> bool {
        self.game_state.is_game_running()
    }
    fn can_change_turn(&self, player_id: &str) -> bool {
        match &self.game_state {
            GameState::BoardCreation(_) => false,
            GameState::GameRunning(data) => data.turn == player_id,
        }
    }

    fn get_rankings(&self, players: &[GamePlayer]) -> Vec<crate::data::Rank> {
        match &self.game_state {
            GameState::BoardCreation(_) => vec![],
            GameState::GameRunning(data) => {
                let mut player_turn = players.iter().map(|p| (0, 0, p)).collect::<Vec<_>>();
                for l in 0..data.selected_numbers.len() {
                    let temp_selected_cells = &data.selected_numbers[0..l + 1];
                    players.iter().enumerate().for_each(|(player_index, p)| {
                        let score = p
                            .data
                            .as_bingo_player_data()
                            .and_then(|b| b.board.as_ref())
                            .map(|board| board.get_score(temp_selected_cells))
                            .unwrap_or(0)
                            .clamp(0, self.board_size as u32);
                        if player_turn[player_index].0 < score {
                            player_turn[player_index] = (score, l, player_turn[player_index].2);
                        }
                    });
                }
                player_turn.sort_by(|p1, p2| {
                    if p1.0.cmp(&p2.0).is_eq() {
                        p1.1.cmp(&p2.1)
                    } else {
                        p1.0.cmp(&p2.0).reverse()
                    }
                });
                let mut rank = 0;
                let mut last_p: Option<(u32, usize, &GamePlayer)> = None;
                player_turn
                    .into_iter()
                    .map(|p| {
                        if let Some(last_player) = last_p {
                            if last_player.0 > p.0 || last_player.1 < p.1 {
                                rank += 1;
                            }
                        } else {
                            rank += 1;
                        }
                        last_p = Some(p);
                        Rank {
                            rank,
                            player: p.2.player.clone(),
                        }
                    })
                    .collect()
            }
        }
    }

    fn get_next_turn_player(&self, players: &[GamePlayer]) -> Option<String> {
        if let GameState::GameRunning(data) = &self.game_state {
            if !players
                .iter()
                .filter(|p| p.send_channel.is_some())
                .any(|p| {
                    p.data
                        .as_bingo_player_data()
                        .and_then(|b| b.board.as_ref())
                        .map(|b| !b.has_completed(&data.selected_numbers))
                        .unwrap_or(false)
                })
            {
                return None;
            }
            let mut cycle_iter = players.iter().cycle();
            let current_player_position = players.iter().position(|p| p.player.id == data.turn);
            if let Some(position) = current_player_position {
                cycle_iter.nth(position);
                for player in cycle_iter {
                    if player.send_channel.is_some()
                        && player
                            .data
                            .as_bingo_player_data()
                            .and_then(|b| b.board.as_ref())
                            .map(|b| !b.has_completed(&data.selected_numbers))
                            .unwrap_or(false)
                    {
                        return Some(player.player.id.clone());
                    }
                }
            };
            None
        } else {
            None
        }
    }

    fn change_turn(&mut self, player_id: &str) {
        match &mut self.game_state {
            GameState::BoardCreation(_) => {}
            GameState::GameRunning(data) => data.turn = player_id.into(),
        }
    }

    fn handle_player_message(
        &mut self,
        player_id: &str,
        players: &mut [GamePlayer],
        message: Self::PlayerMessage,
    ) -> std::result::Result<(), anyhow::Error> {
        match message {
            BingoPlayerMessages::ReadyBoard(board) => match &mut self.game_state {
                GameState::BoardCreation(board_creation) => {
                    if board_creation.ready.contains(&player_id.to_string()) {
                        Err(anyhow::anyhow!("Board already set"))
                    } else if let Some(player) =
                        players.iter_mut().find(|p| p.player.id == player_id)
                    {
                        if let Some(d) = player.data.as_bingo_player_data_mut() {
                            d.board = Some(board);
                        }

                        board_creation.ready.push(player_id.into());
                        if board_creation.ready.len() == players.len() {
                            use rand::seq::SliceRandom;
                            self.game_state = GameState::GameRunning(GameRunning {
                                turn: players
                                    .choose(&mut rand::thread_rng())
                                    .ok_or_else(|| anyhow::anyhow!("No player"))?
                                    .player
                                    .id
                                    .clone(),
                                selected_numbers: vec![],
                            });
                        }
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!("Player not found"))
                    }
                }
                GameState::GameRunning(_) => Err(anyhow::anyhow!("Game Already Running")),
            },
            BingoPlayerMessages::Move(mov) => match &mut self.game_state {
                GameState::BoardCreation(_) => Err(anyhow::anyhow!("Game Not Running")),
                GameState::GameRunning(running_data) => {
                    if running_data.turn == player_id {
                        if running_data
                            .selected_numbers
                            .iter()
                            .any(|c| c.cell_value == mov)
                        {
                            return Err(anyhow::anyhow!("Invalid move"));
                        } else {
                            running_data.selected_numbers.push(SelectedCell {
                                selected_by: player_id.into(),
                                cell_value: mov,
                            });
                            if let Some(player) = self.get_next_turn_player(players) {
                                self.change_turn(&player)
                            }
                        }
                        Ok(())
                    } else {
                        Err(anyhow::anyhow!("Not your turn"))
                    }
                }
            },
        }
    }

    fn is_game_end(&self, players: &[GamePlayer]) -> bool {
        if let Some(game_running) = self.game_state.as_game_running() {
            let online_players = players.iter().filter(|p| p.send_channel.is_some());
            online_players
                .filter(|p| {
                    p.data
                        .as_bingo_player_data()
                        .and_then(|b| b.board.as_ref())
                        .map(|board| {
                            board.get_score(&game_running.selected_numbers) < self.board_size as u32
                        })
                        .unwrap_or(false)
                })
                .count()
                <= 1
        } else {
            players.iter().filter(|p| p.send_channel.is_some()).count() <= 1
        }
    }

    fn start_game(data: Self::StartMessage) -> (Self, Self::PlayerGameData) {
        (
            Self {
                board_size: data.board_size,
                game_state: GameState::BoardCreation(BoardCreation { ready: vec![] }),
            },
            BingoPlayerData { board: None },
        )
    }

    fn input_handler(room_id: String, player_id: String) -> Self::InputHandler {
        BingoInputs { room_id, player_id }
    }
}

#[derive(Serialize, SimpleObject, Clone)]
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
    /// Returns `true` if the game_state is [`GameRunning`].
    pub fn is_game_running(&self) -> bool {
        matches!(self, Self::GameRunning(..))
    }

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
#[graphql(complex)]
pub struct Board {
    pub numbers: Vec<Vec<Cell>>,
}

#[ComplexObject]
impl Board {
    pub async fn score<'ctx>(
        &self,
        ctx: &Context<'_>,
        room_id: String,
    ) -> Result<u32, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        log::info!("Trying to Get read for room board");
        let rooms = data.private_rooms.read().await;
        log::info!("Got read for room board");
        let room = rooms.get(&room_id).ok_or("Room Not found")?;

        let game = &room.state.as_game().ok_or("Not game")?.game;
        let state = &game.as_bingo().ok_or("Not Bingo")?.game_state;
        match state {
            GameState::BoardCreation(_) => Ok(0),
            GameState::GameRunning(state) => Ok(self.get_score(&state.selected_numbers)),
        }
    }
}

impl Board {
    pub fn new(numbers: Vec<Vec<Cell>>, board_size: u16) -> Result<Self, anyhow::Error> {
        let all_num = numbers.join(&[][..]).into_iter().collect::<HashSet<_>>();
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

    pub fn get_score(&self, selected_cells: &[SelectedCell]) -> u32 {
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
        if !d1.into_iter().any(|i| ndarr.get(i).unwrap_or(&0) != &0) {
            points += 1;
        }
        if !d2.into_iter().any(|i| ndarr.get(i).unwrap_or(&0) != &0) {
            points += 1;
        }
        points
    }

    pub fn wining_points(&self) -> u32 {
        self.numbers.len() as u32
    }

    pub fn has_completed(&self, selected_cells: &[SelectedCell]) -> bool {
        self.get_score(selected_cells) >= self.wining_points()
    }

    pub fn max_points(&self) -> u32 {
        (self.numbers.len() * 2 + 2) as u32
    }
}

#[derive(Clone, Serialize, SimpleObject)]
pub struct BingoPlayerData {
    board: Option<Board>,
}

pub struct BingoInputs {
    pub room_id: String,
    pub player_id: String,
}

#[Object]
impl BingoInputs {
    pub async fn start_game<'ctx>(
        &self,
        ctx: &Context<'_>,
        board_size: u16,
    ) -> Result<bool, async_graphql::Error> {
        let room = {
            let data = ctx.data::<Storage>()?;

            let mut rooms = data.private_rooms.write().await;
            let room = rooms
                .get_mut(&self.room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(
                &self.player_id,
                PlayerEvents::StartGame(StartMessages::BingoStart(BingoStart { board_size })),
            )
            .await?;
            room.clone()
        };

        room.clone()
            .state
            .broadcast(ServerResponse::GameMessage(GameMessage {
                event: GameEvents::GameStarted(GameStarted {
                    game: room.state.as_game().ok_or("Not game")?.game.clone(),
                }),
                room: room.clone(),
            }))
            .await;
        Ok(true)
    }

    pub async fn ready_board<'ctx>(
        &self,
        ctx: &Context<'_>,
        board: Vec<Vec<u32>>,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let room = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&self.room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exis"))?;
            let board_size = room
                .state
                .as_game()
                .ok_or_else(|| async_graphql::Error::from("Game not running"))?
                .game
                .as_bingo()
                .map(|b| b.board_size)
                .ok_or("Cant find board size")?;

            room.handle_player_message(
                &self.player_id,
                PlayerEvents::GameMessage(PlayerMessages::BingoMessages(
                    BingoPlayerMessages::ReadyBoard(Board::new(board, board_size)?),
                )),
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
        ctx: &Context<'_>,
        number: u32,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let room = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&self.room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(
                &self.player_id,
                PlayerEvents::GameMessage(PlayerMessages::BingoMessages(
                    BingoPlayerMessages::Move(number),
                )),
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
}
