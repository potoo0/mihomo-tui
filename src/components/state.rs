use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use circular_buffer::CircularBuffer;

use crate::models::{Connection, ConnectionStat, Memory, Traffic, Version};

const BUFFER_SIZE: usize = 100;
const CONNS_BUFFER_SIZE: usize = 500;

#[derive(Default, Clone)]
pub struct AppState {
    pub version: Option<Version>,
    pub memory: Arc<Mutex<CircularBuffer<BUFFER_SIZE, Memory>>>,
    pub traffic: Arc<Mutex<CircularBuffer<BUFFER_SIZE, Traffic>>>,
    pub conn_stat: Arc<Mutex<Option<ConnectionStat>>>,
    pub connections: Arc<Mutex<CircularBuffer<CONNS_BUFFER_SIZE, Connection>>>,
}
