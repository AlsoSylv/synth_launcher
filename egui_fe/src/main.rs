mod worker_logic;
mod wrappers;

use std::fs::File;
use std::io::{Read, Write};
use worker_logic::*;
use wrappers::*;

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use eframe::egui::{self, Button, Label, Sense, Style};
use launcher_core::account::types::Account;
use launcher_core::{
    types::{AssetIndexJson, VersionJson, VersionManifest},
    AsyncLauncher,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};

// TODO: Store encrypted auth token for reuse: Use Keyring crate
// TODO: Document existing UI functionality: In-Progress
// TODO: Make vector of accounts, allow account selection
// TODO: Make instances, must be savable to disk, maybe using RON?: Json Format
// TODO: Redo error handling, fields that can error should hold Result<T, E>
// UPDATE: We could also add a tag to the error? Not sure. Constant Error checking would suck.
// TODO: Config file/Config tx/rx setup: TOML
struct LauncherGui {
    // Async thread pool to handle futures
    rt: async_bridge::Runtime<Message, Response, State>,
    // receiver for messages sent before the event is finished
    rx: async_channel::Receiver<EarlyMessage>,
    // Reference to the async launcher
    launcher: Arc<AsyncLauncher>,
    // Minecraft Data
    data: MCData,
    // Data related to the player
    player: PlayerData,
    launcher_path: Arc<PathBuf>,
    // Current major java version
    java_version: u32,
    java_path: Option<Rc<String>>,
    jvm_name: Rc<String>,
    current_error: Option<Error>,
    // Path to JVM, if changed
    // Flipped once for startup tasks
    started: bool,
    config_file: File,
    config: Config,
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
    // Classpath for the MC jar, also doubles as verifying
    // That libraries are completely loaded before launch
    class_path: Option<String>,
    // The jar path, stored separate because futures are not ordered
    // This is None if the version has changed
    jar_path: Option<String>,
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
    // Whether or not all assets are loaded
    assets: bool,
    // If the launcher is attempting to launch
    launching: bool,
}

impl LauncherGui {
    fn new(cc: &eframe::CreationContext, home_dir: PathBuf, config_file: File, config: Config) -> Box<Self> {
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
            launcher_path: Arc::new(home_dir),
            java_version: u32::MAX,
            current_error: None,
            java_path: None,
            jvm_name: Rc::new("Default".into()),
            started: false,
            config_file,
            config
        }.into()
    }

    fn update_state(&mut self, _: &egui::Context) {
        let event = self.rt.try_recv();
        if let Ok(message) = event {
            match message {
                Response::Versions(version) => match version {
                    Ok(versions) => self.data.versions = Some(versions),
                    Err(err) => self.current_error = Some(err.into()),
                },
                Response::Version(version) => match version {
                    Ok(json) => self.data.version_json = Some(json.into()),
                    Err(err) => self.current_error = Some(err.into()),
                },
                Response::Auth(res) => match res {
                    Ok(acc) => self.player.account = Some(acc),
                    Err(err) => self.current_error = Some(err.into()),
                },
                Response::JavaMajorVersion(version) | Response::DefaultJavaVersion(version) => {
                    println!("H");
                    self.java_version = version.unwrap();
                    println!("{}", self.java_version);
                }
                Response::Tagged(response, tag) => {
                    if let Some(versions) = &self.data.versions {
                        match response {
                            TaggedResponse::Libraries(result) => match result {
                                Ok(path) => {
                                    if versions.versions[self.data.selected_version] == tag {
                                        self.data.class_path = Some(path);
                                    }
                                }
                                Err(err) => self.current_error = Some(err.into()),
                            },
                            TaggedResponse::AssetIndex(idx) => match idx {
                                Ok(json) => {
                                    if versions.versions[self.data.selected_version] == tag {
                                        let index = Arc::new(json);

                                        let future = get_assets(
                                            self.launcher.clone(),
                                            index.clone(),
                                            self.launcher_path.clone(),
                                            self.data.total_assets.clone(),
                                            self.data.finished_assets.clone(),
                                            tag.clone(),
                                        );

                                        self.rt.future(future);

                                        self.data.asset_index = Some(index);
                                    }
                                }
                                Err(err) => self.current_error = Some(err.into()),
                            },
                            TaggedResponse::Asset(result) => match result {
                                Ok(()) => {
                                    if versions.versions[self.data.selected_version] == tag {
                                        self.data.assets = true;
                                    }
                                }
                                Err(err) => self.current_error = Some(err.into()),
                            },
                            TaggedResponse::Jar(res) => match res {
                                Ok(jar) => {
                                    if versions.versions[self.data.selected_version] == tag {
                                        self.data.jar_path = Some(jar);
                                    }
                                }
                                Err(err) => self.current_error = Some(err.into()),
                            },
                        }
                    }
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
            self.launcher_path.clone(),
        );
        self.rt.future(future);
        let future = get_libraries(
            self.launcher.clone(),
            libraries,
            self.launcher_path.clone(),
            self.data.total_libraries.clone(),
            self.data.finished_libraries.clone(),
            tag.clone(),
        );
        self.rt.future(future);
        let future = get_jar(
            self.launcher.clone(),
            json.clone(),
            self.launcher_path.clone(),
            self.data.total_jar.clone(),
            self.data.finished_jar.clone(),
            tag.clone(),
        );
        self.rt.future(future);
    }

    fn maybe_launch(&self) -> bool {
        if let (Some(class_path), Some(json), Some(acc), Some(jar_path)) = (
            &self.data.class_path,
            &self.data.version_json,
            &self.player.account,
            &self.data.jar_path,
        ) {
            if self.data.assets && self.data.launching {
                let jvm = if let Some(jvm) = &self.java_path {
                    jvm.as_ref()
                } else {
                    "java"
                };

                launcher_core::launch_game(
                    jvm,
                    json,
                    &self.launcher_path,
                    &self.launcher_path.join("assets"),
                    acc,
                    CLIENT_ID,
                    "0",
                    "Synth Launcher",
                    "0.1.0",
                    &format!("{}{}", class_path, jar_path),
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

            if self.data.jar_path.is_none() {
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

    fn send_message(&self, contents: Contents) {
        self.rt.send_with_message(Message {
            path: self.launcher_path.clone(),
            contents,
        });
    }

    fn on_start(&self) {
        if !self.started {
            self.rt.future(get_default_version_response());
            self.send_message(Contents::Auth);
            self.send_message(Contents::Versions);
        }
    }
}

impl eframe::App for LauncherGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // For events that only happen on start
        self.on_start();
        self.started = true;
        self.update_state(ctx);

        let mut config_updated = false;

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
                                    let path = self.launcher_path.clone();

                                    if let Some(json) = &self.data.version_json {
                                        if version.id != json.id() {
                                            self.data.version_json = None;
                                            self.data.class_path = None;
                                            self.data.jar_path = None;
                                            self.data.assets = false;
                                            self.rt.future(get_version(launcher, version, path));
                                        }
                                    } else {
                                        self.rt.future(get_version(launcher, version, path));
                                    }
                                };
                            })
                        });

                    egui::ComboBox::from_id_source("Java Selector").selected_text(self.jvm_name.as_str()).show_ui(ui, |ui| {
                        if ui.button("Default").clicked() {
                            self.jvm_name = Rc::new("Default".into());
                            self.rt.future(get_default_version_response());
                        }

                        for jvm in &self.config.jvms {
                            if ui.button(jvm.name.as_str()).clicked() {
                                self.jvm_name = jvm.name.clone();
                                self.java_path = Some(jvm.path.clone());
                                let (_vendor, version) = get_vendor_major_version(&jvm.path);
                                self.java_version = version;
                            }
                        }
                    });

                    if self.java_version != u32::MAX {
                        ui.label(format!("Java Version: {}", self.java_version));
                    } else {
                        ui.label("No Java Version");
                    };


                    if ui.button("Add Java Version").clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            let path: Rc<String> = path.display().to_string().into();
                            let (vendor, version) = get_vendor_major_version(&path);
                            self.config.jvms.push(JVM { path, name: Rc::new(format!("{vendor} {version}")) });
                            config_updated = true;
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

        if config_updated {
            let bytes = toml::to_string_pretty(&self.config).unwrap();
            self.config_file.write_all(bytes.as_bytes()).unwrap();
        }
    }
}

