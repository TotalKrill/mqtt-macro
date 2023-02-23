//! A crate for easily transforming structs and (simple) enums into MQTT items with a
//! topic and payload.

mod serde_impl;

// #[cfg(test)]
mod tests;

pub use serde_impl::*;

mod topic;
pub use topic::*;

pub use mqtt_procmacro::MqttItem;

pub trait MqttItem: Sized {
    type DeserializeError;
    type SerializeError;

    /// Attempt to transform this [`MqttItem`] into it's corresponding [`Topic`] and a
    /// byte payload
    fn into_topic_and_payload(self) -> Result<(Topic, Vec<u8>), Self::SerializeError> {
        let mut topic = Topic::new();
        let mut payload = Vec::new();

        self.push_topic_and_payload(&mut topic, &mut payload)?;

        Ok((topic, payload))
    }

    /// Attempt to deserialize this [`MqttItem`] from the given topic and payload
    fn from_topic_and_payload<'a>(
        topic: Topic,
        payload: &[u8],
    ) -> Result<Self, Self::DeserializeError>;

    /// Attempt to serialize this [`MqttItem`] and those serialized values
    /// into `topic` and `payload`
    fn push_topic_and_payload(
        self,
        topic: &mut Topic,
        payload: &mut Vec<u8>,
    ) -> Result<(), Self::SerializeError>;

    /// Generates a list of all generic topics
    fn all_generic_topics() -> &'static [&'static str];
}

#[macro_export]
macro_rules! parse_or_err {
    ($topic: expr, $layer_id: literal) => {
        if let Some(entry) = $topic.next() {
            entry.parse().map_err(|_| MqttDeserializeError::Invalid)?
        } else {
            return Err(MqttDeserializeError::MissingTopicLayer($layer_id));
        }
    };
}
