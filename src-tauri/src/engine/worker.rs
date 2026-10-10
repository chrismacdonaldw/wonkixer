use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use wonkixer_dsp::Params;
use wonkixer_dsp::analysis::Take;

use super::capture::Sink;
use super::devices::{self, Direction};
use super::meters::MeterStore;
use super::streams::{self, StreamFailure, Streams};
use super::supervisor::{Route, Supervisor};

pub const STATE_EVENT: &str = "engine:state";
const METER_INTERVAL: Duration = Duration::from_millis(33);
const METERS_EVENT: &str = "engine:meters";
const ENGINE_STOPPED: &str = "Start the engine before recording a take.";

pub type StartReply = oneshot::Sender<Result<EngineState, String>>;
pub type StateReply = oneshot::Sender<EngineState>;
pub type CaptureReply = oneshot::Sender<Result<(), String>>;
pub type TakeReply = oneshot::Sender<Result<Take, String>>;

pub enum Command {
    Start {
        route: Route,
        reply: StartReply,
    },
    Stop {
        reply: StateReply,
    },
    State {
        reply: StateReply,
    },
    Supervise {
        enabled: bool,
        reply: StateReply,
    },
    BeginCapture {
        limit_seconds: u32,
        reply: CaptureReply,
    },
    EndCapture {
        reply: TakeReply,
    },
    StreamFailed {
        generation: u64,
        failure: StreamFailure,
    },
    Shutdown,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineState {
    pub running: bool,
    pub input: Option<Endpoint>,
    pub output: Option<Endpoint>,
    pub sample_rate: Option<u32>,
    pub error: Option<String>,
    pub waiting_for_devices: bool,
}

impl EngineState {
    fn failed(message: String) -> Self {
        Self {
            error: Some(message),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Endpoint {
    pub id: String,
    pub name: String,
}

pub fn run(
    app: AppHandle,
    failures: Sender<Command>,
    inbox: Receiver<Command>,
    params: Arc<Params>,
) {
    let mut worker = Worker::new(app, failures, params);
    loop {
        match inbox.recv_timeout(METER_INTERVAL) {
            Ok(Command::Start { route, reply }) => {
                reply.send(worker.start(route)).ok();
            }
            Ok(Command::Stop { reply }) => {
                worker.stop();
                reply.send(worker.state.clone()).ok();
            }
            Ok(Command::State { reply }) => {
                reply.send(worker.state.clone()).ok();
            }
            Ok(Command::Supervise { enabled, reply }) => {
                worker.supervise(enabled);
                reply.send(worker.state.clone()).ok();
            }
            Ok(Command::BeginCapture {
                limit_seconds,
                reply,
            }) => {
                reply.send(worker.begin_capture(limit_seconds)).ok();
            }
            Ok(Command::EndCapture { reply }) => {
                reply.send(worker.end_capture()).ok();
            }
            Ok(Command::StreamFailed {
                generation,
                failure,
            }) => worker.fail(generation, failure),
            Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => worker.recover(),
        }
        worker.drain_capture();
        worker.publish_meters();
    }
}

struct Live {
    _streams: Streams,
    input: Endpoint,
    output: Endpoint,
    sample_rate: u32,
    generation: u64,
    capture: Sink,
}

impl Live {
    fn endpoint(&self, direction: Direction) -> &Endpoint {
        match direction {
            Direction::Input => &self.input,
            Direction::Output => &self.output,
        }
    }
}

struct OpenFailure {
    message: String,
    device_missing: bool,
}

impl OpenFailure {
    fn missing(direction: Direction) -> Self {
        Self {
            message: format!("The {} is not connected.", direction.noun()),
            device_missing: true,
        }
    }
}

impl From<String> for OpenFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            device_missing: false,
        }
    }
}

struct Worker {
    app: AppHandle,
    failures: Sender<Command>,
    host: cpal::Host,
    meters: Arc<MeterStore>,
    params: Arc<Params>,
    live: Option<Live>,
    state: EngineState,
    generation: u64,
    supervisor: Supervisor,
}

impl Worker {
    fn new(app: AppHandle, failures: Sender<Command>, params: Arc<Params>) -> Self {
        Self {
            app,
            failures,
            host: cpal::default_host(),
            meters: Arc::new(MeterStore::default()),
            params,
            live: None,
            state: EngineState::default(),
            generation: 0,
            supervisor: Supervisor::default(),
        }
    }

