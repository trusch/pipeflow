//! Serde helpers for persistent UI state maps and sets.

use crate::domain::audio::VolumeControl;
use crate::util::id::NodeIdentifier;
use crate::util::spatial::Position;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Serialization helper for HashMap<NodeIdentifier, Position>.
/// JSON requires string keys, so we serialize as a Vec of tuples.
pub(super) mod persistent_positions_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    #[derive(Serialize, Deserialize)]
    struct PositionEntry {
        identifier: NodeIdentifier,
        position: Position,
    }

    pub fn serialize<S>(
        map: &HashMap<NodeIdentifier, Position>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let entries: Vec<PositionEntry> = map
            .iter()
            .map(|(k, v)| PositionEntry {
                identifier: k.clone(),
                position: *v,
            })
            .collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<HashMap<NodeIdentifier, Position>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<PositionEntry> = Vec::deserialize(deserializer)?;
        Ok(entries
            .into_iter()
            .map(|e| (e.identifier, e.position))
            .collect())
    }
}

/// Serialization helper for HashSet<NodeIdentifier>.
/// Serialize as a Vec since HashSet of complex types needs special handling.
pub(super) mod persistent_identifiers_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S>(set: &HashSet<NodeIdentifier>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let entries: Vec<&NodeIdentifier> = set.iter().collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashSet<NodeIdentifier>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<NodeIdentifier> = Vec::deserialize(deserializer)?;
        Ok(entries.into_iter().collect())
    }
}

/// Serialization helper for HashMap<NodeIdentifier, String>.
/// JSON requires string keys, so we serialize as a Vec of tuples.
pub(super) mod persistent_names_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    #[derive(Serialize, Deserialize)]
    struct NameEntry {
        identifier: NodeIdentifier,
        custom_name: String,
    }

    pub fn serialize<S>(
        map: &HashMap<NodeIdentifier, String>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let entries: Vec<NameEntry> = map
            .iter()
            .map(|(k, v)| NameEntry {
                identifier: k.clone(),
                custom_name: v.clone(),
            })
            .collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<NodeIdentifier, String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<NameEntry> = Vec::deserialize(deserializer)?;
        Ok(entries
            .into_iter()
            .map(|e| (e.identifier, e.custom_name))
            .collect())
    }
}

/// Serialization helper for HashMap<NodeIdentifier, VolumeControl>.
/// JSON requires string keys, so we serialize as a Vec of tuples.
pub(super) mod persistent_volumes_serde {
    use super::*;
    use serde::{Deserializer, Serializer};

    #[derive(Serialize, Deserialize)]
    struct VolumeEntry {
        identifier: NodeIdentifier,
        volume: VolumeControl,
    }

    pub fn serialize<S>(
        map: &HashMap<NodeIdentifier, VolumeControl>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let entries: Vec<VolumeEntry> = map
            .iter()
            .map(|(k, v)| VolumeEntry {
                identifier: k.clone(),
                volume: v.clone(),
            })
            .collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D>(
        deserializer: D,
    ) -> Result<HashMap<NodeIdentifier, VolumeControl>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let entries: Vec<VolumeEntry> = Vec::deserialize(deserializer)?;
        Ok(entries
            .into_iter()
            .map(|e| (e.identifier, e.volume))
            .collect())
    }
}
