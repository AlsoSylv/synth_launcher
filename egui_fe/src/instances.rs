use launcher_core::types::Version;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Deserialize, Serialize)]
pub struct Instance {
    pub name: String,
    pub image: Option<PathBuf>,
    pub jvm: Rc<Jvm>,
    pub version: Arc<Version>,
    pub path: PathBuf,
    pub mod_loader: Option<Loader>,
    pub jvm_args: Vec<String>,
    pub env_args: Vec<String>,
}

#[derive(Default)]
pub struct InstanceBuilder {
    pub name: String,
    pub image: Option<String>,
    pub jvm: Rc<Jvm>,
    pub version: Option<Arc<Version>>,
    pub path: String,
    pub mod_loader: Option<Loader>,
    pub jvm_args: String,
    pub env_args: String,
}

impl InstanceBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(&self) -> &String {
        &self.name
    }

    pub fn name_mut(&mut self) -> &mut String {
        &mut self.name
    }

    pub fn image(&self) -> &Option<String> {
        &self.image
    }

    pub fn image_mut(&mut self) -> &mut Option<String> {
        &mut self.image
    }

    pub fn jvm(&self) -> &Rc<Jvm> {
        &self.jvm
    }

    pub fn jvm_mut(&mut self) -> &mut Rc<Jvm> {
        &mut self.jvm
    }

    pub fn version(&self) -> &Option<Arc<Version>> {
        &self.version
    }

    pub fn version_mut(&mut self) -> &mut Option<Arc<Version>> {
        &mut self.version
    }

    pub fn path(&self) -> &String {
        &self.path
    }

    pub fn path_mut(&mut self) -> &mut String {
        &mut self.path
    }

    pub fn mod_loader(&self) -> &Option<Loader> {
        &self.mod_loader
    }

    pub fn mod_loader_mut(&mut self) -> &mut Option<Loader> {
        &mut self.mod_loader
    }

    pub fn jvm_args(&self) -> &String {
        &self.jvm_args
    }

    pub fn jvm_args_mut(&mut self) -> &mut String {
        &mut self.jvm_args
    }

    pub fn env_args(&self) -> &String {
        &self.env_args
    }

    pub fn env_args_mut(&mut self) -> &mut String {
        &mut self.env_args
    }

    pub fn build(self) -> Instance {
        Instance {
            name: self.name,
            image: self.image.map(PathBuf::from),
            jvm: self.jvm,
            version: self.version.unwrap(),
            path: PathBuf::from(self.path),
            mod_loader: self.mod_loader,
            jvm_args: self.jvm_args.split(' ').map(String::from).collect(),
            env_args: self.env_args.split(' ').map(String::from).collect(),
        }
    }
}

#[derive(Deserialize, Serialize, PartialEq, Eq)]
pub enum Loader {
    Fabric,
}

#[derive(Deserialize, Serialize)]
pub struct Jvm {
    pub path: String,
    pub name: String,
}

impl Default for Jvm {
    fn default() -> Self {
        Self {
            path: "java".into(),
            name: "Default".into(),
        }
    }
}
