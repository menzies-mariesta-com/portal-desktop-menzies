//! Serial connect queue so OpenVPN and GlobalProtect elevates / kill-switch
//! mutations never race. Order follows activation order.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolSlot {
    OpenVpn,
    GlobalProtect,
}

type Job = Box<dyn FnOnce() + Send + 'static>;

struct QueueInner {
    running: bool,
    pending: VecDeque<(ProtocolSlot, Job)>,
}

#[derive(Clone)]
pub struct SerialConnectQueue {
    inner: Arc<Mutex<QueueInner>>,
}

impl Default for SerialConnectQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl SerialConnectQueue {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(QueueInner {
                running: false,
                pending: VecDeque::new(),
            })),
        }
    }

    /// Enqueue a connect/disconnect job for a protocol slot. Jobs run one at a time.
    pub fn enqueue<F>(&self, slot: ProtocolSlot, job: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(e) => {
                log::error!("connect queue lock poisoned: {e}");
                return;
            }
        };
        guard.pending.push_back((slot, Box::new(job)));
        if guard.running {
            return;
        }
        guard.running = true;
        drop(guard);
        self.spawn_worker();
    }

    fn spawn_worker(&self) {
        let queue = self.clone();
        thread::spawn(move || loop {
            let next = {
                let mut guard = match queue.inner.lock() {
                    Ok(g) => g,
                    Err(_) => return,
                };
                match guard.pending.pop_front() {
                    Some(job) => Some(job),
                    None => {
                        guard.running = false;
                        None
                    }
                }
            };
            let Some((slot, job)) = next else {
                return;
            };
            log::info!("serial connect queue running {:?}", slot);
            job();
        });
    }
}

pub fn new_connect_queue() -> SerialConnectQueue {
    SerialConnectQueue::new()
}
