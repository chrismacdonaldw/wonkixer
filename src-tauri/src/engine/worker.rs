use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use wonkixer_dsp::Params;

use super::devices;
use super::meters::MeterStore;
use super::streams::{self, Streams};

const METER_INTERVAL: Duration = Duration::from_millis(33);
const STATE_EVENT: &str = "engine:state";
const METERS_EVENT: &str = "engine:meters";

pub type StartReply = oneshot::Sender<Result<EngineState, String>>;
pub type StateReply = oneshot::Sender<EngineState>;

pub enum Command {
    Start {
        input_id: String,
        output_id: String,
        reply: StartReply,
    },
    Stop {
        reply: StateReply,
    },
    State {
        reply: StateReply,
    },
    StreamFailed {
        generation: u64,
        message: String,
    },
    Shutdown,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineState {
    pub running: bool,
    pub input: Option<Endpoint>,
    pub output: Option<Endpoint>,
    pub sample_rate: Option<u32>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
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
            Ok(Command::Start {
                input_id,
                output_id,
                reply,
            }) => {
                reply.send(worker.start(&input_id, &output_id)).ok();
            }
            Ok(Command::Stop { reply }) => {
                worker.stop();
                reply.send(worker.state.clone()).ok();
            }
            Ok(Command::State { reply }) => {
                reply.send(worker.state.clone()).ok();
            }
            Ok(Command::StreamFailed {
                generation,
                message,
            }) => worker.fail(generation, message),
            Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        worker.publish_meters();
    }
}

struct Live {
    _streams: Streams,
    sample_rate: u32,
    generation: u64,
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
        }
    }

    fn start(&mut self, input_id: &str, output_id: &str) -> Result<EngineState, String> {
        self.live = None;
        match self.open(input_id, output_id) {
            Ok(state) => {
                self.transition(state);
                Ok(self.state.clone())
            }
            Err(message) => {
                self.transition(EngineState {
                    error: Some(message.clone()),
                    ..EngineState::default()
                });
                Err(message)
            }
        }
    }

    fn open(&mut self, input_id: &str, output_id: &str) -> Result<EngineState, String> {
        let input = devices::find(&self.host, input_id)?;
        let output = devices::find(&self.host, output_id)?;
        let input_endpoint = Endpoint {
            id: input_id.to_owned(),
            name: devices::display_name(&input)?,
        };
        let output_endpoint = Endpoint {
            id: output_id.to_owned(),
            name: devices::display_name(&output)?,
        };

        self.generation += 1;
        let generation = self.generation;
        let failures = self.failures.clone();
        self.meters.reset();
        let (streams, sample_rate) = streams::open(
            &input,
            &output,
            Arc::clone(&self.meters),
            Arc::clone(&self.params),
            move |message| {
                failures
                    .send(Command::StreamFailed {
                        generation,
                        message,
                    })
                    .ok();
            },
        )?;

        self.live = Some(Live {
            _streams: streams,
            sample_rate,
            generation,
        });
        Ok(EngineState {
            running: true,
            input: Some(input_endpoint),
            output: Some(output_endpoint),
            sample_rate: Some(sample_rate),
            error: None,
        })
    }

    fn stop(&mut self) {
        if self.live.take().is_some() {
            self.transition(EngineState::default());
        }
    }

    fn fail(&mut self, generation: u64, message: String) {
        let current = self.live.as_ref().map(|live| live.generation);
        if current != Some(generation) {
            return;
        }
        self.live = None;
        self.transition(EngineState {
            error: Some(message),
            ..EngineState::default()
        });
    }

    fn transition(&mut self, state: EngineState) {
        self.state = state;
        self.app.emit(STATE_EVENT, &self.state).ok();
    }

    fn publish_meters(&self) {
        if let Some(live) = &self.live {
            self.app
                .emit(METERS_EVENT, self.meters.snapshot(live.sample_rate))
                .ok();
        }
    }
}