fn check_config() -> Result<(PathBuf, File, Config), Error> {
    let home = home::home_dir().unwrap();

    let mut file: File;
    let config: Config;

    if home.join("config.toml").exists() {
        file = File::open("config.toml")?;
        let mut buffer = String::new();
        file.read_to_string(&mut buffer)?;
        config = toml::from_str(&buffer)?;
    } else {
        file = File::create("config.toml")?;
        config = Config::default();
    }

    Ok((home, file, config))
}

#[derive(Default, Deserialize, Serialize)]
struct Config {
    jvms: Vec<JVM>,
    accounts: Vec<Account>
}

#[derive(Default, Deserialize, Serialize)]
struct JVM {
    path: Rc<String>,
    name: Rc<String>,
}

fn main() {
    let (home, config_file, config) = check_config().unwrap();

    eframe::run_native(
        "Test App",
        eframe::NativeOptions::default(),
        Box::new(|cc| LauncherGui::new(cc, home, config_file, config)),
    )
    .unwrap();
}

#[derive(Debug)]
enum Error {
    Reqwest(reqwest::Error),
    Tokio(tokio::io::Error),
    SerdeJson(serde_json::Error),
    Toml(toml::de::Error),
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Error::Reqwest(value)
    }
}

impl From<tokio::io::Error> for Error {
    fn from(value: tokio::io::Error) -> Self {
        Error::Tokio(value)
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::SerdeJson(value)
    }
}

impl From<toml::de::Error> for Error {
    fn from(value: toml::de::Error) -> Self {
        Error::Toml(value)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            Error::Reqwest(err) => err.to_string(),
            Error::Tokio(err) => err.to_string(),
            Error::SerdeJson(err) => err.to_string(),
            Error::Toml(err) => err.to_string(),
        };
        write!(f, "{}", str)
    }
}

impl From<launcher_core::Error> for Error {
    fn from(value: launcher_core::Error) -> Self {
        match value {
            launcher_core::Error::Reqwest(e) => {
                Error::Reqwest(e)
            }
            launcher_core::Error::Tokio(e) => {
                Error::Tokio(e)
            }
            launcher_core::Error::SerdeJson(e) => {
                Error::SerdeJson(e)
            }
        }
    }
}