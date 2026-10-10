mod capture;
pub mod devices;
mod meters;
mod processor;
mod streams;
mod supervisor;
mod transport;
mod worker;

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use tauri::AppHandle;
use tokio::sync::oneshot;
use wonkixer_dsp::analysis::Take;
use wonkixer_dsp::{ParamValues, Params};

use supervisor::Route;
use worker::Command;
pub use worker::EngineState;

const ENGINE_UNAVAILABLE: &str = "The audio engine is not running.";

pub struct Engine {
    commands: Sender<Command>,
    params: Arc<Params>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    pub fn spawn(app: AppHandle) -> Self {
        let (commands, inbox) = mpsc::channel();
        let failures = commands.clone();
        let params = Arc::new(Params::new(&ParamValues::default()));
        let shared = Arc::clone(&params);
        let worker = thread::Builder::new()
            .name("audio-engine".to_owned())
            .spawn(move || worker::run(app, failures, inbox, shared))
            .expect("failed to spawn the audio engine thread");
        Self {
            commands,
            params,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub fn params(&self) -> &Params {
        &self.params
    }

    pub async fn start(&self, input_id: String, output_id: String) -> Result<EngineState, String> {
        let (reply, response) = oneshot::channel();
        self.send(Command::Start {
            route: Route {
                input_id,
                output_id,
            },
            reply,
        })?;
        response.await.map_err(|_| ENGINE_UNAVAILABLE.to_owned())?
    }

    pub async fn stop(&self) -> Result<EngineState, String> {
        let (reply, response) = oneshot::channel();
        self.send(Command::Stop { reply })?;
        response.await.map_err(|_| ENGINE_UNAVAILABLE.to_owned())
    }

    pub async fn state(&self) -> Result<EngineState, String> {
        let (reply, response) = oneshot::channel();
        self.send(Command::State { reply })?;
        response.await.map_err(|_| ENGINE_UNAVAILABLE.to_owned())
    }

    pub async fn set_supervision(&self, enabled: bool) -> Result<EngineState, String> {
        let (reply, response) = oneshot::channel();
        self.send(Command::Supervise { enabled, reply })?;
        response.await.map_err(|_| ENGINE_UNAVAILABLE.to_owned())
    }

    pub async fn begin_capture(&self, limit_seconds: u32) -> Result<(), String> {
        let (reply, response) = oneshot::channel();
        self.send(Command::BeginCapture {
            limit_seconds,
            reply,
        })?;
        response.await.map_err(|_| ENGINE_UNAVAILABLE.to_owned())?
    }

    pub async fn end_capture(&self) -> Result<Take, String> {
        let (reply, response) = oneshot::channel();
        self.send(Command::EndCapture { reply })?;
        response.await.map_err(|_| ENGINE_UNAVAILABLE.to_owned())?
    }

    pub fn shutdown(&self) {
        self.commands.send(Command::Shutdown).ok();
        let worker = self.worker.lock().ok().and_then(|mut slot| slot.take());
        if let Some(worker) = worker {
            worker.join().ok();
        }
    }

    fn send(&self, command: Command) -> Result<(), String> {
        self.commands
            .send(command)
            .map_err(|_| ENGINE_UNAVAILABLE.to_owned())
    }
}
