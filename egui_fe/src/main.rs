use eframe::{
    egui::{self as egui, Style},
    epaint::Rounding,
};
use launcher_core::{types::VersionManifest, Error};

struct LauncherGui {
    rt: async_bridge::Runtime<Message, Response, State>,
    data: MCData,
}

#[derive(Default)]
struct MCData {
    versions: Option<VersionManifest>,
    versions_task_started: bool,
    selected_version: usize,
}

enum Message {
    Versions,
}

enum Response {
    Versions(Result<VersionManifest, Error>),
}

#[derive(Clone)]
struct State {
    launcher_core: launcher_core::AsyncLauncher,
}

fn worker_event_loop(
    message: Message,
    state: &State,
) -> impl std::future::Future<Output = Response> {
    let launcher_core = state.launcher_core.clone();
    async move {
        match message {
            Message::Versions => {
                let versions = launcher_core.get_version_manifest().await;
                Response::Versions(versions)
            }
        }
    }
}

impl LauncherGui {
    fn new(cc: &eframe::CreationContext) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .build()
            .expect("Runtime Failed to Build");

        let client = reqwest::Client::new();
        let launcher_core = launcher_core::AsyncLauncher::new(client.clone());

        LauncherGui {
            rt: async_bridge::Runtime::new(
                4,
                State { launcher_core },
                cc.egui_ctx.clone(),
                worker_event_loop,
                rt,
            ),
            data: Default::default(),
        }
    }

    fn update_state(&mut self, ctx: &egui::Context) {
        if let Ok(message) = self.rt.try_recv() {
            match message {
                Response::Versions(version) => match version {
                    Ok(versions) => self.data.versions = Some(versions),
                    Err(err) => {
                        egui::Window::new("Error Window").show(ctx, |ui| {
                            ui.label(err.to_string());
                            if let Error::Reqwest(_) = err {
                                if ui.button("Retry").clicked() {
                                    self.data.versions_task_started = false;
                                }
                            }
                        });
                    }
                },
            }
        }
    }
}

impl eframe::App for LauncherGui {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.update_state(ctx);
        if !self.data.versions_task_started {
            self.rt.send_with_message(Message::Versions);
            self.data.versions_task_started = true;
        }

        egui::CentralPanel::default().show(ctx, |_| {
            egui::SidePanel::left("General Panel")
                .default_width(60.0)
                .resizable(false)
                .show(ctx, |ui| {
                    if let Some(versions) = &self.data.versions {
                        egui::ComboBox::from_id_source("Version Box")
                            .width(100.0)
                            .selected_text(&versions.versions[self.data.selected_version].id)
                            .show_ui(ui, |ui| {
                                versions
                                    .versions
                                    .iter()
                                    .enumerate()
                                    .for_each(|(index, version)| {
                                        ui.style_mut().visuals.button_frame = false;
                                        ui.style_mut().visuals.menu_rounding =
                                            Rounding::none().at_least(45.0).at_most(45.0);

                                        if ui.small_button(&version.id).clicked() {
                                            self.data.selected_version = index;
                                        };
                                        ui.separator();

                                        ui.style_mut().visuals.button_frame = true;
                                    })
                            });
                    }
                });

            egui::CentralPanel::default().show(ctx, |ui| {
                let mut style = Style::default();
                style.visuals.button_frame = true;

                ui.set_style(style);

                if ui.button("Play").clicked() {}
            });
        });
    }
}

fn main() {
    eframe::run_native(
        "Test App",
        eframe::NativeOptions::default(),
        Box::new(|cc| Box::new(LauncherGui::new(cc))),
    )
    .unwrap();
}
