use std::cmp::Ordering;

use async_graphql::{ComplexObject, Context, Object, Result, SimpleObject, Union};

use colors_transform::Color;
use ndarray::Array2;
use serde::Serialize;

use crate::{
    data::{GameMessage, Player, Rank, ServerResponse, Storage},
    logic::{GameEvents, GamePlayer, GameStarted, PlayerEvents, RoomUpdate},
};

use super::{GameTrait, PlayerMessages, StartMessages};

#[derive(Clone, Serialize, Union)]
pub enum EdgeType {
    Occupied(Occupied),
    Unoccupied(Unoccupied),
}

impl EdgeType {
    /// Returns `true` if the vertex_type is [`Unoccupied`].
    pub fn is_unoccupied(&self) -> bool {
        matches!(self, Self::Unoccupied(..))
    }

    /// Returns `true` if the edge_type is [`Occupied`].
    pub fn is_occupied(&self) -> bool {
        matches!(self, Self::Occupied(..))
    }

    pub fn as_occupied(&self) -> Option<&Occupied> {
        if let Self::Occupied(v) = self {
            Some(v)
        } else {
            None
        }
    }

    pub fn as_unoccupied(&self) -> Option<&Unoccupied> {
        if let Self::Unoccupied(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

#[derive(Clone, Serialize, SimpleObject)]
pub struct Occupied {
    pub mov_no: u32,
    pub occupied_by: String,
    pub id: u32,
}

#[derive(Clone, Serialize, SimpleObject)]
pub struct Unoccupied {
    pub id: u32,
}

#[derive(Clone, Serialize)]
pub struct Boxes {
    pub horizontal_edges: Array2<EdgeType>,
    pub vertical_edges: Array2<EdgeType>,
    pub turn: String,
}

pub enum BoxesPlayerMessages {
    Move(Move),
}
pub struct Move {
    pub edge_id: u32,
}

pub struct BoxesStart {
    pub board_width: u32,
    pub board_height: u32,
}

#[derive(Clone, Serialize, SimpleObject)]
#[graphql(complex)]
pub struct BoxesPlayerData {
    pub color: String,
}

pub struct BoxesInputs {
    pub room_id: String,
    pub player_id: String,
}

#[derive(Default, Clone, Serialize, SimpleObject)]
pub struct Cell {
    pub occupied_by: Option<String>,
}

#[Object]
impl Boxes {
    pub async fn vertical_edges(&self) -> Result<&[EdgeType], async_graphql::Error> {
        self.vertical_edges
            .as_slice()
            .ok_or_else(|| "Cant get array".into())
    }
    pub async fn horizontal_edges(&self) -> Result<&[EdgeType], async_graphql::Error> {
        self.horizontal_edges
            .as_slice()
            .ok_or_else(|| "Cant get array".into())
    }

    pub async fn cells(&self) -> Result<Vec<Cell>, async_graphql::Error> {
        Ok(self
            .get_cells()
            .as_slice()
            .ok_or("Cant get array")?
            .to_vec())
    }

    pub async fn turn(&self) -> &str {
        &self.turn
    }
}

impl Boxes {
    pub fn get_cells(&self) -> Array2<Cell> {
        let width = self.horizontal_edges.rows().into_iter().len();
        let height = self.vertical_edges.columns().into_iter().len();
        let mut cells = Array2::<Cell>::default((width, height));
        for i in 0..height {
            for j in 0..width {
                let left = self
                    .horizontal_edges
                    .get((i, j))
                    .unwrap_or_else(|| panic!("Cant find left edge {} {}", i, j));
                let right = self
                    .horizontal_edges
                    .get((i, j + 1))
                    .unwrap_or_else(|| panic!("Cant find left edge {} {}", i, j));
                let top = self
                    .vertical_edges
                    .get((i, j))
                    .unwrap_or_else(|| panic!("Cant find left edge {} {}", i, j));
                let bottom = self
                    .vertical_edges
                    .get((i + 1, j))
                    .unwrap_or_else(|| panic!("Cant find left edge {} {}", i, j));
                let mut edges = [left, right, top, bottom];

                if edges.iter().all(|e| e.is_occupied()) {
                    edges.sort_by(|a, b| {
                        if a.is_unoccupied() && b.is_unoccupied() {
                            Ordering::Equal
                        } else if a.is_occupied() && b.is_unoccupied() {
                            Ordering::Greater
                        } else if a.is_unoccupied() && b.is_occupied() {
                            Ordering::Less
                        } else {
                            a.as_occupied()
                                .unwrap()
                                .mov_no
                                .cmp(&b.as_occupied().unwrap().mov_no)
                        }
                    });
                    cells.get_mut((i, j)).replace(&mut Cell {
                        occupied_by: Some(
                            edges
                                .iter()
                                .last()
                                .unwrap()
                                .as_occupied()
                                .unwrap()
                                .occupied_by
                                .clone(),
                        ),
                    });
                } else {
                    cells
                        .get_mut((i, j))
                        .replace(&mut Cell { occupied_by: None });
                }
            }
        }
        cells
    }

    pub fn get_score(&self, player_id: &str) -> u32 {
        self.get_cells()
            .iter()
            .filter(|c| {
                if let Some(occ_by) = &c.occupied_by {
                    occ_by == player_id
                } else {
                    false
                }
            })
            .count() as u32
    }
}

impl GameTrait for Boxes {
    type PlayerMessage = BoxesPlayerMessages;

    type StartMessage = BoxesStart;

    type PlayerGameData = BoxesPlayerData;

    type InputHandler = BoxesInputs;

    fn is_game_running(&self) -> bool {
        true
    }

    fn can_change_turn(&self, player_id: &str) -> bool {
        self.turn == player_id
    }

    fn get_rankings(&self, players: &[GamePlayer]) -> Vec<Rank> {
        let mut scored = players
            .iter()
            .map(|p| (self.get_score(&p.player.id), &p.player))
            .collect::<Vec<_>>();
        scored.sort_by(|p1, p2| p1.0.cmp(&p2.0).reverse());
        let mut ranks = vec![];
        let mut last_rank = 0;
        let mut last_score = u32::MAX;
        for p in scored.iter() {
            if last_score > p.0 {
                last_rank += 1;
                last_score = p.0;
            }
            ranks.push(Rank {
                rank: last_rank,
                player: p.1.clone(),
            })
        }
        ranks
    }

    fn get_next_turn_player(&self, players: &[GamePlayer]) -> Option<String> {
        if self.get_cells().iter().all(|p| p.occupied_by.is_some())
            || players.iter().all(|p| p.send_channel.is_none())
        {
            None
        } else {
            let mut cycle_iter = players.iter().cycle();
            let current_player_position = players.iter().position(|p| p.player.id == self.turn);
            if let Some(position) = current_player_position {
                cycle_iter.nth(position);
                for player in cycle_iter {
                    if player.send_channel.is_some() {
                        return Some(player.player.id.clone());
                    }
                }
                None
            } else {
                None
            }
        }
    }

    fn change_turn(&mut self, player_id: &str) {
        self.turn = player_id.into();
    }

    fn handle_player_message(
        &mut self,
        player_id: &str,
        players: &mut [GamePlayer],
        message: Self::PlayerMessage,
    ) -> Result<(), anyhow::Error> {
        match message {
            BoxesPlayerMessages::Move(mov) => {
                if self.turn != player_id {
                    return Err(anyhow::anyhow!("Not your Turn"));
                }
                let previous_cell_count = self
                    .get_cells()
                    .iter()
                    .filter(|c| c.occupied_by.is_some())
                    .count();
                let mov_no = self
                    .vertical_edges
                    .as_slice()
                    .ok_or_else(|| anyhow::anyhow!("No edge"))?
                    .iter()
                    .chain(
                        self.horizontal_edges
                            .as_slice()
                            .ok_or_else(|| anyhow::anyhow!("No edge"))?
                            .iter(),
                    )
                    .map(|e| match e {
                        EdgeType::Occupied(o) => o.mov_no,
                        EdgeType::Unoccupied(_) => 0,
                    })
                    .max()
                    .unwrap_or_default()
                    + 1;
                let edge = self
                    .horizontal_edges
                    .as_slice_mut()
                    .ok_or_else(|| anyhow::anyhow!("no elements"))?
                    .iter_mut()
                    .chain(
                        self.vertical_edges
                            .as_slice_mut()
                            .ok_or_else(|| anyhow::anyhow!("no elements"))?
                            .iter_mut(),
                    )
                    .find(|e| match e {
                        EdgeType::Occupied(o) => o.id == mov.edge_id,
                        EdgeType::Unoccupied(u) => u.id == mov.edge_id,
                    })
                    .ok_or_else(|| anyhow::anyhow!("Cant find edge"))?;
                match edge {
                    EdgeType::Occupied(_) => return Err(anyhow::anyhow!("Edge already occupied")),
                    EdgeType::Unoccupied(_) => {
                        *edge = EdgeType::Occupied(Occupied {
                            mov_no,
                            occupied_by: player_id.into(),
                            id: mov.edge_id,
                        })
                    }
                }
                let new_cell_count = self
                    .get_cells()
                    .iter()
                    .filter(|c| c.occupied_by.is_some())
                    .count();
                if new_cell_count <= previous_cell_count {
                    if let Some(player) = self.get_next_turn_player(players) {
                        self.change_turn(&player)
                    }
                }
            }
        }
        Ok(())
    }

    fn is_game_end(&self, players: &[GamePlayer]) -> bool {
        players.iter().all(|p| p.send_channel.is_none())
            || self
                .get_cells()
                .iter()
                .all(|cell| cell.occupied_by.is_some())
    }

    fn start_game(data: Self::StartMessage, players: &[Player], _player_id: &str) -> Self {
        let mut id = 0;
        let horizontal_edges = Array2::<EdgeType>::from_shape_fn(
            (data.board_height as usize, (data.board_width + 1) as usize),
            |_| {
                id += 1;
                EdgeType::Unoccupied(Unoccupied { id })
            },
        );
        let vertical_edges = Array2::<EdgeType>::from_shape_fn(
            ((data.board_height + 1) as usize, data.board_width as usize),
            |_| {
                id += 1;
                EdgeType::Unoccupied(Unoccupied { id })
            },
        );
        use rand::seq::SliceRandom;
        let boxx = Self {
            horizontal_edges,
            vertical_edges,
            turn: players
                .choose(&mut rand::thread_rng())
                .map(|p| p.id.clone())
                .unwrap_or_default(),
        };

        boxx
    }

    fn input_handler(room_id: String, player_id: String) -> Self::InputHandler {
        Self::InputHandler { room_id, player_id }
    }

    fn create_player_data(
        _data: &Self::StartMessage,
        players: &[Player],
        player_id: &str,
    ) -> Self::PlayerGameData {
        let player_idx = players
            .iter()
            .position(|p| p.id == player_id)
            .unwrap_or_default();
        let max_players = players.len();
        let c = (player_idx * 360) as f32 / (max_players as f32);
        let color = colors_transform::Hsl::from(c, 100.0, 50.0);
        BoxesPlayerData {
            color: color.to_rgb().to_css_hex_string(),
        }
    }
}

#[ComplexObject]
impl BoxesPlayerData {
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

        let game_data = &room.state.as_game().ok_or("Not game")?;
        let game = &game_data.game;
        let boxes = &game.as_boxes().ok_or("Not Bingo")?;
        let player = game_data
            .players
            .iter()
            .find(|p| {
                if let Some(b) = p.data.as_boxes_player_data() {
                    b.color == self.color
                } else {
                    false
                }
            })
            .ok_or("Cant find player")?;
        Ok(boxes.get_score(&player.player.id))
    }
}

#[Object]
impl BoxesInputs {
    pub async fn start_game<'ctx>(
        &self,
        ctx: &Context<'_>,
        board_width: u32,
        board_height: u32,
    ) -> Result<bool, async_graphql::Error> {
        let room = {
            let data = ctx.data::<Storage>()?;

            let mut rooms = data.private_rooms.write().await;
            let room = rooms
                .get_mut(&self.room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(
                &self.player_id,
                PlayerEvents::StartGame(StartMessages::BoxesStart(BoxesStart {
                    board_width,
                    board_height,
                })),
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

    pub async fn player_move<'ctx>(
        &self,
        ctx: &Context<'_>,
        edge_id: u32,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let room = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&self.room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(
                &self.player_id,
                PlayerEvents::GameMessage(PlayerMessages::BoxesPlayerMessages(
                    BoxesPlayerMessages::Move(Move { edge_id }),
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
