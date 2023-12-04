use async_channel::Sender;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use eframe::egui::{self, Button, Label, Sense, Style};
use launcher_core::account::auth::{
    authorization_token_response, minecraft_ownership_response, minecraft_profile_response,
    minecraft_response, refresh_token_response, xbox_response, xbox_security_token_response,
};
use launcher_core::account::types::Account;
use launcher_core::{
    account::auth::device_response,
    types::{AssetIndex, AssetIndexJson, Library, Version, VersionJson, VersionManifest},
    Error,
};
use reqwest::Client;

struct LauncherGui {
    rt: async_bridge::Runtime<Message, Response, State>,
    data: MCData,
    current_error: Option<Error>,
    account: Option<Account>,
    url: Option<String>,
    code: Option<String>,
    rx: async_channel::Receiver<EarlyMessage>,
}

struct MCData {
    versions: Option<VersionManifest>,
    selected_version: Option<usize>,
    version_json: Option<Arc<VersionJson>>,
    asset_index: Option<Arc<AssetIndexJson>>,
    total_libraries: Arc<AtomicUsize>,
    finished_libraries: Arc<AtomicUsize>,
    total_assets: Arc<AtomicUsize>,
    finished_assets: Arc<AtomicUsize>,
    class_path: String,
    jar_path: String,
    json_start: bool,
    versions_task_started: bool,
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
            json_start: true,
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
    Auth,
}

enum Response {
    Versions(Result<VersionManifest, Error>),
    Version(Result<Box<VersionJson>, Error>),
    Libraries(Result<String, Error>),
    AssetIndex(Result<AssetIndexJson, Error>),
    Asset(Result<(), Error>),
    Jar(Result<String, Error>),
    Auth(Result<Account, Error>),
}

#[derive(Clone)]
struct State {
    client: Client,
    launcher_core: launcher_core::AsyncLauncher,
    tx: Sender<EarlyMessage>,
}

enum EarlyMessage {
    LinkCode((String, String)),
}

fn worker_event_loop(
    message: Message,
    state: &State,
) -> impl std::future::Future<Output = Response> {
    let client = state.client.clone();
    let launcher_core = state.launcher_core.clone();
    let tx = state.tx.clone();
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
                Response::Version(json.map(Box::new))
            }
            Message::Libraries(libs, total, finished) => {
                let path = launcher_core
                    .download_libraries_and_get_path(
                        &libs,
                        &path.join("libraries"),
                        &path.join("natives"),
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
            Message::Auth => {
                let result = auth_or_refresh(&client, &tx).await;
                Response::Auth(result)
            }
        }
    }
}

const CLIENT_ID: &str = "04bc8538-fc3c-4490-9e61-a2b3f4cbcf5c";

async fn auth_or_refresh(client: &Client, tx: &Sender<EarlyMessage>) -> Result<Account, Error> {
    let exists = tokio::fs::try_exists("./refresh.txt").await?;
    let auth_res = if exists {
        let token = tokio::fs::read_to_string("./refresh.txt").await?;
        refresh_token_response(client, &token, CLIENT_ID)
            .await?
            .into()
    } else {
        // https://wiki.vg/Microsoft_Authentication_Scheme

        let device_response = device_response(client, CLIENT_ID).await?;

        let code = device_response.user_code;
        let ms_url = device_response.verification_uri;

        tx.send(EarlyMessage::LinkCode((ms_url, code)))
            .await
            .unwrap();

        loop {
            let device_code = &device_response.device_code;
            let auth_hook = authorization_token_response(client, device_code, CLIENT_ID).await;
            if let Ok(t) = auth_hook {
                break t;
            }
        }
    };

    let xbox_response = xbox_response(client, &auth_res.access_token).await?;

    let xbox_secure_token_res = xbox_security_token_response(client, &xbox_response.token).await?;

    let claims = &xbox_secure_token_res.display_claims;
    let token = &xbox_secure_token_res.token;
    let mc_res = minecraft_response(claims, token, client).await?;

    let ownership_check = minecraft_ownership_response(&mc_res.access_token, client).await?;

    if ownership_check.items.is_empty() {
        todo!("Is this worth checking?")
    }

    let profile = minecraft_profile_response(&mc_res.access_token, client).await?;

    tokio::fs::write("./refresh.txt", &auth_res.refresh_token).await?;

    use std::time::{Duration, SystemTime};

    let expires_in = Duration::from_secs(auth_res.expires_in);
    let system_time = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap();
    let combined_duration = system_time + expires_in;

    let account = Account {
        active: true,
        expiry: combined_duration.as_secs(),
        access_token: mc_res.access_token,
        profile,
    };

    Ok(account)
}

