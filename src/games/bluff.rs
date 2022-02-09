use std::collections::HashMap;

use async_graphql::{Context, Enum, Object, SimpleObject,};
use rand::{prelude::IteratorRandom, SeedableRng, };
use serde::Serialize;

use crate::{data::{Rank, Storage, ServerResponse, GameMessage}, logic::{PlayerEvents, GameEvents, GameStarted, RoomUpdate}};

use super::{GameTrait, PlayerGameData, StartMessages, PlayerMessages};

#[derive(Clone, Serialize)]

pub struct Bluff {
    turn_start: String,
    turn: String,
    centered_card: Vec<Vec<Card>>,
    deck_card: Vec<Card>,    claimed: Option<Card>

}

#[derive(Clone, Serialize, PartialEq,Eq, SimpleObject,Debug)]
pub struct Card {
    number: CardNum,
    color: CardColor,
}

#[derive(Clone, Serialize, SimpleObject)]
pub struct BluffPlayerData {
    cards: Vec<Card>,
    end_turn_raised: bool,
}

impl From<u8> for Card {
    fn from(num: u8) -> Self {
        if num > 51 {
            panic!("Not possible")
        }
        let color = num / 13;
        let num = num % 13;
        let num = match num {
            0 => CardNum::Ace,
            1 => CardNum::Two,
            2 => CardNum::Three,
            3 => CardNum::Four,
            4 => CardNum::Five,
            5 => CardNum::Six,
            6 => CardNum::Seven,
            7 => CardNum::Eight,
            8 => CardNum::Nine,
            9 => CardNum::Ten,
            10 => CardNum::Jack,
            11 => CardNum::Queen,
            12 => CardNum::King,

            _ => panic!("Not possible"),
        };
        let color = match color {
            0 => CardColor::Spade,
            1 => CardColor::Heart,
            2 => CardColor::Club,
            3 => CardColor::Diamond,
            _ => panic!("Not possible"),
        };
        Self { number: num, color }
    }
}

#[derive(Clone, Serialize, Copy, PartialEq, Eq, Enum,Debug)]
pub enum CardColor {
    Spade,
    Heart,
    Club,
    Diamond,
}

#[derive(Clone, Serialize, Copy, PartialEq, Eq, Enum,Debug)]
pub enum CardNum {
    Ace,
    Two,
    Three,
    Four,
    Five,
    Six,
    Seven,
    Eight,
    Nine,
    Ten,
    Jack,
    Queen,
    King,
}

pub enum BluffPlayerMessages {
    RaiseEndRound,
    Deal(Vec<Card>, Card),
    Pass,
    Flip,
}

pub struct StartBluff {
    seed: u64,
}

impl GameTrait for Bluff {
    type PlayerMessage = BluffPlayerMessages;

    type StartMessage = StartBluff;

    type PlayerGameData = BluffPlayerData;

    type InputHandler = BluffInputs;

    fn is_game_running(&self) -> bool {
        true
    }

    fn can_change_turn(&self, player_id: &str) -> bool {
        self.turn == player_id
    }

