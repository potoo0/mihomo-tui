use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use circular_buffer::CircularBuffer;

use crate::components::ComponentId;
use crate::models::{Connection, ConnectionStat, Memory, Traffic, Version};

const BUFFER_SIZE: usize = 100;
const CONNS_BUFFER_SIZE: usize = 500;

#[derive(Clone, Default)]
pub struct AppState {
    pub version: Option<Version>,
    pub memory: Arc<Mutex<CircularBuffer<BUFFER_SIZE, Memory>>>,
    pub traffic: Arc<Mutex<CircularBuffer<BUFFER_SIZE, Traffic>>>,
    pub conn_stat: Arc<Mutex<Option<ConnectionStat>>>,
    pub connections: Arc<Mutex<CircularBuffer<CONNS_BUFFER_SIZE, Connection>>>,

    pub focused: ComponentId,
    pub live_mode: Arc<AtomicBool>,
    pub filter_pattern: Arc<RwLock<Option<String>>>,
    pub ordering: Arc<RwLock<Option<(usize, bool)>>>,
}

impl AppState {
    pub fn new(live: bool) -> Self {
        Self {
            live_mode: Arc::new(AtomicBool::new(live)),
            ..Default::default()
        }
    }
}
