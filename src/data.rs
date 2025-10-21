use std::fmt::Display;

use serde::{Deserialize, Deserializer, de};

#[derive(Debug, Deserialize, PartialEq)]
pub struct ChatMessage {
    pub time: u64,
    pub username: String,
    #[serde(deserialize_with = "MessageContainer::deserialize_from")]
    pub msg: MessageContainer,
    pub meta: ChatMeta,
}

impl ChatMessage {
    /// Short format of the message for logging purposes.
    pub fn short_format(&self) -> String {
        format!("<{}> {}", self.username, self.msg.text)
    }

    /// Message is a server whisper and should not be logged.
    pub fn should_be_skipped(&self) -> bool {
        if let Some(add_class) = &self.meta.add_class {
            return add_class == "server-whisper";
        }
        false
    }
}

impl Display for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\t{}\t{}\t{}",
            self.time, self.msg.team, self.username, self.msg.text
        )
    }
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChatMeta {
    add_class: Option<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct Login {
    pub error: Option<String>,
    pub name: Option<String>,
    pub success: bool,
}

#[derive(Debug, PartialEq)]
pub struct MessageContainer {
    text: String,
    team: Team,
}

impl MessageContainer {
    fn deserialize_from<'de, D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v: String = Deserialize::deserialize(deserializer)?;
        let dom = html_parser::Dom::parse(&v).map_err(de::Error::custom)?;
        let mut text = String::new();
        let mut team = Team::Empty;
        for child in dom.children {
            match child {
                html_parser::Node::Text(t) => {
                    text += &t;
                }
                html_parser::Node::Element(element)
                    if element.name == "span" && element.classes == ["teamColorSpan"] =>
                {
                    if let Some(html_parser::Node::Text(t)) = element.children.first()
                        && let Some(named) = Team::named_from_element(t)
                    {
                        team = named
                    }
                }
                html_parser::Node::Element(element) => {
                    text += &element.source_span.text;
                }
                other => {
                    log::debug!("Found an unexpected member in message: {:?}", other)
                }
            }
        }
        Ok(MessageContainer {
            text: text.trim().to_string(),
            team,
        })
    }
}

#[derive(Debug, PartialEq)]
enum Team {
    Empty,
    Named(String),
}

impl Display for Team {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Team::Empty => write!(f, "NULL"),
            Team::Named(name) => write!(f, "{}", name),
        }
    }
}

impl Team {
    /// Convert span text into a team name.
    fn named_from_element(text: &str) -> Option<Self> {
        if !text.starts_with("-team") {
            return None;
        }
        if !text.ends_with('-') {
            return None;
        }
        let name = &text[5..text.len() - 1];
        if name.is_empty() {
            return None;
        }
        Some(Self::Named(name.to_string()))
    }
}

#[derive(Debug, Deserialize)]
pub struct SocketConfig {
    pub servers: Vec<SocketConfigServer>,
}

#[derive(Debug, Deserialize)]
pub struct SocketConfigServer {
    pub url: String,
}

#[cfg(test)]
mod tests {
    use test_case::test_case;

    use super::{ChatMessage, ChatMeta, Login, MessageContainer, Team};
    use serde_json::json;

    #[test]
    fn chat_message_deserialize_image() {
        let timestamp: u64 = 1760633254810;
        let json = json!({
            "username": "ChetBaker",
            "msg": "<a href=\"https://example.com/image.jpg?ex=1234&amp;is=5678\" target=\"_blank\">\
                <img src=\"https://example.com/image.jpg?ex=1234&amp;is=5678\" /></a>",
            "meta": {},
            "time": timestamp
        });
        let chat: ChatMessage = serde_json::from_value(json).unwrap();
        assert_eq!(
            chat,
            ChatMessage {
                time: timestamp,
                username: "ChetBaker".into(),
                msg: MessageContainer {
                    text: "<a href=\"https://example.com/image.jpg?ex=1234&amp;is=5678\" target=\"_blank\">\
                        <img src=\"https://example.com/image.jpg?ex=1234&amp;is=5678\" /></a>".into(),
                    team: Team::Empty,
                },
                meta: ChatMeta { add_class: None },
            }
        )
    }

    #[test]
    fn chat_message_deserialize_greentext() {
        let timestamp: u64 = 1760634672025;
        let json = json!({
            "username": "PotF",
            "msg": "&gt;XD <span style=\"display:none\" class=\"teamColorSpan\">-teamwg-</span>",
            "meta": {
                "addClass": "greentext"
            },
            "time": timestamp
        });
        let chat: ChatMessage = serde_json::from_value(json).unwrap();
        assert_eq!(
            chat,
            ChatMessage {
                time: timestamp,
                username: "PotF".into(),
                msg: MessageContainer {
                    text: "&gt;XD".into(),
                    team: Team::Named("wg".into()),
                },
                meta: ChatMeta {
                    add_class: Some("greentext".into())
                },
            }
        )
    }

    #[test]
    fn chat_message_deserialize_named_team() {
        let timestamp: u64 = 1760608841390;
        let json = json!({
            "username": "ChatSpammer",
            "msg": ":harmony: :harmony: <span style=\"display:none\" class=\"teamColorSpan\">-teamck-</span>",
            "meta": {},
            "time": timestamp
        });
        let chat: ChatMessage = serde_json::from_value(json).unwrap();
        assert_eq!(
            chat,
            ChatMessage {
                time: timestamp,
                username: "ChatSpammer".into(),
                msg: MessageContainer {
                    text: ":harmony: :harmony:".into(),
                    team: Team::Named("ck".into()),
                },
                meta: ChatMeta { add_class: None },
            }
        )
    }

