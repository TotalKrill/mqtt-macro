use std::fmt::Display;

/// A topic tree
#[derive(Debug, PartialEq, Clone)]
pub struct TopicTree(Vec<Topic>);

impl TopicTree {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    pub fn inner(&self) -> &Vec<Topic> {
        &self.0
    }

    pub fn inner_mut(&mut self) -> &mut Vec<Topic> {
        &mut self.0
    }
}

impl Display for TopicTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut collected = String::new();
        for topic in self.inner() {
            collected.push_str(&format!("{}\n", topic));
        }
        f.write_str(&collected)
    }
}

/// An MQTT topic
///
/// A topic is defined as layers, separated by a `/` character.
#[derive(Debug, PartialEq, Clone)]
pub struct Topic {
    inner: String,
}

impl Topic {
    /// Create a new topic from the given string
    pub fn from_str(input: &str) -> Self {
        // TODO: topic verification?
        Self {
            inner: input.to_string(),
        }
    }

    /// Create a new empty topic
    pub fn new() -> Self {
        Self {
            inner: String::new(),
        }
    }

    /// Push a new layer to this topic
    ///
    /// Note: if `layer` contains a `/` character, this function will behave
    /// as if multiple layers (as separated by each `/` character) have been pushed
    /// to the topic
    pub fn push(&mut self, layer: &str) {
        // TODO: topic verification?
        let value = &mut self.inner;
        if value.len() != 0 {
            self.inner.push('/');
        }
        self.inner.push_str(&layer)
    }

    /// Push a new layer to the front of this topic
    ///
    /// Note: if `layer` contains a `/` character, this function will behave
    /// as if multiple layers (as separated by each `/` character) the layers, separated
    /// by those `/` characters, were prepended to the topic.
    pub fn push_front(&mut self, layer: &str) {
        let mut value = self.inner.clone();
        if value.len() != 0 {
            value = layer.to_string() + "/" + &value;
        } else {
            value = layer.to_string();
        }
        self.inner = value;
    }

    /// Create an iterator over the layers of this topic
    pub fn layers(&self) -> impl Iterator<Item = &str> {
        self.inner.split('/')
    }

    /// Get the raw underlying `str` representing this Topic
    pub fn str(&self) -> &str {
        &self.inner
    }
}

impl Display for Topic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.str())
    }
}

impl From<Topic> for String {
    fn from(input: Topic) -> String {
        input.str().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::Topic;

    #[test]
    fn layer_count() {
        assert_eq!(
            Topic::from_str("region/device/1234/neighbor/2345/rssi")
                .layers()
                .count(),
            6
        );

        assert_eq!(Topic::from_str("+/+/+/#").layers().count(), 4);
        assert_eq!(Topic::from_str("#").layers().count(), 1);
    }

    #[test]
    fn push() {
        let mut topic = Topic::from_str("region");
        assert_eq!("region", topic.str());

        topic.push("subtopic");
        assert_eq!("region/subtopic", topic.str());
    }

    #[test]
    fn push_front() {
        let mut topic = Topic::from_str("region");
        assert_eq!("region", topic.str());

        topic.push_front("prefix");
        assert_eq!("prefix/region", topic.str());
    }
}
