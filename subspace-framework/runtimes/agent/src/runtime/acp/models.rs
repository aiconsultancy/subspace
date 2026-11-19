//! The session's model and reasoning choices, as ACP session config options.
use super::chat::{AcpChat, Pending};
use super::protocol::model_options;
use super::{ChatEvent, Event, EventQueue, Value, json};

impl AcpChat {
    pub(super) async fn apply_preferred_model(
        &mut self,
        events: &EventQueue,
    ) -> Result<(), String> {
        if let Some(model) = self.preferred_model.take()
            && self.models.choices.iter().any(|choice| choice.id == model)
            && self.models.current.as_deref() != Some(model.as_str())
        {
            return self
                .set_config(
                    Pending::RestoreSelection,
                    self.models.model_option.clone(),
                    model,
                )
                .await;
        }
        if let Some(effort) = self.preferred_effort.take()
            && self.models.choices.iter().any(|choice| {
                Some(choice.id.as_str()) == self.models.current.as_deref()
                    && choice.efforts.contains(&effort)
            })
            && self.models.effort.as_deref() != Some(effort.as_str())
        {
            return self
                .set_config(
                    Pending::RestoreSelection,
                    self.models.effort_option.clone(),
                    effort,
                )
                .await;
        }
        self.publish_ready(events).await;
        Ok(())
    }

    pub(super) async fn config_set(
        &mut self,
        result: &Value,
        events: &EventQueue,
    ) -> Result<(), String> {
        if result["configOptions"].is_array() {
            self.models = model_options(&result["configOptions"]);
        }
        self.publish_models(events).await;
        Ok(())
    }

    pub(super) async fn publish_models(&self, events: &EventQueue) {
        events
            .push(Event::Models(self.models.choices.clone()))
            .await;
        if let Some(model) = self.models.current.clone() {
            let effort = self.models.effort.clone();
            events
                .push(Event::of(
                    self.id,
                    ChatEvent::SelectedModel { model, effort },
                ))
                .await;
        }
    }

    pub(super) async fn model_set(
        &mut self,
        model: &str,
        result: &Value,
        events: &EventQueue,
    ) -> Result<(), String> {
        if result["configOptions"].is_array() {
            self.models = model_options(&result["configOptions"]);
        }
        self.publish_models(events).await;
        if self.models.current.as_deref() == Some(model) {
            let model = model.to_owned();
            events
                .push(Event::of(self.id, ChatEvent::ModelConfirmed { model }))
                .await;
        }
        Ok(())
    }

    pub(super) async fn set_model(
        &mut self,
        model: &str,
        effort: Option<String>,
    ) -> Result<(), String> {
        if !self.models.choices.iter().any(|choice| choice.id == model) {
            return Err(format!("Choose a model {} listed.", self.agent));
        }
        let option = self.models.model_option.clone();
        self.set_config(
            Pending::SetModel(model.to_owned()),
            option,
            model.to_owned(),
        )
        .await?;
        match (effort, self.models.effort_option.clone()) {
            (Some(effort), Some(option)) => {
                self.set_config(Pending::Config, Some(option), effort).await
            }
            _ => Ok(()),
        }
    }

    async fn set_config(
        &mut self,
        pending: Pending,
        option: Option<String>,
        value: String,
    ) -> Result<(), String> {
        let option = option.ok_or_else(|| format!("{} offers no model choice.", self.agent))?;
        let session = self.session_id.clone().unwrap_or_default();
        let params = json!({"sessionId": session, "configId": option, "value": value});
        self.request(pending, "session/set_config_option", params)
            .await
    }
}
