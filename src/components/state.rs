use crate::models::Version;

#[derive(Default, Clone)]
pub struct AppState {
    pub version: Option<Version>,
    // selected_tab: ComponentId,
}
