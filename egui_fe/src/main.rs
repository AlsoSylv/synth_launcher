use std::{
    path::Path,
    sync::{atomic::AtomicUsize, Arc},
};

use eframe::egui::{self as egui, Button, Style};
use launcher_core::{
    types::{AssetIndex, AssetIndexJson, Library, Version, VersionJson, VersionManifest},
    Error,
};

struct LauncherGui {
    rt: async_bridge::Runtime<Message, Response, State>,
    data: MCData,
}

struct MCData {
    versions: Option<VersionManifest>,
    versions_task_started: bool,
    selected_version: usize,
    version_json: Option<Arc<VersionJson>>,
    asset_index: Option<Arc<AssetIndexJson>>,
    total_libraries: Arc<AtomicUsize>,
    finished_libraries: Arc<AtomicUsize>,
    total_assets: Arc<AtomicUsize>,
    finished_assets: Arc<AtomicUsize>,
    class_path: String,
    waiting: bool,
    ready: bool,
    jar: bool,
    jar_path: String,
}

impl Default for MCData {
    fn default() -> Self {
        Self {
            versions: Default::default(),
            versions_task_started: Default::default(),
            selected_version: usize::MAX,
            version_json: Default::default(),
            asset_index: Default::default(),
            total_libraries: Default::default(),
            finished_libraries: Default::default(),
            total_assets: Default::default(),
            finished_assets: Default::default(),
            class_path: Default::default(),
            waiting: false,
            ready: false,
            jar: false,
            jar_path: Default::default(),
        }
    }
}

enum Message {
    Versions,
    Version(Version),
    AssetIndex(Arc<AssetIndex>),
    Libraries(Arc<[Library]>, Arc<AtomicUsize>, Arc<AtomicUsize>),
    Assets(Arc<AssetIndexJson>, Arc<AtomicUsize>, Arc<AtomicUsize>),
    Jar(Arc<VersionJson>),
}

enum Response {
    Versions(Result<VersionManifest, Error>),
    Version(Result<VersionJson, Error>),
    Libraries(Result<String, Error>),
    AssetIndex(Result<AssetIndexJson, Error>),
    Asset(Result<(), Error>),
    Jar(Result<String, Error>),
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
    let path = Path::new("./");
    async move {
        match message {
            Message::Versions => {
                let versions = launcher_core.get_version_manifest().await;
                Response::Versions(versions)
            }
            Message::Version(version) => {
                let json = launcher_core
                    .get_version_json(&version, &path.join("versions"))
                    .await;
                Response::Version(json)
            }
            Message::Libraries(libs, total, finished) => {
                let path = launcher_core
                    .download_libraries_and_get_path(
                        &libs,
                        &path.join("libraries"),
                        &total,
                        &finished,
                    )
                    .await;
                Response::Libraries(path)
            }
            Message::AssetIndex(asset_index) => {
                let index = launcher_core
                    .get_asset_index_json(&asset_index, &path.join("assets"))
                    .await;
                println!("Sent");
                Response::AssetIndex(index)
            }
            Message::Assets(index, total, finished) => {
                let result = launcher_core
                    .download_and_store_asset_index(&index, &path.join("assets"), &total, &finished)
                    .await;
                Response::Asset(result)
            }
            Message::Jar(json) => {
                let result = launcher_core
                    .download_jar(&json, &path.join("versions"))
                    .await;
                Response::Jar(result)
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
                Response::Version(version) => match version {
                    Ok(json) => {
                        let index = match &json {
                            VersionJson::Modern(json) => Arc::new(json.asset_index.clone()),
                            VersionJson::Legacy(json) => Arc::new(json.asset_index.clone()),
                            VersionJson::Ancient(json) => Arc::new(json.asset_index.clone()),
                        };

                        self.rt.send_with_message(Message::AssetIndex(index));
                        println!("recieved");
                        self.data.version_json = Some(Arc::new(json))
                    }
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
                Response::Libraries(result) => match result {
                    Ok(path) => self.data.class_path = path,
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
                Response::AssetIndex(idx) => match idx {
                    Ok(json) => {
                        println!("Recieved");
                        self.data.asset_index = Some(Arc::new(json));
                    }
                    Err(err) => {
                        println!("{:?}", err);
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
                Response::Asset(result) => match result {
                    Ok(()) => {
                        self.data.ready = true;
                    }
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
                Response::Jar(result) => {
                    self.data.jar = true;
                    self.data.jar_path = result.unwrap();
                }
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

        egui::SidePanel::left("General Panel")
            .default_width(60.0)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(versions) = &self.data.versions {
                    let text = if self.data.selected_version == usize::MAX {
                        "None"
                    } else {
                        &versions.versions[self.data.selected_version].id
                    };
                    egui::ComboBox::from_id_source("Version Box")
                        .width(100.0)
                        .selected_text(text)
                        .show_ui(ui, |ui| {
                            versions
                                .versions
                                .iter()
                                .enumerate()
                                .for_each(|(index, version)| {
                                    if ui.small_button(&version.id).clicked() {
                                        self.data.selected_version = index;
                                        let version =
                                            &versions.versions[self.data.selected_version];
                                        self.rt
                                            .send_with_message(Message::Version(version.clone()));
                                    };
                                    ui.separator();
                                })
                        });
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut style = Style::default();
            style.visuals.button_frame = true;

            ui.set_style(style);

            let total = self
                .data
                .total_libraries
                .load(std::sync::atomic::Ordering::Relaxed);
            let finished = self
                .data
                .finished_libraries
                .load(std::sync::atomic::Ordering::Relaxed);

            if total == 0 {
                ui.label("0 %");
            } else {
                ui.label(format!("{} %", (finished as f64 / total as f64) * 100.0));
            }

            let button = Button::new("Play");

            if ui.add_enabled(!self.data.waiting, button).clicked() {
                let unwrapped = &self.data.version_json;
                let unwrapped = unwrapped.as_ref().unwrap();
                let libraries = match unwrapped.as_ref() {
                    VersionJson::Modern(json) => json.libraries.clone(),
                    VersionJson::Legacy(json) => json.libraries.clone(),
                    VersionJson::Ancient(json) => json.libraries.clone(),
                };

                let index = self.data.asset_index.as_ref().unwrap().clone();
                self.rt.send_with_message(Message::Libraries(
                    libraries,
                    self.data.total_libraries.clone(),
                    self.data.finished_libraries.clone(),
                ));
                self.rt.send_with_message(Message::Assets(
                    index,
                    self.data.total_assets.clone(),
                    self.data.finished_assets.clone(),
                ));
                self.rt.send_with_message(Message::Jar(unwrapped.clone()));

                self.data.waiting = true;
            }

            if self.data.ready && self.data.jar {
                self.data.class_path.push_str(&self.data.jar_path);
                let json = self.data.version_json.as_ref().unwrap();
                let dir = Path::new("./");
                let class_path = &self.data.class_path;

                self.data.ready = false;

                launcher_core::AsyncLauncher::launch_game(
                    json,
                    dir,
                    &dir.join("assets"),
                    "Sylv",
                    "null",
                    "null",
                    "null",
                    "null",
                    &dir.join("libraries"),
                    "null",
                    "null",
                    class_path,
                )
            }
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