impl LauncherGui {
    fn new(cc: &eframe::CreationContext) -> Self {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .build()
            .expect("Runtime Failed to Build");

        let client = Client::new();
        let launcher_core = launcher_core::AsyncLauncher::new(client.clone());
        let (tx, rx) = async_channel::unbounded();

        let rt = async_bridge::Runtime::new(
            4,
            State {
                client,
                launcher_core,
                tx,
            },
            cc.egui_ctx.clone(),
            worker_event_loop,
            rt,
        );

        LauncherGui {
            rt,
            data: Default::default(),
            current_error: None,
            rx,
            account: None,
            code: None,
            url: None,
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
                Response::Libraries(result) => match result {
                    Ok(path) => {
                        self.data.libraries = true;
                        self.data.class_path = path;
                    }
                    Err(err) => self.current_error = Some(err),
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
                    Err(err) => self.current_error = Some(err),
                },
                Response::Asset(result) => match result {
                    Ok(()) => self.data.assets = true,
                    Err(err) => self.current_error = Some(err),
                },
                Response::Jar(res) => match res {
                    Ok(jar) => {
                        self.data.jar = true;
                        self.data.jar_path = jar;
                    }
                    Err(err) => self.current_error = Some(err),
                },
                Response::Auth(res) => match res {
                    Ok(acc) => self.account = Some(acc),
                    Err(err) => self.current_error = Some(err),
                },
            }
        }
    }

    fn prepare_launch(&mut self) {
        let unwrapped = &self.data.version_json;
        let version = unwrapped.as_ref().unwrap(); // This button is only enabled if this exists
        let libraries = version.libraries().clone();
        let index = version.asset_index().clone();

        self.rt.send_with_message(Message::AssetIndex(index));
        self.rt.send_with_message(Message::Libraries(
            libraries,
            self.data.total_libraries.clone(),
            self.data.finished_libraries.clone(),
        ));
        self.rt.send_with_message(Message::Jar(version.clone()));
        self.data.launching = true;
    }

    fn maybe_launch(&mut self) {
        if self.data.libraries && self.data.assets && self.data.jar && self.data.launching {
            self.data.launching = false;
            self.data.class_path.push_str(&self.data.jar_path);

            let json = self.data.version_json.as_ref().unwrap();
            let acc = self.account.as_ref().unwrap();

            let dir = Path::new("./");
            let class_path = &self.data.class_path;

            launcher_core::launch_game(
                json,
                &dir,
                &dir.join("assets"),
                acc,
                CLIENT_ID,
                "0",
                "Cynth Launcher",
                "0.1.0",
                class_path,
            );
        }
    }
}

impl eframe::App for LauncherGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_state(ctx);
        if !self.data.versions_task_started {
            self.rt.send_with_message(Message::Auth);
            self.rt.send_with_message(Message::Versions);
            self.data.versions_task_started = true;
        }

        if let Some(error) = &self.current_error {
            egui::Window::new("Help").auto_sized().show(ctx, |ui| {
                ui.label(error.to_string());
            });
        }

        if self.account.is_none() {
            egui::Window::new("Login").auto_sized().show(ctx, |ui| {
                if let (Some(url), Some(code)) = (&self.url, &self.code) {
                    let hyper = egui::Hyperlink::from_label_and_url("Click here to login", url);
                    let label = Label::new(code).sense(Sense::click());
                    let label = ui.add(label);

                    let label = label.on_hover_ui(|ui| {
                        ui.label("Copy this token into the site below!");
                    });

                    if label.clicked() {
                        ctx.copy_text(code.to_string());
                    }
                    ui.add(hyper);
                } else {
                    ui.label("Loading code and url, please wait...");
                    if let Ok(val) = self.rx.try_recv() {
                        match val {
                            EarlyMessage::LinkCode((url, code)) => {
                                self.code = Some(code);
                                self.url = Some(url);
                            }
                        }
                    }
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
                    let text = if let Some(index) = self.data.selected_version {
                        &versions.versions[index].id
                    } else {
                        "None"
                    };

                    ui.add_space(height * 0.01);

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
                                                self.data.json_start = true;
                                            }
                                        }

                                        self.data.jar = false;
                                        self.data.libraries = false;
                                        self.data.assets = false;

                                        if self.data.json_start {
                                            self.data.json_start = false;

                                            self.rt.send_with_message(Message::Version(
                                                version.clone(),
                                            ));
                                        }
                                    };
                                })
                        });

                    let button = Button::new("Play");

                    let enabled = ui.add_enabled(
                        self.data.version_json.is_some()
                            && !self.data.launching
                            && self.account.is_some(),
                        button,
                    );

                    if enabled.clicked() {
                        self.prepare_launch()
                    }
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

            self.maybe_launch();
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
