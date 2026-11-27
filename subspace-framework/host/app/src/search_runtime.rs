//! Product-scoped search initialization for GUI and headless tool consumers.
use std::time::Duration;

use gpui_kit::{App, AppContext as _, Global, Task};
use subspace_agent_tools::ToolEnvironment;
use subspace_product::ProductContext;
use subspace_search::{Policy, Service};

use crate::product::ProductFiles;
use crate::registry::BoardRegistration;

const FIRST_SWEEP_DELAY: Duration = Duration::from_secs(600);

/// A product supplies its optional model implementation without coupling other products to it.
pub type RerankerFactory = fn(std::path::PathBuf) -> std::sync::Arc<dyn subspace_search::Reranker>;

pub(crate) fn initialize(
    environment: &mut ToolEnvironment,
    context: &ProductContext,
    boards: &[BoardRegistration],
    files: &ProductFiles,
    reranker: Option<RerankerFactory>,
) -> Result<(), String> {
    let allowed: Vec<String> = boards
        .iter()
        .filter(|board| board.searchable)
        .map(|board| board.id.to_owned())
        .collect();
    if allowed.is_empty() {
        return Ok(());
    }
    let root = files
        .state
        .parent()
        .ok_or("Cannot locate private search storage.")?
        .join("search");
    let service = Service::open(
        root.clone(),
        context.product.binary_name.as_str().to_owned(),
        allowed,
    )?;
    offer_defaults(&service, boards);
    if let Some(create) = reranker {
        service.set_reranker(create(root.join("models")));
    }
    environment.search = Some(service);
    Ok(())
}

/// Turns on the store of each board that declares it on by default, until
/// the person chooses in the Search view.
fn offer_defaults(service: &Service, boards: &[BoardRegistration]) {
    let stored = Policy {
        enabled: true,
        ..Policy::default()
    };
    for board in boards
        .iter()
        .filter(|board| board.searchable && board.settings.store_by_default)
    {
        let _ = service.offer_default(board.id, stored.clone());
    }
}

struct Maintenance {
    _task: Task<()>,
}
impl Global for Maintenance {}

pub(crate) fn start(cx: &mut App) {
    let Some(service) = subspace_dashboard::full_text_service(cx) else {
        return;
    };
    let task = cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(FIRST_SWEEP_DELAY).await;
            let service = service.clone();
            let _ = cx.background_spawn(async move { service.sweep() }).await;
        }
    });
    cx.set_global(Maintenance { _task: task });
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use gpui_kit::{App, Window};
    use subspace_agent_tools::{AgentTool, ToolEnvironment, test_support};
    use subspace_core::config::Config;
    use subspace_dashboard::DashboardHandle;
    use subspace_dashboard::settings::BoardSettings;
    use subspace_product::{OsKind, PlatformEnv, ProductContext, fixture};

    use super::{FIRST_SWEEP_DELAY, initialize};
    use crate::product::ProductFiles;
    use crate::registry::BoardRegistration;

    fn context(root: &Path) -> ProductContext {
        let known = fixture::products().unwrap();
        let env = PlatformEnv {
            os: OsKind::Macos,
            home: root.to_owned(),
            xdg_config_home: None,
            xdg_state_home: None,
            app_data: None,
            local_app_data: None,
        };
        ProductContext::new(known[0].clone(), env)
    }

    fn no_gui(_: &mut Window, _: &mut App) -> DashboardHandle {
        panic!("Search initialization must not construct a board")
    }

    fn no_tools(_: &ToolEnvironment) -> Vec<std::sync::Arc<dyn AgentTool>> {
        panic!("Search initialization must not fetch board tools")
    }

    fn board() -> BoardRegistration {
        BoardRegistration {
            id: "irc",
            title: "Channels".into(),
            description: "IRC networks, channels and private messages".into(),
            icon: None,
            help: "",
            help_images: &[],
            searchable: true,
            create: std::sync::Arc::new(no_gui),
            tools: no_tools,
            settings: BoardSettings::default(),
            settings_editor: None,
            fields: None,
            start_background: |_| panic!("Headless initialization must not start monitoring"),
            stop_background: |_| panic!("Headless initialization must not stop monitoring"),
            retain_when_disabled: false,
            configuration: None,
            default_order: usize::MAX,
            artwork: subspace_dashboard::BoardArtwork::default(),
            connection: Default::default(),
            contracts: Default::default(),
        }
    }

    #[test]
    fn headless_search_uses_the_product_allowlist_without_enabling_data_collection() {
        let root = tempfile::tempdir().unwrap();
        let context = context(root.path());
        let files = ProductFiles::for_context(&context);
        let mut environment = test_support::environment(Config::default());
        initialize(&mut environment, &context, &[board()], &files, None).unwrap();
        let service = environment.search.unwrap();
        assert_eq!(service.app_id(), context.product.binary_name.as_str());
        assert!(!service.policy("irc").unwrap().enabled);
        assert!(service.policy("github").is_err());
        assert_eq!(service.sources().unwrap()[0].records, 0);
        assert!(
            service
                .reranker_status()
                .is_none_or(|status| !status.installed)
        );
        assert_eq!(FIRST_SWEEP_DELAY.as_secs(), 600);
    }

    #[test]
    fn a_board_stored_by_default_starts_on_until_the_person_chooses() {
        let root = tempfile::tempdir().unwrap();
        let context = context(root.path());
        let files = ProductFiles::for_context(&context);
        let mut stored = board();
        stored.settings.store_by_default = true;
        let mut environment = test_support::environment(Config::default());
        initialize(&mut environment, &context, &[stored.clone()], &files, None).unwrap();
        let service = environment.search.unwrap();
        assert!(service.policy("irc").unwrap().enabled);
        service
            .set_policy("irc", subspace_search::Policy::default())
            .unwrap();
        drop(service);
        let mut environment = test_support::environment(Config::default());
        initialize(&mut environment, &context, &[stored], &files, None).unwrap();
        assert!(!environment.search.unwrap().policy("irc").unwrap().enabled);
    }

    #[test]
    fn nonparticipating_products_do_not_create_search_storage() {
        let root = tempfile::tempdir().unwrap();
        let context = context(root.path());
        let files = ProductFiles::for_context(&context);
        let mut environment = test_support::environment(Config::default());
        initialize(
            &mut environment,
            &context,
            &[],
            &files,
            Some(|_| panic!("A nonparticipating product must not construct a model")),
        )
        .unwrap();
        assert!(environment.search.is_none());
        assert!(!root.path().join("Library").exists());
    }
}
