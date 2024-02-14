use crate::{types::*, validator::ValidationConfig};
use crossbeam_channel::Sender as SenderSync;
use futures_lite::future::poll_fn;
use std::{cell::RefCell, future::Future, pin::Pin, sync::Arc, task::Poll, time::Instant};

/// All state associated with a task (owned by the executor)
pub struct TaskState {
    pub api: AsyncValidatorApi,
    pub future: Pin<Box<dyn Future<Output = Result<GrpcRequest, String>>>>,
    pub started_at: Instant,
    pub request: RawGrpcRequest,
}

/// State available in both async tasks (for receiving results in the task future)
/// and (for setting results in the executor) 
pub struct AsyncValidatorApiInner {
    pub terrain_result: Option<TerrainResponse>,
}

/// State used to implement to implement async methods (owned by task future)
#[derive(Clone)]
pub struct AsyncValidatorApi {
    inner: Arc<RefCell<AsyncValidatorApiInner>>,
    global: Arc<GlobalApiState>,
    seq: usize,
}

/// Single instance of state shared between all tasks
pub struct GlobalApiState {
    pub config: ValidationConfig,
    pub terrain_request_tx: SenderSync<TerrainQuery>,
}

impl AsyncValidatorApi {
    pub fn new(global: Arc<GlobalApiState>, seq: usize) -> Self {
        Self {
            inner: Arc::new(RefCell::new(AsyncValidatorApiInner {
                terrain_result: None,
            })),
            global,
            seq,
        }
    }

    pub fn config(&self) -> &ValidationConfig {
        &self.global.config
    }

    pub async fn get_altitude(&self, lat: f64, lon: f64) -> Option<f64> {
        let mut sent = false;
        poll_fn(|_ctx| {
            if !sent {
                sent = true;
                //println!("Sending terrain request inside task {}", self.seq);
                let _ = self.global.terrain_request_tx.try_send(TerrainQuery {
                    lat,
                    lon,
                    seq: self.seq,
                });
            }
            self.with(|api| match api.terrain_result {
                Some(t) => Poll::Ready(Some(t.terrain.alt)),
                None => Poll::Pending,
            })
        })
        .await
    }

    pub fn with<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&AsyncValidatorApiInner) -> R,
    {
        let r = RefCell::borrow(&self.inner);
        f(&r)
    }

    pub fn with_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut AsyncValidatorApiInner) -> R,
    {
        let mut r = RefCell::borrow_mut(&self.inner);
        f(&mut r)
    }
}
