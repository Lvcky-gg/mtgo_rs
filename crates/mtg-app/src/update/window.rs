//! The launch gate: application data is opened only after checking/updating finishes.
use std::sync::{
    Arc, Mutex,
    mpsc::{Receiver, TryRecvError},
};

use super::{Message, Restart};

pub type PendingRestart = Arc<Mutex<Option<Restart>>>;

pub struct Launch {
    receiver: Option<Receiver<Message>>,
    app: Option<crate::app::App>,
    status: String,
    error: Option<String>,
    restart: PendingRestart,
}

impl Launch {
    pub fn new(
        ctx: egui::Context,
        skip: bool,
        error: Option<String>,
        restart: PendingRestart,
    ) -> Self {
        Self {
            receiver: (!skip).then(|| super::start(ctx.clone())),
            app: skip.then(|| crate::app::App::new(ctx)),
            status: "Checking for updates…".into(),
            error,
            restart,
        }
    }
}

impl eframe::App for Launch {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        if let Some(receiver) = &self.receiver {
            loop {
                match receiver.try_recv() {
                    Ok(Message::Progress(status)) => self.status = status,
                    Ok(Message::Finished(Ok(Some(restart)))) => {
                        *self.restart.lock().expect("update restart lock") = Some(restart);
                        self.receiver = None;
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        return;
                    }
                    Ok(Message::Finished(result)) => {
                        self.error = result.err();
                        self.receiver = None;
                        self.app = Some(crate::app::App::new(ui.ctx().clone()));
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.error =
                            Some("Update check stopped; using the installed version.".into());
                        self.receiver = None;
                        self.app = Some(crate::app::App::new(ui.ctx().clone()));
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
        }
        if let Some(app) = &mut self.app {
            if let Some(error) = &self.error {
                egui::Panel::top("launch-update-error").show(ui, |ui| {
                    ui.label(format!(
                        "Could not update the client: {error}. Using the installed version."
                    ));
                });
            }
            app.ui(ui, frame);
        } else {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(80.0);
                    ui.heading("MTGO RS");
                    ui.spinner();
                    ui.label(&self.status);
                });
            });
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
