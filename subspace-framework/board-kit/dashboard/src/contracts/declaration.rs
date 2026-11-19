//! What a board declares about contracts, captured at registration without a
//! view, and the checks the host runs on every included board's declarations.

use std::collections::HashSet;

use subspace_contracts::{Event, MessageKey, Request};

/// A board's part in one contract message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    /// Sends the event.
    Publishes,
    /// Receives the event from other boards.
    Subscribes,
    /// Asks the request.
    Requests,
    /// Answers the request. At most one included board responds to each.
    Responds,
}

impl Role {
    /// The role as a verb: `publishes`, `subscribes`, `requests`, `responds`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Publishes => "publishes",
            Self::Subscribes => "subscribes",
            Self::Requests => "requests",
            Self::Responds => "responds",
        }
    }
}

/// One role for one message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Declaration {
    role: Role,
    key: MessageKey,
}

impl Declaration {
    pub const fn publishes<E: Event>() -> Self {
        Self {
            role: Role::Publishes,
            key: MessageKey::event::<E>(),
        }
    }

    pub const fn subscribes<E: Event>() -> Self {
        Self {
            role: Role::Subscribes,
            key: MessageKey::event::<E>(),
        }
    }

    // qual:api
    pub const fn requests<R: Request>() -> Self {
        Self {
            role: Role::Requests,
            key: MessageKey::request::<R>(),
        }
    }

    pub const fn responds<R: Request>() -> Self {
        Self {
            role: Role::Responds,
            key: MessageKey::request::<R>(),
        }
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn key(&self) -> MessageKey {
        self.key
    }
}

/// Every contract role a board declares: `Dashboard::contracts`, read by
/// `BoardRegistration` without creating the board.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BoardContracts(&'static [Declaration]);

impl BoardContracts {
    pub const fn new(declarations: &'static [Declaration]) -> Self {
        Self(declarations)
    }

    pub fn declarations(&self) -> &'static [Declaration] {
        self.0
    }

    pub fn declares(&self, role: Role, key: MessageKey) -> bool {
        self.0
            .iter()
            .any(|declaration| declaration.role == role && declaration.key == key)
    }
}

/// Rejects malformed names, a board declaring one role twice and a request
/// with more than one responder among `boards`.
pub fn validate(boards: &[(&'static str, BoardContracts)]) -> Result<(), String> {
    let mut responders: HashSet<MessageKey> = HashSet::new();
    for (board, contracts) in boards {
        let mut seen: HashSet<Declaration> = HashSet::new();
        for declaration in contracts.declarations() {
            let key = declaration.key;
            if !key.is_valid() {
                return Err(format!(
                    "invalid contract message {} for board {board}",
                    key.label()
                ));
            }
            if !seen.insert(*declaration) {
                return Err(format!(
                    "board {board} declares that it {} {} twice",
                    declaration.role.as_str(),
                    key.label()
                ));
            }
            if declaration.role == Role::Responds && !responders.insert(key) {
                return Err(format!("more than one board responds to {}", key.label()));
            }
        }
    }
    Ok(())
}
