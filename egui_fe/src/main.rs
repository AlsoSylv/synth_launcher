mod worker_logic;
mod wrappers;

use worker_logic::*;
use wrappers::*;

use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use eframe::egui::{self, Button, Label, Sense, Style};
use launcher_core::account::types::Account;
use launcher_core::{
    types::{AssetIndexJson, VersionJson, VersionManifest},
    AsyncLauncher, Error,
};
use reqwest::Client;

// TODO: Store encrypted auth token for reuse: Use Keyring crate
// TODO: Document existing UI functionality: In-Progress
// TODO: Make vector of accounts, allow account selection
// TODO: Make instances, must be savable to disk, maybe using RON?: Json Format
// TODO: Redo error handling, fields that can error should hold Result<T, E>
// UPDATE: We could also add a tag to the error? Not sure. Constant Error checking would suck.
// TODO: Config file/Config tx/rx setup: TOML
struct LauncherGui {
    rt: async_bridge::Runtime<Message, Response, State>,
    rx: async_channel::Receiver<EarlyMessage>,
    launcher: Arc<AsyncLauncher>,
    data: MCData,
    player: PlayerData,
    java_version: u32,
    current_error: Option<Error>,
    java_path: Option<Arc<str>>,
    started: bool,
}

#[derive(Default)]
struct PlayerData {
    // Player account, if it exists
    account: Option<Account>,
    // URL for auth, if it exists
    url: Option<String>,
    // Code will always exist if URL does
    code: Option<String>,
}

#[derive(Default)]
struct MCData {
    // Version Manifest read/write able
    versions: Option<VersionManifest>,
    // Holds the index in manifest for the current selected version
    selected_version: usize,
    // Version JSON, read only
    version_json: Option<Arc<VersionJson>>,
    // Asset Index, read only
    asset_index: Option<Arc<AssetIndexJson>>,
    // Total and finished libraries, divide as floats
    // and multiply by 100 to get progress as percentage
    total_libraries: Arc<AtomicUsize>,
    finished_libraries: Arc<AtomicUsize>,
    // Total and finished assets, divide as floats
    // and multiply by 100 to get progress as percentage
    total_assets: Arc<AtomicUsize>,
    finished_assets: Arc<AtomicUsize>,
    // Total progress downloading the MC jar
    total_jar: Arc<AtomicUsize>,
    finished_jar: Arc<AtomicUsize>,
    // Classpath for the MC jar, also doubles as verifying
    // That libraries are completely loaded before launch
    class_path: Option<String>,
    // The jar path, stored separate because futures are not ordered
    jar_path: String,
    // Whether or not all assets are loaded
    assets: bool,
    // Whether or not the current MC jar is ready
    jar: bool,
    // If the launcher is attempting to launch
    launching: bool,
}

impl LauncherGui {
    fn new(cc: &eframe::CreationContext) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .build()
            .expect("Runtime Failed to Build");

        let client = Client::new();
        let launcher_core = Arc::new(AsyncLauncher::new(client.clone()));
        let (tx, rx) = async_channel::unbounded();

        let rt = async_bridge::Runtime::new(
            4,
            State {
                client,
                launcher_core: launcher_core.clone(),
                tx,
            },
            cc.egui_ctx.clone(),
            worker_event_loop,
            rt,
        );

