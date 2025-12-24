use std::collections::HashSet;
use std::sync::LazyLock;
use std::time::SystemTime;

use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error, ser::*};
use serde_json;
use serde_json::Value;
use serde_with::{TimestampSeconds, serde_as};
use ustr::{Ustr, UstrSet};

/// The name of the tag that indicates death links.
pub(crate) static DEATH_LINK_TAG: LazyLock<Ustr> = LazyLock::new(|| Ustr::from("DeathLink"));

#[derive(Debug, Clone)]
pub(crate) struct Bounced {
    pub(crate) games: Option<UstrSet>,
    pub(crate) slots: Option<HashSet<u32>>,
    pub(crate) tags: Option<UstrSet>,
    pub(crate) data: BounceData,
}

/// An internal representation of the [Bounced] struct, used as an intermediate
/// state to determine how to decode the [BounceData].
#[derive(Debug, Clone, Deserialize, Serialize)]
struct InternalBounced {
    games: Option<UstrSet>,
    slots: Option<HashSet<u32>>,
    #[serde(default)]
    tags: Option<UstrSet>,
    data: Option<Value>,
}

// Deserialize Bounced based on its tags.
impl<'de> Deserialize<'de> for Bounced {
    fn deserialize<D>(deserializer: D) -> Result<Bounced, D::Error>
    where
        D: Deserializer<'de>,
    {
        let internal = InternalBounced::deserialize(deserializer)?;
        if let Some(ref tags) = internal.tags
            && tags.contains(&*DEATH_LINK_TAG)
        {
            Ok(Bounced {
                games: internal.games,
                slots: internal.slots,
                tags: internal.tags,
                data: BounceData::DeathLink(
                    match serde_json::from_value(internal.data.unwrap_or_default()) {
                        Ok(data) => data,
                        Err(err) => return Err(D::Error::custom(err)),
                    },
                ),
            })
        } else {
            Ok(Bounced {
                games: internal.games,
                slots: internal.slots,
                tags: internal.tags,
                data: BounceData::Generic(internal.data),
            })
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum BounceData {
    DeathLink(DeathLink),
    Generic(Option<Value>),
}

#[serde_as]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeathLink {
    #[serde_as(as = "TimestampSeconds<f64>")]
    pub time: SystemTime,
    pub cause: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone)]
pub(crate) struct Bounce {
    pub(crate) games: Option<UstrSet>,
    pub(crate) slots: Option<HashSet<u32>>,
    pub(crate) tags: Option<UstrSet>,
    pub(crate) data: BounceData,
}

impl Serialize for Bounce {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct(
            "Bounce",
            2 + self.games.iter().count() + self.slots.iter().count(),
        )?;

        if let Some(games) = &self.games {
            state.serialize_field("games", games)?;
        }

        if let Some(slots) = &self.slots {
            state.serialize_field("slots", slots)?;
        }

        match &self.data {
            BounceData::DeathLink(death_link) => {
                let mut tags = self.tags.clone().unwrap_or_default();
                tags.insert(*DEATH_LINK_TAG);

                state.serialize_field("tags", &tags)?;
                state.serialize_field("data", &death_link)?;
            }
            BounceData::Generic(value) => {
                if let Some(tags) = &self.tags {
                    state.serialize_field("tags", tags)?;
                }
                state.serialize_field("data", &value)?;
            }
        }

        state.end()
    }
}
