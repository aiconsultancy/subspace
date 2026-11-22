use super::{Outcome, ProjectBrowser};
use crate::contracts::RuntimeKind;
use crate::environments::EnvironmentManager;
use gpui_kit::component::select::SearchableVec;
use gpui_kit::{Context, Window};

impl ProjectBrowser {
    /// Opens the New project form with its Project ID field focused.
    pub(crate) fn begin_creating(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.creating = true;
        self.project_id
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn refresh_catalog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.run(window, cx, |service| {
            let manager = EnvironmentManager::new(service.root().join("environments"));
            let environments = manager
                .catalog()?
                .into_iter()
                .filter(|environment| {
                    matches!(
                        environment.spec.runtime,
                        RuntimeKind::Python | RuntimeKind::Node | RuntimeKind::Bun
                    )
                })
                .collect();
            Ok(Outcome::Catalog(
                service.list()?,
                environments,
                crate::editors::discover(),
            ))
        });
    }

    pub(super) fn sync_environments(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self
            .environments
            .iter()
            .map(|environment| environment.spec.id.clone())
            .collect::<Vec<_>>();
        self.environment.update(cx, |state, cx| {
            state.set_items(SearchableVec::new(ids.clone()), window, cx);
            if state.selected_value().is_none()
                && let Some(first) = ids.first()
            {
                state.set_selected_value(first, window, cx);
            }
        });
    }

    pub(super) fn create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.project_id.read(cx).value().to_string();
        let Some(environment) = self.environment.read(cx).selected_value().cloned() else {
            self.fail("Select a configured Python, Node or Bun environment. Configure it in Environments first.".into(), cx);
            return;
        };
        let Some(environment) = self
            .environments
            .iter()
            .find(|item| item.spec.id == environment)
            .cloned()
        else {
            return;
        };
        self.run(window, cx, move |service| {
            let (definition, files) = example(&id, &environment.spec)?;
            let created = service.create(&definition, files)?;
            Ok(Outcome::Opened(Box::new(
                service.open(&created.definition.id)?,
            )))
        });
    }
}

pub(crate) fn example(
    id: &str,
    environment: &crate::contracts::EnvironmentSpec,
) -> anyhow::Result<(
    crate::project::ProjectDefinition,
    std::collections::BTreeMap<String, Vec<u8>>,
)> {
    let (entrypoint, source): (&str, &[u8]) = match environment.runtime {
        RuntimeKind::Python => ("main.py", include_bytes!("examples/main.py")),
        RuntimeKind::Node | RuntimeKind::Bun => ("main.mjs", include_bytes!("examples/main.mjs")),
        RuntimeKind::Shell => anyhow::bail!("Choose a Python, Node or Bun environment"),
    };
    let mut definition =
        crate::project::ProjectDefinition::parse(include_bytes!("../project/example-v1.json"))?;
    definition.id = id.into();
    definition.title = id.into();
    definition.environment = environment.id.clone();
    definition.entrypoint = entrypoint.into();
    definition.files = vec![entrypoint.into()];
    definition.validate()?;
    Ok((
        definition,
        std::collections::BTreeMap::from([(entrypoint.into(), source.to_vec())]),
    ))
}
