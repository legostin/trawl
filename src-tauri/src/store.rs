use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::model::Flow;

#[derive(Clone)]
pub struct FlowStore {
    inner: Arc<Inner>,
}

struct Inner {
    flows: Mutex<VecDeque<Flow>>,
    capacity: usize,
    counter: AtomicU64,
}

impl FlowStore {
    pub fn new(capacity: usize) -> FlowStore {
        FlowStore {
            inner: Arc::new(Inner {
                flows: Mutex::new(VecDeque::new()),
                capacity,
                counter: AtomicU64::new(0),
            }),
        }
    }

    pub fn next_id(&self) -> u64 {
        self.inner.counter.fetch_add(1, Ordering::SeqCst) + 1
    }

    pub fn insert(&self, flow: Flow) {
        let mut q = self.inner.flows.lock().unwrap();
        if q.len() >= self.inner.capacity {
            q.pop_front();
        }
        q.push_back(flow);
    }

    pub fn update<F: FnOnce(&mut Flow)>(&self, id: u64, f: F) -> bool {
        let mut q = self.inner.flows.lock().unwrap();
        if let Some(flow) = q.iter_mut().find(|x| x.id == id) {
            f(flow);
            true
        } else {
            false
        }
    }

    pub fn all(&self) -> Vec<Flow> {
        self.inner.flows.lock().unwrap().iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Flow, HttpMessage, UrlParts};

    fn sample(id: u64) -> Flow {
        Flow::new_request(
            id,
            "GET".into(),
            UrlParts { scheme: "http".into(), host: "h".into(), port: 80, path: "/".into() },
            HttpMessage { headers: vec![], body: vec![], body_is_text: true },
        )
    }

    #[test]
    fn next_id_is_monotonic() {
        let s = FlowStore::new(10);
        assert_eq!(s.next_id(), 1);
        assert_eq!(s.next_id(), 2);
    }

    #[test]
    fn insert_and_all_roundtrip() {
        let s = FlowStore::new(10);
        s.insert(sample(1));
        s.insert(sample(2));
        assert_eq!(s.all().len(), 2);
    }

    #[test]
    fn ring_limit_evicts_oldest() {
        let s = FlowStore::new(2);
        s.insert(sample(1));
        s.insert(sample(2));
        s.insert(sample(3));
        let ids: Vec<u64> = s.all().iter().map(|f| f.id).collect();
        assert_eq!(ids, vec![2, 3]);
    }

    #[test]
    fn update_mutates_existing() {
        let s = FlowStore::new(10);
        s.insert(sample(1));
        let ok = s.update(1, |f| f.method = "POST".into());
        assert!(ok);
        assert_eq!(s.all()[0].method, "POST");
        assert!(!s.update(999, |_| {}));
    }
}
