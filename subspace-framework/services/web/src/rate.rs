use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const CAPACITY: u32 = 10;
const REFILL_INTERVAL: Duration = Duration::from_secs(6);

#[derive(Clone)]
pub(super) struct Limits {
    process: Arc<Mutex<Bucket>>,
    session: Arc<Mutex<Bucket>>,
}

impl Limits {
    pub fn shared() -> Self {
        static PROCESS: OnceLock<Arc<Mutex<Bucket>>> = OnceLock::new();
        Self {
            process: PROCESS
                .get_or_init(|| Arc::new(Mutex::new(Bucket::new())))
                .clone(),
            session: Arc::new(Mutex::new(Bucket::new())),
        }
    }

    #[cfg(test)]
    pub fn isolated() -> Self {
        Self {
            process: Arc::new(Mutex::new(Bucket::new())),
            session: Arc::new(Mutex::new(Bucket::new())),
        }
    }

    pub fn take(&self) -> Result<(), String> {
        let mut process = lock(&self.process);
        let mut session = lock(&self.session);
        if !process.take() || !session.take() {
            Err("web_read rate limit exceeded".into())
        } else {
            Ok(())
        }
    }
}

struct Bucket {
    tokens: u32,
    updated: Instant,
}

impl Bucket {
    fn new() -> Self {
        Self {
            tokens: CAPACITY,
            updated: Instant::now(),
        }
    }

    fn take(&mut self) -> bool {
        let refill = (self.updated.elapsed().as_secs() / REFILL_INTERVAL.as_secs()) as u32;
        if refill > 0 {
            self.tokens = (self.tokens + refill).min(CAPACITY);
            self.updated = Instant::now();
        }
        if self.tokens == 0 {
            false
        } else {
            self.tokens -= 1;
            true
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
