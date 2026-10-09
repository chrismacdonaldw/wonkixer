pub mod devices;
mod meters;
mod streams;
mod transport;
mod worker;

use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use tauri::AppHandle;
use tokio::sync::oneshot;

use worker::Command;
pub use worker::EngineState;

const ENGINE_UNAVAILABLE: &str = "The audio engine is not running.";

pub struct Engine {
    commands: Sender<Command>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Engine {
    pub fn spawn(app: AppHandle) -> Self {
        let (commands, inbox) = mpsc::channel();
        let failures = commands.clone();
        let worker = thread::Builder::new()
            .name("audio-engine".to_owned())
            .spawn(move || worker::run(app, failures, inbox))
            .expect("failed to spawn the audio engine thread");
        Self {
            commands,
            worker: Mutex::new(Some(worker)),
        }
    }

    pub async fn start(&self, input_id: String, output_id: String) -> Result<EngineState, String> {
        let (reply, response) = oneshot::channel();
        self.send(Command::Start {
            input_id,
            output_id,
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
