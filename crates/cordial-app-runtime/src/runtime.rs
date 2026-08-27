use std::collections::BTreeMap;

use crate::error::AppError;
use crate::event::{AppEvent, AppId, AppSnapshot};
use crate::receipt::AppReceipt;

/// Deterministic application state machine over finalized Cordial events.
pub trait CordialApp {
    fn app_id(&self) -> &AppId;

    fn validate(&self, event: &AppEvent) -> Result<(), AppError>;

    fn apply(&mut self, event: AppEvent) -> Result<AppReceipt, AppError>;

    fn snapshot(&self) -> AppSnapshot;
}

/// In-memory application runtime.
///
/// This first skeleton only owns the app registry. Event processing, cursor
/// management, duplicate protection, and receipt storage are added in the next
/// implementation slice.
#[derive(Default)]
pub struct AppRuntime {
    apps: BTreeMap<AppId, Box<dyn CordialApp>>,
}

impl AppRuntime {
    pub fn new() -> Self {
        Self {
            apps: BTreeMap::new(),
        }
    }

    pub fn register_app(&mut self, app: Box<dyn CordialApp>) -> Option<Box<dyn CordialApp>> {
        self.apps.insert(app.app_id().clone(), app)
    }

    pub fn app_count(&self) -> usize {
        self.apps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.apps.is_empty()
    }
}