    fn start(&mut self, route: Route) -> Result<EngineState, String> {
        self.live = None;
        match self.open(&route) {
            Ok(state) => {
                self.supervisor.remember(route);
                self.transition(state);
                Ok(self.state.clone())
            }
            Err(failure) => {
                if failure.device_missing {
                    self.supervisor.remember(route);
                } else {
                    self.supervisor.forget();
                }
                self.transition(EngineState::failed(failure.message.clone()));
                Err(failure.message)
            }
        }
    }

    fn recover(&mut self) {
        let Some(route) = self.supervisor.tick(self.live.is_some()).cloned() else {
            return;
        };
        let connected = devices::is_connected(&self.host, &route.input_id)
            && devices::is_connected(&self.host, &route.output_id);
        if !connected {
            return;
        }
        let state = self
            .open(&route)
            .unwrap_or_else(|failure| EngineState::failed(failure.message));
        self.transition(state);
    }

    fn open(&mut self, route: &Route) -> Result<EngineState, OpenFailure> {
        let input = self.locate(&route.input_id, Direction::Input)?;
        let output = self.locate(&route.output_id, Direction::Output)?;
        let input_endpoint = Endpoint {
            id: route.input_id.clone(),
            name: devices::display_name(&input)?,
        };
        let output_endpoint = Endpoint {
            id: route.output_id.clone(),
            name: devices::display_name(&output)?,
        };

        self.generation += 1;
        let generation = self.generation;
        let failures = self.failures.clone();
        self.meters.reset();
        let opened = streams::open(
            &input,
            &output,
            Arc::clone(&self.meters),
            Arc::clone(&self.params),
            move |failure| {
                failures
                    .send(Command::StreamFailed {
                        generation,
                        failure,
                    })
                    .ok();
            },
        )?;

        let state = EngineState {
            running: true,
            input: Some(input_endpoint.clone()),
            output: Some(output_endpoint.clone()),
            sample_rate: Some(opened.sample_rate),
            error: None,
            waiting_for_devices: false,
        };
        self.live = Some(Live {
            _streams: opened.streams,
            input: input_endpoint,
            output: output_endpoint,
            sample_rate: opened.sample_rate,
            generation,
            capture: opened.capture,
        });
        Ok(state)
    }

    fn locate(&self, id: &str, direction: Direction) -> Result<cpal::Device, OpenFailure> {
        devices::find(&self.host, id).map_err(|_| OpenFailure::missing(direction))
    }

    fn stop(&mut self) {
        self.supervisor.forget();
        self.live = None;
        self.transition(EngineState::default());
    }

    fn supervise(&mut self, enabled: bool) {
        self.supervisor.set_enabled(enabled);
        self.transition(self.state.clone());
    }

    fn fail(&mut self, generation: u64, failure: StreamFailure) {
        let Some(live) = self.live.take_if(|live| live.generation == generation) else {
            return;
        };
        let message = failure.describe(&live.endpoint(failure.direction).name);
        drop(live);
        self.transition(EngineState::failed(message));
    }

    fn begin_capture(&mut self, limit_seconds: u32) -> Result<(), String> {
        self.live_mut()?.capture.begin(limit_seconds)
    }

    fn end_capture(&mut self) -> Result<Take, String> {
        self.live_mut()?.capture.end()
    }

    fn drain_capture(&mut self) {
        if let Some(live) = &mut self.live {
            live.capture.drain();
        }
    }

    fn live_mut(&mut self) -> Result<&mut Live, String> {
        self.live.as_mut().ok_or_else(|| ENGINE_STOPPED.to_owned())
    }

    fn transition(&mut self, state: EngineState) {
        let state = EngineState {
            waiting_for_devices: self.supervisor.is_waiting(self.live.is_some()),
            ..state
        };
        if state != self.state {
            self.state = state;
            self.app.emit(STATE_EVENT, &self.state).ok();
        }
    }

    fn publish_meters(&self) {
        if let Some(live) = &self.live {
            self.app
                .emit(METERS_EVENT, self.meters.snapshot(live.sample_rate))
                .ok();
        }
    }
}
