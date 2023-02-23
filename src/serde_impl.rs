use serde::{de::DeserializeOwned, Serialize};

pub fn serde_json_serialize<T>(payload: &mut Vec<u8>, value: &T) -> Result<(), serde_json::Error>
where
    T: Serialize,
{
    let string = serde_json::to_string(value)?;
    let bytes = string.as_bytes();
    bytes.iter().for_each(|val| payload.push(*val));
    Ok(())
}

#[derive(Debug)]
pub enum MqttDeserializeError {
    MissingTopicLayer(String),
    UnknownLayer,
    NotUtf8,
    Invalid,
    InvalidTopicLayer(String, String),
    Serde(serde_json::Error),
}

impl PartialEq<MqttDeserializeError> for MqttDeserializeError {
    fn eq(&self, other: &MqttDeserializeError) -> bool {
        match (self, other) {
            (Self::Serde(_), Self::Serde(_)) => true,
            _ => core::mem::discriminant(self) == core::mem::discriminant(other),
        }
    }
}

impl From<()> for MqttDeserializeError {
    fn from(_: ()) -> Self {
        Self::Invalid
    }
}

pub fn serde_json_deserialize<'a, T>(payload: &'a [u8]) -> Result<T, MqttDeserializeError>
where
    T: DeserializeOwned,
{
    let payload = std::str::from_utf8(payload).map_err(|_| MqttDeserializeError::NotUtf8)?;
    serde_json::from_str(payload).map_err(|e| MqttDeserializeError::Serde(e))
}