    #[test]
    fn chat_message_deserialize_null() {
        let timestamp: u64 = 1760631669671;
        let json = json!({
            "username": "Yuu",
            "msg": "It's hip to be square.",
            "meta": {},
            "time": timestamp
        });
        let chat: ChatMessage = serde_json::from_value(json).unwrap();
        assert_eq!(
            chat,
            ChatMessage {
                time: timestamp,
                username: "Yuu".into(),
                msg: MessageContainer {
                    text: "It's hip to be square.".into(),
                    team: Team::Empty,
                },
                meta: ChatMeta { add_class: None },
            }
        )
    }

    #[test]
    fn chat_message_deserialize_server_whisper() {
        let timestamp: u64 = 1761058613150;
        let msg = String::from(
            "Voteskip passed: 1/2 skipped; eligible voters: 2 = \
            total (2) - AFK (0) - no permission (0); ratio = 0.5",
        );
        let json = json!({
            "username": "[voteskip]",
            "msg": msg,
            "meta": {
                "addClass": "server-whisper",
                "addClassToNameAndTimestamp": true
            },
            "time": timestamp
        });
        let chat: ChatMessage = serde_json::from_value(json).unwrap();
        assert_eq!(
            chat,
            ChatMessage {
                time: timestamp,
                username: "[voteskip]".into(),
                msg: MessageContainer {
                    text: msg,
                    team: Team::Empty,
                },
                meta: ChatMeta {
                    add_class: Some("server-whisper".into())
                },
            }
        )
    }

    #[test]
    fn chat_message_display() {
        let chat = ChatMessage {
            time: 1760634889806,
            username: "Dog".into(),
            msg: MessageContainer {
                text: "5 &gt; 3".into(),
                team: Team::Named("vg".into()),
            },
            meta: ChatMeta { add_class: None },
        };
        assert_eq!(format!("{}", chat), "1760634889806\tvg\tDog\t5 &gt; 3");
    }

    #[test]
    fn chat_message_short_format() {
        let chat = ChatMessage {
            time: 1760634889806,
            username: "Dog".into(),
            msg: MessageContainer {
                text: ":carlos:".into(),
                team: Team::Named("m".into()),
            },
            meta: ChatMeta { add_class: None },
        };
        assert_eq!(format!("{}", chat.short_format()), "<Dog> :carlos:");
    }

    #[test]
    fn chat_message_should_be_skipped_server_whisper() {
        let chat = ChatMessage {
            time: 1760634889806,
            username: "[voteskip]".into(),
            msg: MessageContainer {
                text: "Voteskip passed".into(),
                team: Team::Empty,
            },
            meta: ChatMeta {
                add_class: Some("server-whisper".into()),
            },
        };
        assert_eq!(chat.should_be_skipped(), true);
    }

    #[test]
    fn chat_message_should_be_skipped_no_class() {
        let chat = ChatMessage {
            time: 1760634889806,
            username: "Dog".into(),
            msg: MessageContainer {
                text: "5 &gt; 3".into(),
                team: Team::Named("vg".into()),
            },
            meta: ChatMeta { add_class: None },
        };
        assert_eq!(chat.should_be_skipped(), false);
    }

    #[test]
    fn chat_message_should_be_skipped_wrong_class() {
        let chat = ChatMessage {
            time: 1760634889806,
            username: "Dog".into(),
            msg: MessageContainer {
                text: "5 &gt; 3".into(),
                team: Team::Named("vg".into()),
            },
            meta: ChatMeta {
                add_class: Some("greentext".into()),
            },
        };
        assert_eq!(chat.should_be_skipped(), false);
    }

    #[test]
    fn login_deserialize_error() {
        let json = json!({
            "error": "That username is registered.",
            "success": false
        });
        let login: Login = serde_json::from_value(json).unwrap();
        assert_eq!(
            login,
            Login {
                error: Some("That username is registered.".into()),
                name: None,
                success: false,
            }
        )
    }

    #[test]
    fn login_deserialize_success() {
        let json = json!({
            "guest": true,
            "name": "cupcake1",
            "success": true
        });
        let login: Login = serde_json::from_value(json).unwrap();
        assert_eq!(
            login,
            Login {
                error: None,
                name: Some("cupcake1".into()),
                success: true,
            }
        )
    }

    #[test_case(Team::Empty, "NULL" ; "empty")]
    #[test_case(Team::Named("vg".into()), "vg" ; "named")]
    fn team_display(team: Team, expected: &str) {
        let output = format!("{}", team);
        assert_eq!(output, expected);
    }

    #[test_case("-team-", None ; "blank")]
    #[test_case("-team1999-", Some(Team::Named("1999".into())) ; "numerical")]
    #[test_case("-teama-", Some(Team::Named("a".into())) ; "short")]
    #[test_case("-teamhanny-", Some(Team::Named("hanny".into())) ; "long")]
    #[test_case("-teamv", None ; "missing suffix")]
    #[test_case("teamv", None ; "broken prefix")]
    fn team_named_from_element(text: &str, expected: Option<Team>) {
        let team = Team::named_from_element(text);
        assert_eq!(team, expected);
    }
}
