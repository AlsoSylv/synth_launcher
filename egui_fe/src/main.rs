use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
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
    selected_version: Option<usize>,
    version_json: Option<Arc<VersionJson>>,
    asset_index: Option<Arc<AssetIndexJson>>,
    total_libraries: Arc<AtomicUsize>,
    finished_libraries: Arc<AtomicUsize>,
    total_assets: Arc<AtomicUsize>,
    finished_assets: Arc<AtomicUsize>,
    class_path: String,
    jar_path: String,
    json_start: AtomicBool,
    assets: bool,
    libraries: bool,
    jar: bool,
    launching: bool,
}

impl Default for MCData {
    fn default() -> Self {
        Self {
            versions: None,
            selected_version: None,
            version_json: None,
            asset_index: None,
            total_libraries: Default::default(),
            finished_libraries: Default::default(),
            total_assets: Default::default(),
            finished_assets: Default::default(),
            class_path: Default::default(),
            jar_path: Default::default(),
            json_start: AtomicBool::new(true),
            versions_task_started: false,
            assets: false,
            libraries: false,
            jar: false,
            launching: false,
        }
    }
}

enum Message {
    Versions,
    Version(Arc<Version>),
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
                let versions = launcher_core
                    .get_version_manifest(&path.join("versions"))
                    .await;
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

        let rt = async_bridge::Runtime::new(
            4,
            State { launcher_core },
            cc.egui_ctx.clone(),
            worker_event_loop,
            rt,
        );

        LauncherGui {
            rt,
            data: Default::default(),
        }
    }

    fn update_state(&mut self, ctx: &egui::Context) {
        let event = self.rt.try_recv();
        if let Ok(message) = event {
            match message {
                Response::Versions(version) => match version {
                    Ok(versions) => self.data.versions = Some(versions),
                    Err(err) => {
                        println!("{err:?}");
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
                    Ok(json) => self.data.version_json = Some(Arc::new(json)),
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
                    Ok(path) => {
                        self.data.libraries = true;
                        self.data.class_path = path;
                    },
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
                        let index = Arc::new(json);

                        self.rt.send_with_message(Message::Assets(
                            index.clone(),
                            self.data.total_assets.clone(),
                            self.data.finished_assets.clone(),
                        ));

                        self.data.asset_index = Some(index)
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
                    Ok(()) => self.data.assets = true,
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

        let size = _frame.info().window_info.size;
        let width = size.x;
        let height = size.y;

        egui::SidePanel::left("General Paenl")
            .default_width(width * 0.1)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(versions) = &self.data.versions {
                    let text = if let Some(index) = self.data.selected_version {
                        &versions.versions[index].id
                    } else {
                        "None"
                    };

                    egui::ComboBox::from_id_source("VersionSelect")
                        .selected_text(text)
                        .show_ui(ui, |ui| {
                            versions
                                .versions
                                .iter()
                                .enumerate()
                                .for_each(|(index, version)| {
                                    let button = ui.selectable_value(
                                        &mut self.data.selected_version,
                                        Some(index),
                                        &version.id,
                                    );

                                    if button.clicked() {
                                        if let Some(json) = &self.data.version_json {
                                            if version.id != json.id() {
                                                self.data.json_start.store(true, Ordering::Relaxed);
                                            }
                                        }

                                        let start = self.data.json_start.load(Ordering::Relaxed);

                                        if start {
                                            self.data.json_start.store(false, Ordering::Relaxed);

                                            self.rt.send_with_message(Message::Version(
                                                version.clone(),
                                            ));
                                        }
                                    };
                                })
                        });
                } else {
                    ui.spinner();
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut style = Style::default();
            style.visuals.button_frame = true;

            ui.set_style(style);

            let total = self.data.total_libraries.load(Ordering::Relaxed);
            let finished = self.data.finished_libraries.load(Ordering::Relaxed);

            if total == 0 {
                ui.label("0 %");
            } else {
                ui.label(format!("{:.2} %", (finished as f64 / total as f64) * 100.0));
            }

            let button = Button::new("Play");

            let enabled = ui.add_enabled(self.data.version_json.is_some() && !self.data.launching, button);

            if enabled.clicked() {
                let unwrapped = &self.data.version_json;
                let version = unwrapped
                    .as_ref()
                    .expect("This button is only enabled if this exists");
                let libraries = version.libraries().clone();
                let index = version.asset_index();

                self.rt
                    .send_with_message(Message::AssetIndex(index.clone()));
                self.rt.send_with_message(Message::Libraries(
                    libraries,
                    self.data.total_libraries.clone(),
                    self.data.finished_libraries.clone(),
                ));
                self.rt.send_with_message(Message::Jar(version.clone()));
                self.data.launching = true;
            }

            if self.data.libraries && self.data.assets && self.data.jar && self.data.launching {
                self.data.launching = false;
                self.data.class_path.push_str(&self.data.jar_path);
                let json = self.data.version_json.as_ref().unwrap();
                let dir = Path::new("./");
                let class_path = &self.data.class_path;

                match json.as_ref() {
                    VersionJson::Modern(modern) => launcher_core::launch_modern_version(
                        modern,
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
                    ),
                    VersionJson::Legacy(_) => todo!(),
                };
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
