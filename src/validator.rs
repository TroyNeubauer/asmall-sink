use crate::runtime::*;
use crate::types::*;
use crossbeam_channel::Sender as SenderSync;
use std::borrow::Borrow;

use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;
use std::time::Instant;
use std::{
    collections::{HashMap, VecDeque},
    future::Future,
};

pub struct ValidationConfig {
    min_guided_agl: f64,
    max_guided_agl: f64,
    max_async_wait: Duration,
    validate_terrain: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            min_guided_agl: 10.0,
            max_guided_agl: 180.0,
            max_async_wait: Duration::from_millis(500),
            validate_terrain: true,
        }
    }
}

pub type GrpcError = String;

pub struct GrpcValidator {
    seq: usize,
    validated: VecDeque<Result<GrpcRequest, GrpcError>>,
    tasks: HashMap<usize, TaskState>,
    config: &'static ValidationConfig,
    ctx: Context<'static>,
    global_api_state: Arc<GlobalApiState>,
}

impl GrpcValidator {
    pub fn new(terrain_request_tx: SenderSync<TerrainQuery>) -> Self {
        // our wakeups are handled by the caller using `update_terrain` to indicate new data is
        // avaibale, so we have control of knowing when wakeups happen.
        // This means than calling `await` on any future other than the async methods in
        // `AsyncValidatorApi` will block forever
        let waker = Box::leak(Box::new(noop_waker::noop_waker()));
        let ctx = Context::from_waker(waker);
        let config = Default::default();

        Self {
            seq: 0,
            validated: Default::default(),
            tasks: Default::default(),
            config: Box::leak(Box::default()),
            ctx,
            global_api_state: Arc::new(GlobalApiState {
                config,
                terrain_request_tx,
            }),
        }
    }

    pub fn update_terrain(&mut self, terrain: &TerrainResponse) {
        if let Some(()) = self.tasks.get_mut(&terrain.seq).map(|t| {
            //println!("Updating TERRAIN");
            t.api.with_mut(|api| {
                //println!("Storing terrain {terrain:?} for task {}", terrain.seq);
                api.terrain_result = Some(*terrain);
            });
        }) {
            // poll task if we were able to update it
            self.poll_task(terrain.seq)
        }
    }

    pub fn insert(&mut self, request: RawGrpcRequest) {
        if !is_request_async(&request) {
            let validated = match request {
                RawGrpcRequest::Basic => Some(inner::validate_basic(())),
                RawGrpcRequest::FlyTo(_) => unreachable!(),
                RawGrpcRequest::Waypoint(_) => unreachable!(),
            };

            if let Some(v) = validated {
                self.validated.push_back(v);
            }
        }

        if is_request_async(&request) {
            let seq = self.seq;
            self.seq += 1;

            let api = AsyncValidatorApi::new(Arc::clone(&self.global_api_state), seq);
            let api2 = api.clone();

            let future: Pin<Box<dyn Future<Output = Result<GrpcRequest, String>>>> =
                match request.clone() {
                    RawGrpcRequest::FlyTo(lla) => Box::pin(Self::validate_fly_to(api, lla)),
                    RawGrpcRequest::Waypoint(v) => Box::pin(Self::validate_waypoints(api, v)),
                    RawGrpcRequest::Basic => unreachable!(),
                };
            let state = TaskState {
                api: api2,
                future,
                started_at: Instant::now(),
                request,
            };

            let old = self.tasks.insert(seq, state);
            assert!(old.is_none());

            // poll task immediately in case it can make progress
            self.poll_task(seq);
        }
    }

    /// Called every tick to handle timing out old requests.
    pub fn tick(&mut self) {
        self.tasks.retain(|_, r| {
            let r: &TaskState = r.borrow();
            if r.started_at.elapsed() > self.config.max_async_wait {
                //println!("Dropping async task: {:?}", r.request);
                false
            } else {
                true
            }
        });
    }

    /// Takes the next request off of the internal queue for processing.
    pub fn take_validated(&mut self) -> Option<Result<GrpcRequest, GrpcError>> {
        self.validated.pop_front()
    }

    fn poll_task(&mut self, seq: usize) {
        if let Some(t) = self.tasks.get_mut(&seq) {
            match Future::poll(t.future.as_mut(), &mut self.ctx) {
                Poll::Pending => {}
                Poll::Ready(r) => {
                    self.validated.push_front(r);
                    self.tasks.remove(&seq);
                }
            }
        }
    }

    async fn validate_fly_to(api: AsyncValidatorApi, pos: LLA) -> Result<GrpcRequest, String> {
        if !api.config().validate_terrain {
            return Ok(GrpcRequest::FlyTo(pos));
        }

        match api.get_altitude(pos.lat, pos.lon).await {
            Some(terrain_height) => {
                check_lla(pos, terrain_height, api.config())?;
                Ok(GrpcRequest::FlyTo(pos))
            }
            None => Ok(GrpcRequest::FlyTo(pos)),
        }
    }

    async fn validate_waypoints(api: AsyncValidatorApi, r: Vec<LLA>) -> Result<GrpcRequest, String> {
        if !api.config().validate_terrain {
            return Ok(GrpcRequest::Waypoint(r));
        }

        for pos in r.iter().copied() {
            match api.get_altitude(pos.lat, pos.lon).await {
                Some(terrain_height) => {
                    check_lla(pos, terrain_height, api.config())?;
                }
                None => {}
            }
        }

        Ok(GrpcRequest::Waypoint(r))
    }
}

fn check_lla(pos: LLA, terrain_height: f64, config: &ValidationConfig) -> Result<(), String> {
    let agl = pos.alt - terrain_height;
    if agl < config.min_guided_agl {
        return Err("Destination too close to ground".to_string());
    }

    if agl > config.max_guided_agl {
        return Err("Destination too far to ground".to_string());
    }
    Ok(())
}

fn is_request_async(r: &RawGrpcRequest) -> bool {
    match r {
        RawGrpcRequest::Basic => false,
        RawGrpcRequest::FlyTo(_) => true,
        RawGrpcRequest::Waypoint(_) => true,
    }
}

mod inner {
    use super::*;

    pub fn validate_basic(_r: ()) -> Result<GrpcRequest, String> {
        Ok(GrpcRequest::Basic)
    }
}
