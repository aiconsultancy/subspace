//! Every agent request receives a reply, including requests racing Stop.
use super::chat::{AcpChat, PermissionAsk};
use super::{Value, protocol};

impl AcpChat {
    pub(super) async fn agent_request(
        &mut self,
        id: &Value,
        method: &str,
        params: &Value,
    ) -> Result<Option<PermissionAsk>, String> {
        if method != "session/request_permission" {
            self.write(protocol::method_not_found(id, method)).await?;
            return Ok(None);
        }
        if self.cancelled {
            self.write(protocol::reply(id, protocol::permission_outcome(None)))
                .await?;
            return Ok(None);
        }
        let generation = super::next_id();
        self.permissions.push((id.clone(), generation));
        Ok(Some(PermissionAsk {
            request: id.clone(),
            generation,
            options: params["options"].clone(),
            summary: protocol::permission_summary(self.agent, params),
        }))
    }

    pub(super) async fn answer_permission(
        &mut self,
        ask: &PermissionAsk,
        allowed: bool,
    ) -> Result<(), String> {
        let Some(index) = self
            .permissions
            .iter()
            .position(|(id, generation)| *id == ask.request && *generation == ask.generation)
        else {
            return Ok(());
        };
        self.permissions.remove(index);
        let option = if self.cancelled {
            None
        } else {
            protocol::permission_option(&ask.options, allowed)
        };
        let outcome = protocol::permission_outcome(option.as_deref());
        self.write(protocol::reply(&ask.request, outcome)).await
    }
}