    fn get_rankings(&self, players: &[crate::logic::GamePlayer]) -> Vec<crate::data::Rank> {
        let mut scored = players
            .iter()
            .map(|p| {
                (
                    p.data.as_bluff_player_data().unwrap().cards.len(),
                    &p.player,
                )
            })
            .collect::<Vec<_>>();
        scored.sort_by(|p1, p2| p1.0.cmp(&p2.0));
        let mut ranks = vec![];
        let mut last_rank = 0;
        let mut last_score = usize::MAX;
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

    fn get_next_turn_player(&self, players: &[crate::logic::GamePlayer]) -> Option<String> {
        let mut cycle_iter = players.iter().cycle();
        let current_player_position = players.iter().position(|p| p.player.id == self.turn);
        if let Some(position) = current_player_position {
            cycle_iter.nth(position);
            for player in cycle_iter {
                if player.send_channel.is_some()
                    && !player.data.as_bluff_player_data().unwrap().cards.is_empty()
                {
                    return Some(player.player.id.clone());
                }
            }
            None
        } else {
            None
        }
    }

    fn change_turn(&mut self, player_id: &str) {
        self.turn = player_id.into();
    }

    fn handle_player_message(
        &mut self,
        player_id: &str,
        players: &mut [crate::logic::GamePlayer],
        message: Self::PlayerMessage,
    ) -> Result<(), anyhow::Error> {
        match message {
            BluffPlayerMessages::RaiseEndRound => {
                let p = players.iter_mut().find(|p|p.player.id==player_id);
                if let Some(p)=p{
                    if let PlayerGameData::BluffPlayerData(data) = &mut p.data{
                        data.end_turn_raised = true;
                    }
                }
                if players.iter().all(|p|p.data.as_bluff_player_data().unwrap().end_turn_raised){
                    if let Some(p)=self.get_next_round_player(players){
                        self.turn=p.to_string();
                        self.turn_start=p;
                        for p in players{
                            if let PlayerGameData::BluffPlayerData(data) = &mut p.data{
                                data.end_turn_raised = false;
                            }
                        }
                        for cards in &self.centered_card{
                            self.deck_card.append(&mut cards.clone());
                        }
                        self.centered_card.clear();
                        self.claimed = None;
                    }
                }
                Ok(())
            },
            BluffPlayerMessages::Deal(cards,claim) => {
                if self.turn != player_id {
                    return Err(anyhow::anyhow!("Not your Turn"));
                }
                if let Some(claimed_previously)=& self.claimed{
                    if claimed_previously!=&claim{
                        return Err(anyhow::anyhow!("Cant claim another card in middle of round"))
                    }
                }
                let p = players.iter_mut().find(|p|p.player.id==player_id);
                if let Some(p) =p {
                    if let PlayerGameData::BluffPlayerData(data)= &mut p.data{
                        data.cards = data.cards.iter().cloned().filter(|f|!cards.contains(f)).collect();
                        self.centered_card.push(cards);
                        self.claimed = Some(claim);
                        if let Some(player) = self.get_next_turn_player(players){
                            self.change_turn(&player);
                        }
                    }
                }
                Ok(())
            },
            BluffPlayerMessages::Pass => {
                if self.turn != player_id {
                    return Err(anyhow::anyhow!("Not your Turn"));
                }
                if let Some(player) = self.get_next_turn_player(players){
                    self.change_turn(&player);
                }
                Ok(())
            },
            BluffPlayerMessages::Flip => {
                if self.turn != player_id {
                    return Err(anyhow::anyhow!("Not your Turn"));
                }
                let mut to_transfer = None;
                if let Some(last_cards) = self.centered_card.last(){
                    if let Some(claimed) = &self.claimed {
                        if last_cards.iter().all(|card|  card==claimed){
                            to_transfer = Some(player_id.to_string());
                        }else{
                            to_transfer = Some(self.turn_start.clone());
                        }
                    }
                    
                }
                if let Some(to_transfer) = to_transfer{
                    if let Some(p) = players.iter_mut().find(|p|p.player.id==to_transfer){
                        if let PlayerGameData::BluffPlayerData(data) = &mut p.data{
                            for cards in &self.centered_card{
                                data.cards.append(&mut cards.clone());
                            }
                            self.centered_card.clear();
                            self.turn = to_transfer.clone();
                            self.turn_start = to_transfer.clone();
                            self.claimed = None;
                        }
                    }
                }
                Ok(())
            },
        }
    }

    fn is_game_end(&self, players: &[crate::logic::GamePlayer]) -> bool {
        players.iter().filter(|p| p.send_channel.is_some()).count() <= 1
            || players
                .iter()
                .filter(|p| !p.data.as_bluff_player_data().unwrap().cards.is_empty())
                .count()
                <= 1
    }

    fn create_player_data(
        data: &Self::StartMessage,
        players: &[crate::data::Player],
        player_id: &str,
    ) -> Self::PlayerGameData {
        let mut choosen: HashMap<String, Vec<i32>> = HashMap::new();
        let mut rand = rand::rngs::StdRng::seed_from_u64(data.seed);
        for player in players {
            let cards = (0..52).filter(|f| {
                !choosen
                    .iter()
                    .fold(vec![], |mut acc, i| {
                        acc.append(&mut i.1.clone());
                        acc
                    })
                    .contains(&f)
            });

            let c = cards.choose_multiple(&mut rand, 52 / players.len());
            choosen.insert(player.id.clone(), c);
        }

        let cards = choosen
            .get(player_id)
            .unwrap()
            .iter()
            .map(|i| Card::from(*i as u8))
            .collect();
        Self::PlayerGameData {
            cards,
            end_turn_raised: false,
        }
    }

    fn input_handler(room_id: String, player_id: String) -> Self::InputHandler {
        Self::InputHandler { room_id, player_id }
    }

    fn start_game(_data: Self::StartMessage, players: &[crate::logic::GamePlayer], player_id: &str) -> Self {
        let cards = (0..52).map(|i|Card::from(i)).filter(|f| {
            !players.iter()
                .fold(vec![], |mut acc, p| {
                    acc.append(&mut p.data.as_bluff_player_data().unwrap().cards.clone());
                    acc
                })
                .contains(&f)
        }).collect();
        Self{
            turn_start: player_id.to_string(),
            turn: player_id.to_string(),
            centered_card: vec![],
            deck_card: cards,
            claimed:None,
        }
    }
}

impl Bluff {
    fn get_next_round_player(&self, players: &[crate::logic::GamePlayer]) -> Option<String> {
        let mut cycle_iter = players.iter().cycle();
        let current_player_position = players.iter().position(|p| p.player.id == self.turn_start);
        if let Some(position) = current_player_position {
            cycle_iter.nth(position);
            for player in cycle_iter {
                if player.send_channel.is_some()
                    && !player.data.as_bluff_player_data().unwrap().cards.is_empty()
                {
                    return Some(player.player.id.clone());
                }
            }
            None
        } else {
            None
        }
    }
}

pub struct BluffInputs {
    pub room_id: String,
    pub player_id: String,
}

#[Object]
impl BluffInputs {
    pub async fn start_game<'ctx>(&self,  ctx: &Context<'_>,seed:u64) -> Result<bool, async_graphql::Error> {
        let room = {
            let data = ctx.data::<Storage>()?;

            let mut rooms = data.private_rooms.write().await;
            let room = rooms
                .get_mut(&self.room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(
                &self.player_id,
                PlayerEvents::StartGame(StartMessages::BluffStart(StartBluff { seed })),
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

    pub async fn pass<'ctx>(
        &self,
        ctx: &Context<'_>,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let room = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&self.room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(
                &self.player_id,
                PlayerEvents::GameMessage(PlayerMessages::BluffPlayerMessages(
                    BluffPlayerMessages::Pass,
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

    pub async fn flip<'ctx>(
        &self,
        ctx: &Context<'_>,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let room = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&self.room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(
                &self.player_id,
                PlayerEvents::GameMessage(PlayerMessages::BluffPlayerMessages(
                    BluffPlayerMessages::Flip,
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

    pub async fn vote_round_end<'ctx>(
        &self,
        ctx: &Context<'_>,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let room = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&self.room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(
                &self.player_id,
                PlayerEvents::GameMessage(PlayerMessages::BluffPlayerMessages(
                    BluffPlayerMessages::RaiseEndRound,
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
    pub async fn deal<'ctx>(
        &self,
        ctx: &Context<'_>,
        cards:Vec<u8>,
        claim:u8,
    ) -> Result<bool, async_graphql::Error> {
        let data = ctx.data::<Storage>()?;
        let room = {
            let mut rooms = data.private_rooms.write().await;

            let room = rooms
                .get_mut(&self.room_id)
                .ok_or_else(|| async_graphql::Error::from("Room does not exis"))?;
            room.handle_player_message(
                &self.player_id,
                PlayerEvents::GameMessage(PlayerMessages::BluffPlayerMessages(
                    BluffPlayerMessages::Deal(
                        cards.into_iter().map(|c|Card::from(c)).collect(),
                        Card::from(claim)
                    ),
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

#[Object]
impl Bluff{

    pub async fn deck(&self)->Vec<Card>{
        self.deck_card.clone()
    }

    pub async fn centered_card(&self)->Vec<Vec<Card>>{
        self.centered_card.clone()
    }

    pub async fn turn(&self)->String {
        self.turn.clone()
    }

    pub async fn round_player(&self)->String {
        self.turn_start.clone()
    }

}