        LauncherGui {
            rt,
            rx,
            launcher: launcher_core.clone(),
            player: Default::default(),
            data: MCData {
                // Defaults to usize::MAX to show no version is selected
                selected_version: usize::MAX,
                ..Default::default()
            },
            java_version: u32::MAX,
            current_error: None,
            java_path: None,
            started: false,
        }
    }

    fn update_state(&mut self, _: &egui::Context) {
        let event = self.rt.try_recv();
        if let Ok(message) = event {
            match message {
                Response::Versions(version) => match version {
                    Ok(versions) => self.data.versions = Some(versions),
                    Err(err) => self.current_error = Some(err),
                },
                Response::Version(version) => match version {
                    Ok(json) => self.data.version_json = Some(json.into()),
                    Err(err) => self.current_error = Some(err),
                },
                Response::Libraries(result, tag) => match result {
                    Ok(path) => {
                        let versions = self.data.versions.as_ref().unwrap();
                        if versions.versions[self.data.selected_version] == tag {
                            self.data.class_path = Some(path);
                        }
                    }
                    Err(err) => self.current_error = Some(err),
                },
                Response::AssetIndex(idx, tag) => match idx {
                    Ok(json) => {
                        let versions = self.data.versions.as_ref().unwrap();
                        if versions.versions[self.data.selected_version] == tag {
                            let index = Arc::new(json);

                            let future = get_assets(
                                self.launcher.clone(),
                                index.clone(),
                                Path::new("./assets"),
                                self.data.finished_assets.clone(),
                                self.data.total_assets.clone(),
                                tag.clone(),
                            );

                            self.rt.future(future);

                            self.data.asset_index = Some(index)
                        }
                    }
                    Err(err) => self.current_error = Some(err),
                },
                Response::Asset(result, tag) => match result {
                    Ok(()) => {
                        let versions = self.data.versions.as_ref().unwrap();
                        if versions.versions[self.data.selected_version] == tag {
                            self.data.assets = true;
                        }
                    }
                    Err(err) => self.current_error = Some(err),
                },
                Response::Jar(res, tag) => match res {
                    Ok(jar) => {
                        let versions = self.data.versions.as_ref().unwrap();
                        if versions.versions[self.data.selected_version] == tag {
                            self.data.jar = true;
                            self.data.jar_path = jar;
                        }
                    }
                    Err(err) => self.current_error = Some(err),
                },
                Response::Auth(res) => match res {
                    Ok(acc) => self.player.account = Some(acc),
                    Err(err) => self.current_error = Some(err),
                },
                Response::JavaMajorVersion(version) | Response::DefaultJavaVersion(version) => {
                    self.java_version = version.unwrap();
                }
            }
        }

        if let Ok(val) = self.rx.try_recv() {
            match val {
                EarlyMessage::LinkCode((url, code)) => {
                    self.player.code = Some(code);
                    self.player.url = Some(url);
                }
            }
        }
    }

    fn prepare_launch(&self, json: &Arc<VersionJson>, manifest: &VersionManifest) {
        let libraries = json.libraries().clone();
        let index = json.asset_index().clone();
        let current = self.data.selected_version;
        let tag = manifest.versions[current].clone();

        let future = get_asset_index(
            self.launcher.clone(),
            index.clone(),
            tag.clone(),
            Path::new("./assets"),
        );
        self.rt.future(future);
        let future = get_libraries(
            self.launcher.clone(),
            libraries,
            Path::new("./"),
            self.data.total_libraries.clone(),
            self.data.finished_libraries.clone(),
            tag.clone(),
        );
        self.rt.future(future);
        let future = get_jar(
            self.launcher.clone(),
            json.clone(),
            Path::new("./versions"),
            self.data.total_libraries.clone(),
            self.data.finished_libraries.clone(),
            tag.clone(),
        );
        self.rt.future(future);
    }

    fn maybe_launch(&self) -> bool {
        if let (Some(class_path), Some(json), Some(acc)) = (
            &self.data.class_path,
            &self.data.version_json,
            &self.player.account,
        ) {
            if self.data.assets && self.data.launching {
                let dir = Path::new("./");

                let jvm = if let Some(jvm) = &self.java_path {
                    jvm
                } else {
                    "java"
                };

                launcher_core::launch_game(
                    jvm,
                    json,
                    dir,
                    &dir.join("assets"),
                    acc,
                    CLIENT_ID,
                    "0",
                    "Synth Launcher",
                    "0.1.0",
                    &format!("{}{}", class_path, &self.data.jar_path),
                );
                !self.data.launching
            } else {
                self.data.launching
            }
        } else {
            self.data.launching
        }
    }

    fn progress_window(&self, ctx: &egui::Context) {
        egui::Window::new("Progress").auto_sized().show(ctx, |ui| {
            let percentage =
                |finished: usize, total: usize| -> f64 { (finished as f64 / total as f64) * 100.0 };

            let maybe_total = self.data.total_libraries.load(Ordering::Relaxed);
            let finished = self.data.finished_libraries.load(Ordering::Relaxed);

            // Ensure we're not dividing by 0
            let total = if maybe_total == 0 { 1 } else { maybe_total };
            let string = format!("Library Progress: {:.2} %", percentage(finished, total));
            ui.label(string);

            let maybe_total = self.data.total_assets.load(Ordering::Relaxed);
            let finished = self.data.finished_assets.load(Ordering::Relaxed);

            // Ensure we're not dividing by 0
            let total = if maybe_total == 0 { 1 } else { maybe_total };
            let string = format!("Asset Progress: {:.2} %", percentage(finished, total));
            ui.label(string);

            if !self.data.jar {
                let maybe_total = self.data.total_jar.load(Ordering::Relaxed);
                let finished = self.data.finished_jar.load(Ordering::Relaxed);

                // Ensure we're not dividing by 0
                let total = if maybe_total == 0 { 1 } else { maybe_total };
                let string = format!("Jar Progress: {:.2} %", percentage(finished, total));
                ui.label(string);
            } else {
                ui.label("Jar Progress: 100.00%");
            }

            ctx.request_repaint();
        });
    }

    fn on_start(&self) {
        if !self.started {
            self.rt.future(get_default_version_response());
            self.rt.send_with_message(Message::Auth);
            self.rt.send_with_message(Message::Versions);
        }
    }
}

