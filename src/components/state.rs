use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use circular_buffer::CircularBuffer;

use crate::components::ComponentId;
use crate::models::search_query::SearchQuery;
use crate::models::{Connection, ConnectionStat, Memory, Traffic, Version};

const BUFFER_SIZE: usize = 100;
const CONNS_BUFFER_SIZE: usize = 500;

#[derive(Debug, Clone, Default)]
pub struct AppState {
    pub version: Option<Version>,
    pub memory: Arc<Mutex<CircularBuffer<BUFFER_SIZE, Memory>>>,
    pub traffic: Arc<Mutex<CircularBuffer<BUFFER_SIZE, Traffic>>>,
    pub conn_stat: Arc<Mutex<Option<ConnectionStat>>>,
    pub connections: Arc<Mutex<CircularBuffer<CONNS_BUFFER_SIZE, Connection>>>,

    pub tab: Arc<RwLock<ComponentId>>,
    pub tab_state: Arc<RwLock<HashMap<ComponentId, TabState>>>,
    // pub live_mode: Arc<AtomicBool>,
    // pub filter_pattern: Arc<RwLock<Option<String>>>,
    // pub ordering: Arc<RwLock<Option<(usize, bool)>>>,
}

#[derive(Debug, Clone)]
pub struct TabState {
    pub live_mode: bool,
    pub search_query: SearchQuery,
}

impl Default for TabState {
    fn default() -> Self {
        Self { live_mode: true, search_query: SearchQuery::default() }
    }
}

impl AppState {
    pub fn new(tab_state: HashMap<ComponentId, TabState>) -> Self {
        Self { tab_state: Arc::new(RwLock::new(tab_state)), ..Default::default() }
    }
}
