//! The board help tool: the same compiled-in Markdown the shell shows, read
//! only, for the active boards alone. One generic tool, so no board or
//! product code is needed to publish a document to Copilot chat and MCP.

use super::{Access, AgentTool, ReadService, ReadTool, ToolSpec, encoded, schema};
use crate::help_image::{HelpImage, validate_images};
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;

/// One active board's compiled-in help document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardDoc {
    pub id: &'static str,
    pub title: &'static str,
    /// The board's `HELP_MARKDOWN`, exactly as the shell renders it.
    pub markdown: &'static str,
    /// Bundled image metadata and bytes, restricted to this active board.
    pub images: &'static [HelpImage],
}

/// The tool name, unique in a product.
pub const HELP_TOOL: &str = "board_help";

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    List,
    Get { board_id: String },
    Image { board_id: String, image_id: String },
}

struct BoardHelpService {
    docs: Vec<BoardDoc>,
}

impl BoardHelpService {
    fn list(&self) -> Result<Value, String> {
        let boards: Vec<Value> = self
            .docs
            .iter()
            .map(|doc| serde_json::json!({"board_id": doc.id, "title": doc.title}))
            .collect();
        encoded(serde_json::json!({ "boards": boards }))
    }

    fn get(&self, board_id: &str) -> Result<Value, String> {
        let doc = self.doc(board_id)?;
        validate_images(doc.images)?;
        let images: Vec<Value> = doc
            .images
            .iter()
            .map(|image| {
                serde_json::json!({
                    "image_id": image.id,
                    "caption": image.caption,
                    "media_type": image.media_type,
                    "byte_length": image.bytes.len(),
                })
            })
            .collect();
        encoded(serde_json::json!({
            "board_id": doc.id,
            "title": doc.title,
            "markdown": doc.markdown,
            "images": images,
        }))
    }

    fn image(&self, board_id: &str, image_id: &str) -> Result<Value, String> {
        let doc = self.doc(board_id)?;
        validate_images(doc.images)?;
        let image = doc
            .images
            .iter()
            .find(|image| image.id == image_id)
            .ok_or_else(|| {
                format!("No bundled help image {image_id} in active board {board_id}.")
            })?;
        image.reply(doc.id)
    }

    fn doc(&self, board_id: &str) -> Result<&BoardDoc, String> {
        self.docs
            .iter()
            .find(|doc| doc.id == board_id)
            .ok_or_else(|| {
                format!(
                    "No active board {board_id} documents itself. Choose from: {}.",
                    self.ids().join(", ")
                )
            })
    }

    fn ids(&self) -> Vec<&'static str> {
        self.docs.iter().map(|doc| doc.id).collect()
    }
}

impl ReadService for BoardHelpService {
    type Request = Request;

    fn read(&self, request: Request) -> Result<Value, String> {
        match request {
            Request::List => self.list(),
            Request::Get { board_id } => self.get(board_id.trim()),
            Request::Image { board_id, image_id } => self.image(board_id.trim(), &image_id),
        }
    }
}

fn spec() -> ToolSpec {
    ToolSpec {
        name: HELP_TOOL,
        title: "Board help",
        description: "Read the bundled help of an active board: what it shows, how to set \
                      it up, which actions it supports and where its limits are. Call it \
                      when a question is about a board rather than about a service item.",
        access: Access::Read,
        arguments: schema::flat(&[
            schema::action("list", "The active boards that have help.", vec![]),
            schema::action(
                "get",
                "The board's help as Markdown and an image metadata catalog, without image bytes.",
                vec![("board_id", schema::text(), true)],
            ),
            schema::action(
                "image",
                "Read one bundled help illustration from the active board. Use an image_id from get.",
                vec![
                    ("board_id", schema::text(), true),
                    ("image_id", schema::text(), true),
                ],
            ),
        ]),
        approval: super::ApprovalPolicy::None,
    }
}

/// The help tool for the boards that carry a document, or `None` when no
/// active board has one: a consumer then lists no help tool at all.
pub fn help_tool(docs: Vec<BoardDoc>) -> Option<Arc<dyn AgentTool>> {
    let service = service(documented(docs))?;
    Some(Arc::new(ReadTool::new(spec(), service)))
}

/// The boards that actually wrote a document.
fn documented(docs: Vec<BoardDoc>) -> Vec<BoardDoc> {
    docs.into_iter()
        .filter(|doc| !doc.markdown.trim().is_empty())
        .collect()
}

fn service(docs: Vec<BoardDoc>) -> Option<Arc<BoardHelpService>> {
    (!docs.is_empty()).then(|| Arc::new(BoardHelpService { docs }))
}

#[cfg(test)]
#[path = "help/tests.rs"]
mod image_tests;

#[cfg(test)]
mod tests {
    use super::{BoardDoc, HELP_TOOL, help_tool};
    use crate::{Access, AgentTool, test_support};
    use serde_json::{Value, json};
    use std::sync::Arc;

    const IRC: &str = "# IRC\n\nWhat the board shows.";

    fn docs() -> Vec<BoardDoc> {
        vec![
            BoardDoc {
                id: "irc",
                title: "Board",
                markdown: IRC,
                images: &[],
            },
            BoardDoc {
                id: "github",
                title: "GitHub",
                markdown: "# GitHub",
                images: &[],
            },
        ]
    }

    async fn call(tool: &Arc<dyn AgentTool>, arguments: Value) -> Result<Value, String> {
        let (_, invocation) = test_support::invocation(arguments, true);
        tool.call(invocation).await
    }

    #[tokio::test]
    async fn the_tool_lists_active_boards_and_returns_their_exact_markdown() {
        let tool = help_tool(docs()).unwrap();
        assert_eq!(tool.spec().name, HELP_TOOL);
        assert_eq!(tool.spec().access, Access::Read);
        let listed = call(&tool, json!({"action": "list"})).await.unwrap();
        assert_eq!(
            listed["boards"],
            json!([
                {"board_id": "irc", "title": "Board"},
                {"board_id": "github", "title": "GitHub"},
            ])
        );
        let found = call(&tool, json!({"action": "get", "board_id": " irc "}))
            .await
            .unwrap();
        assert_eq!(found["markdown"], json!(IRC));
        assert_eq!(found["title"], json!("Board"));
    }

    #[tokio::test]
    async fn a_board_outside_the_active_documents_is_refused() {
        let tool = help_tool(docs()).unwrap();
        let error = call(&tool, json!({"action": "get", "board_id": "missing"}))
            .await
            .unwrap_err();
        assert!(error.contains("No active board missing"), "{error}");
        assert!(error.contains("irc, github"), "{error}");
    }

    #[tokio::test]
    async fn unknown_actions_and_fields_are_refused_before_any_lookup() {
        let tool = help_tool(docs()).unwrap();
        for arguments in [
            json!({"action": "write", "board_id": "irc"}),
            json!({"action": "get"}),
            json!({"action": "get", "board_id": "irc", "token": "x"}),
        ] {
            assert!(call(&tool, arguments.clone()).await.is_err(), "{arguments}");
        }
    }

    #[test]
    fn boards_without_a_document_publish_no_help_tool() {
        assert!(help_tool(Vec::new()).is_none());
        assert!(
            help_tool(vec![BoardDoc {
                id: "terminal",
                title: "Terminal",
                markdown: "  \n",
                images: &[],
            }])
            .is_none()
        );
    }
}
