use serde::{de::DeserializeOwned, Serialize};

use crate::{MqttDeserializeError, MqttItem};

#[derive(MqttItem, Debug, PartialEq, Clone)]
enum MqttUpdate {
    #[mqtt_item(topic = "<id>/<name>", payload = "<payload>")]
    Variant1 {
        name: String,
        id: u32,
        payload: String,
    },
    #[mqtt_item(topic = "<id>/<name>/<payload>")]
    Variant2 {
        name: String,
        id: u32,
        payload: String,
    },
    #[mqtt_item(topic = "<id>", payload = "<name>")]
    Variant3 { name: String, id: u32 },
    #[mqtt_item(topic = "v4/hello/world/<0>")]
    Variant4(u32),
    #[mqtt_item(
        topic = "v5/hello/world/<name>",
        payload = "<id>",
        deserialize_using = "zero_bytes_option_deser",
        serialize_using = "zero_bytes_option_ser"
    )]
    Variant5 { name: String, id: Option<u32> },
}

// #[derive(MqttItem, Debug, PartialEq, Clone)]
// #[mqtt_item(
//     deserialize_using = "zero_bytes_option_deser",
//     serialize_using = "zero_bytes_option_ser"
// )]
// enum MqttUpdateOverideAll {
//     #[mqtt_item(topic = "single/hello/world/<name>", payload = "<id>")]
//     Variant5 { name: String, id: Option<u32> },
// }

pub fn zero_bytes_option_deser<'a, T>(payload: &'a [u8]) -> Result<Option<T>, MqttDeserializeError>
where
    T: DeserializeOwned,
{
    if payload.len() == 0 {
        Ok(None)
    } else {
        let payload = std::str::from_utf8(payload).map_err(|_| MqttDeserializeError::NotUtf8)?;
        let val = serde_json::from_str(payload).map_err(|e| MqttDeserializeError::Serde(e))?;
        Ok(Some(val))
    }
}

pub fn zero_bytes_option_ser<T>(
    payload: &mut Vec<u8>,
    value: &Option<T>,
) -> Result<(), serde_json::Error>
where
    T: Serialize,
{
    if let Some(value) = value {
        let string = serde_json::to_string(value)?;
        let bytes = string.as_bytes();
        bytes.iter().for_each(|val| payload.push(*val));
        Ok(())
    } else {
        Ok(())
    }
}

// #[test]
// fn global_clearable() {
//     use crate::Topic;

//     let topic = Topic::from_str("v5/hello/world/steve");
//     let payload = b"";
//     let val = MqttUpdateOverideAll::from_topic_and_payload(topic, payload).unwrap();

//     let correct = MqttUpdateOverideAll::Variant5 {
//         name: "steve".into(),
//         id: None,
//     };

//     assert_eq!(correct, val);
// }

#[test]
fn clearable() {
    use crate::Topic;
    use MqttUpdate::*;

    let topic = Topic::from_str("v5/hello/world/steve");
    let payload = b"";
    let val = MqttUpdate::from_topic_and_payload(topic, payload).unwrap();

    let correct = MqttUpdate::Variant5 {
        name: "steve".into(),
        id: None,
    };

    assert_eq!(correct, val);
}

#[test]
fn ordering() {
    use MqttUpdate::*;

    let (v1, v2, v3, v4) = (
        Variant1 {
            name: "name1".into(),
            id: 1,
            payload: "payload1".into(),
        },
        Variant2 {
            name: "name2".into(),
            id: 2,
            payload: "payload2".into(),
        },
        Variant3 {
            name: "name3".into(),
            id: 3,
        },
        Variant4(4),
    );

    macro_rules! test {
        ($input: expr, $topic: expr, $payload: expr) => {
            let (topic, payload) = $input.clone().into_topic_and_payload().unwrap();
            assert_eq!(topic.str(), $topic);
            assert_eq!(std::str::from_utf8(&payload).unwrap(), $payload);

            let out = MqttUpdate::from_topic_and_payload(topic, &payload);
            assert_eq!(Ok($input), out);
        };
    }

    test! {
        v1, "1/name1", r#""payload1""#
    };

    test! {
        v2, "2/name2/payload2", ""
    };

    test! {
        v3, "3", r#""name3""#
    };

    test! {
        v4, "v4/hello/world/4", ""
    };
}
