//! Server feature advertisement and IRC name comparison.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Casemapping {
    Ascii,
    Rfc1459,
    StrictRfc1459,
}

impl Casemapping {
    pub fn fold(self, name: &str) -> String {
        if self == Self::Ascii {
            return name.to_ascii_lowercase();
        }
        name.chars()
            .map(|c| match c {
                '[' => '{',
                ']' => '}',
                '\\' => '|',
                '~' if self == Self::Rfc1459 => '^',
                other => other.to_ascii_lowercase(),
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isupport {
    pub chantypes: Vec<char>,
    pub prefix: Vec<(char, char)>,
    pub network: Option<String>,
    pub casemapping: Casemapping,
    pub chathistory: u16,
    pub nicklen: Option<usize>,
    pub statusmsg: Vec<char>,
    pub list_modes: String,
    pub param_modes: String,
    pub set_param_modes: String,
    pub whox: bool,
}

impl Default for Isupport {
    fn default() -> Self {
        Self {
            chantypes: vec!['#', '&'],
            prefix: vec![('o', '@'), ('v', '+')],
            network: None,
            casemapping: Casemapping::Rfc1459,
            chathistory: 0,
            nicklen: None,
            statusmsg: Vec::new(),
            list_modes: "beI".into(),
            param_modes: "k".into(),
            set_param_modes: "l".into(),
            whox: false,
        }
    }
}

impl Isupport {
    pub fn apply(&mut self, token: &str) {
        let (key, value) = token.split_once('=').unwrap_or((token, ""));
        let value = value.replace(r"\x20", " ").replace(r"\x5C", "\\");
        match key.to_ascii_uppercase().as_str() {
            "CHANTYPES" => self.chantypes = value.chars().collect(),
            "PREFIX" => {
                self.prefix = value
                    .strip_prefix('(')
                    .and_then(|pair| pair.split_once(')'))
                    .map_or_else(Vec::new, |(modes, symbols)| {
                        modes.chars().zip(symbols.chars()).collect()
                    });
            }
            "NETWORK" => self.network = Some(value),
            "CASEMAPPING" => {
                self.casemapping = match value.to_ascii_lowercase().as_str() {
                    "ascii" => Casemapping::Ascii,
                    "strict-rfc1459" => Casemapping::StrictRfc1459,
                    _ => Casemapping::Rfc1459,
                };
            }
            "STATUSMSG" => self.statusmsg = value.chars().collect(),
            "CHANMODES" => {
                let mut modes = value.split(',');
                self.list_modes = modes.next().unwrap_or_default().into();
                self.param_modes = modes.next().unwrap_or_default().into();
                self.set_param_modes = modes.next().unwrap_or_default().into();
            }
            "WHOX" => self.whox = true,
            "-WHOX" => self.whox = false,
            key => self.apply_limit(key, &value),
        }
    }

    fn apply_limit(&mut self, key: &str, value: &str) {
        match key {
            "CHATHISTORY" | "DRAFT/CHATHISTORY" => {
                self.chathistory = value.parse().unwrap_or_default();
            }
            "NICKLEN" => self.nicklen = value.parse().ok().filter(|&limit| limit > 0),
            "-NICKLEN" => self.nicklen = None,
            _ => {}
        }
    }

    pub fn fold(&self, name: &str) -> String {
        self.casemapping.fold(name)
    }

    pub fn channel<'a>(&self, target: &'a str) -> Option<&'a str> {
        let name = target.trim_start_matches(self.statusmsg.as_slice());
        name.starts_with(self.chantypes.as_slice()).then_some(name)
    }

    pub fn symbols(&self) -> Vec<char> {
        self.prefix.iter().map(|&(_, symbol)| symbol).collect()
    }
}
