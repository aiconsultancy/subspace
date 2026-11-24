//! Attributing a proposal to the signed-in account or a human maker.

use gpui_kit::component::input::{InputState, TextareaState};
use gpui_kit::{App, Context, Entity, Window};
use subspace_api_client::decision_writes::{DecisionAttribution, DecisionWriteError};
use subspace_api_client::model::{DecisionEvidenceKind, Me};

use super::draft::{input, paragraph};
use super::picker::Picker;
use crate::Decisions;

const EVIDENCE: [(&str, DecisionEvidenceKind); 5] = [
    ("IRC message", DecisionEvidenceKind::IrcMessage),
    ("Teams message", DecisionEvidenceKind::TeamsMessage),
    ("Subspace comment", DecisionEvidenceKind::SubspaceComment),
    ("Document or URL", DecisionEvidenceKind::DocumentOrUrl),
    ("Meeting", DecisionEvidenceKind::Meeting),
];

pub(super) struct Evidence {
    pub(super) maker: Picker,
    pub(super) kind: Picker,
    pub(super) reference: Entity<InputState>,
    pub(super) excerpt: Entity<TextareaState>,
}

impl Evidence {
    pub(super) fn new(
        me: &Me,
        humans: &[(String, String)],
        window: &mut Window,
        cx: &mut Context<Decisions>,
    ) -> Self {
        let self_label = format!("{} (me)", me.display_name);
        let mut makers = vec![(self_label.clone(), String::new())];
        makers.extend(
            humans
                .iter()
                .filter(|(id, _)| id != &me.user_id)
                .map(|(id, name)| {
                    let duplicate = name == &self_label
                        || humans
                            .iter()
                            .any(|(other, label)| other != id && label == name);
                    let label = if duplicate {
                        format!("{name} · {id}")
                    } else {
                        name.clone()
                    };
                    (label, id.clone())
                }),
        );
        let mut kinds = vec![("No evidence".to_owned(), String::new())];
        kinds.extend(
            EVIDENCE
                .iter()
                .map(|(name, _)| (name.to_string(), name.to_string())),
        );
        Self {
            maker: Picker::new(makers, Some(""), window, cx),
            kind: Picker::new(kinds, Some(""), window, cx),
            reference: input(
                "",
                "Message, comment ID, document link or meeting reference",
                window,
                cx,
            ),
            excerpt: paragraph("", "", window, cx),
        }
    }

    pub(super) fn attribution(
        &self,
        cx: &App,
    ) -> Result<Option<DecisionAttribution>, DecisionWriteError> {
        let maker = self
            .maker
            .value(cx)
            .filter(|id| !id.is_empty())
            .map(str::to_owned);
        let kind = self
            .kind
            .value(cx)
            .and_then(|selected| EVIDENCE.iter().find(|(name, _)| *name == selected));
        let reference = self.reference.read(cx).value().trim().to_owned();
        let excerpt = self.excerpt.read(cx).value().trim().to_owned();
        let Some((_, kind)) = kind else {
            return if maker.is_some() || !reference.is_empty() || !excerpt.is_empty() {
                Err(DecisionWriteError::Evidence)
            } else {
                Ok(None)
            };
        };
        let excerpt = (!excerpt.is_empty()).then_some(excerpt);
        DecisionAttribution::new(maker, *kind, reference, excerpt).map(Some)
    }
}
