use std::sync::Arc;

use serde::de::DeserializeOwned;
use ustr::Ustr;

use crate::{Client, Iter, Player, protocol::*};

/// A group of players playing the same game.
///
/// Currently this is only used for item link groups.
pub struct Group {
    name: Ustr,
    game: Ustr,
    members: Vec<Arc<Player>>,
}

impl Group {
    /// Creates a group by filling in the players from [client].
    ///
    /// We have to do this each time a group is accessed because the client's
    /// name could have been replaced, which we handle by updating the entire
    /// client struct.
    pub(crate) fn new<S: DeserializeOwned>(
        network: &NetworkSlot,
        team: u64,
        client: &Client<S>,
    ) -> Self {
        assert!(network.r#type == SlotType::Group);
        Group {
            name: network.name,
            game: network.game,
            members: network
                .group_members
                .iter()
                .map(|p| client.player_arc(team, *p).unwrap())
                .collect(),
        }
    }

    /// The group name.
    pub fn name(&self) -> Ustr {
        self.name
    }

    /// The game being played by all group members.
    pub fn game(&self) -> Ustr {
        self.game
    }

    /// The members of the group.
    pub fn members(&self) -> impl Iter<&Player> {
        self.members.iter().map(|p| p.as_ref())
    }
}