impl eframe::App for LauncherGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // For events that only happen on start
        self.on_start();
        self.started = true;

        self.update_state(ctx);

        if let Some(error) = &self.current_error {
            egui::Window::new("Help").auto_sized().show(ctx, |ui| {
                ui.label(error.to_string());
            });
        }

        if self.player.account.is_none() {
            egui::Window::new("Login").auto_sized().show(ctx, |ui| {
                if let (Some(url), Some(code)) = (&self.player.url, &self.player.code) {
                    let hyper = egui::Hyperlink::from_label_and_url("Click here to login", url);
                    let label = Label::new(code).sense(Sense::click());
                    let label = ui.add(label).on_hover_ui(|ui| {
                        ui.label("Copy this token into the site below!");
                    });

                    if label.clicked() {
                        ctx.copy_text(code.to_string());
                    }
                    ui.add(hyper);
                } else {
                    ui.label("Loading code and url, please wait...");
                }
            });
        }

        let size = ctx.input(|i| i.screen_rect());
        let width = size.width();
        let height = size.height();

        egui::SidePanel::left("General Panel")
            .default_width(width * 0.1)
            .resizable(false)
            .show(ctx, |ui| {
                if let Some(versions) = &self.data.versions {
                    let index = self.data.selected_version;
                    let text = if index != usize::MAX {
                        &versions.versions[index].id
                    } else {
                        "None"
                    };

                    ui.add_space(height * 0.01);

                    egui::ComboBox::from_id_source("VersionSelect")
                        .selected_text(text)
                        .show_ui(ui, |ui| {
                            let iter = versions.versions.iter().enumerate();
                            iter.for_each(|(index, version)| {
                                let selected = &mut self.data.selected_version;

                                if ui.selectable_value(selected, index, &version.id).clicked() {
                                    let launcher = self.launcher.clone();
                                    let version = version.clone();
                                    let path = Path::new("./versions");

                                    if let Some(json) = &self.data.version_json {
                                        if version.id != json.id() {
                                            self.data.version_json = None;
                                            self.data.class_path = None;
                                            self.data.assets = false;
                                            self.rt.future(get_version(launcher, version, path));
                                        }
                                    } else {
                                        self.rt.future(get_version(launcher, version, path));
                                    }
                                };
                            })
                        });

                    egui::ComboBox::from_id_source("Java Selector").show_ui(ui, |ui| {
                        // TODO: Config Needed
                    });

                    let text = if self.java_version != u32::MAX {
                        self.java_version.to_string()
                    } else {
                        String::from("Select Java Version")
                    };

                    ui.label(format!("Java Version: {text}"));

                    if ui.button("Select Java Version").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            let path: Arc<str> = path.display().to_string().into();
                            self.rt.future(get_major_version_response(path.clone()));
                            self.java_path = Some(path);
                        }
                    }

                    let button = Button::new("Play");

                    if let Some(version_json) = &self.data.version_json {
                        let enabled = !self.data.launching && self.player.account.is_some();
                        let enabled = ui.add_enabled(enabled, button);

                        if enabled.clicked() {
                            self.prepare_launch(version_json, versions);
                            self.data.launching = true;
                        }
                    } else {
                        ui.add_enabled(false, button);
                    }
                } else {
                    ui.spinner();
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut style = Style::default();
            style.visuals.button_frame = true;

            ui.set_style(style);

            if self.data.launching {
                self.progress_window(ctx);
            }

            self.data.launching = self.maybe_launch();
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
