pub use plugin::plugin_client::PluginClient;
#[cfg(unix)]
use tokio::net::UnixStream;

use std::collections::HashMap;
use std::str;
use tonic::transport::{Endpoint, Uri};
use std::convert::TryFrom;
use std::process::{Command, Child, Stdio};
use tower::service_fn;
use std::sync::Mutex;
use std::sync::Arc;

pub mod plugin {
    tonic::include_proto!("plugin");
}

#[derive(Debug, Clone)]
pub struct Plugin {
    path: String,
    plugin_type: String,
    version: String,
    active: Arc<Mutex<Option<Child>>>,
    client: Arc<Mutex<Option<PluginClient<tonic::transport::channel::Channel>>>>
}

#[derive(Debug)]
pub struct PluginProcess {

}

#[derive(Debug, Clone)]
pub struct PluginRegistry {
    pub plugins: HashMap<String, Plugin>
}

impl PluginRegistry {
    pub async fn dispense(&mut self, plugin_name: String) -> Result<PluginClient<tonic::transport::channel::Channel>, String> {
        match self.plugins.get_mut(&plugin_name) {
            Some(x) => {
                if x.active.lock().unwrap().is_some() {
                    return Ok(x.client.lock().unwrap().clone().unwrap());
                }
                let handle = Command::new("sh")
                                        .arg("-c")
                                        .arg(&x.path)
                                        .stdout(Stdio::piped())
                                        .stderr(Stdio::piped())
                                        .spawn();
                match handle {
                    Ok(y) => {
                        let mut active = x.active.lock().unwrap();
                        *active = Some(y);
                        let mut retry: i32 = 0; 
                        let mut channel: Result<tonic::transport::channel::Channel, tonic::transport::Error>;
                        let path_dest = format!("/tmp/tonic/{}", plugin_name);
                        while retry < 10 {
                            let path_dest = path_dest.clone();
                            channel = Endpoint::try_from("http://[::]:50051").unwrap()
                                .connect_with_connector(service_fn(move |_: Uri| {
                                    // Connect to a Uds socket
                                    let path = path_dest.clone();
                                    println!("Path: {}", path);
                                    UnixStream::connect(path)
                            }))
                            .await;

                            match channel {
                                Ok(z) => {
                                    let client = PluginClient::new(z);
                                    let mut client_r = x.client.lock().unwrap();
                                    *client_r = Some(client.clone());
                                    return Ok(client);
                                },
                                Err(e) => {
                                    println!("socket error: {:?}", e);
                                    retry += 1;
                                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                                }
                            }
                        }
                        return Err("failed to contact server".to_string())
                    },
                    Err(_) => {
                        return Err("failed to spawn plugin".to_string());
                    }
                }
            },
            None => {
                return Err("plugin with that name does not exist".to_string());
            }
        }
    }

    pub async fn load_plugin(&mut self, plugin_path: String) -> Result<(), String> {
        let plugin_info = Command::new(plugin_path.clone())
                                    .arg("-i")
                                    .output();
        if plugin_info.is_err() {
            return Err("could not load plugin info".to_string())
        }
        let output = plugin_info.unwrap().stdout.to_vec();
        let info_parts: Vec<&str> = std::str::from_utf8(&output).unwrap().split("|").map(|x| x.trim()).collect();
        self.plugins.insert(info_parts[0].to_string(), Plugin {
            path: plugin_path,
            plugin_type: "execution".to_string(),
            version: info_parts[1].to_string(),
            active: Arc::new(Mutex::new(None)),
            client: Arc::new(Mutex::new(None))
        });
        return Ok(());
    }

    pub async fn reap(mut self, plugin_name: String) -> Result<(), String> {
        match self.plugins.get_mut(&plugin_name) {
            Some(x) => {
                if x.active.lock().unwrap().is_some() {
                    let mut handle = x.active.lock().unwrap();
                    let mut_handle = handle.as_mut().unwrap();
                    mut_handle.kill();
                    std::fs::remove_file(format!("/tmp/tonic/{}", plugin_name));
                    return Ok(());
                }
                return Err("Plugin inactive".to_string());
            }
            None => {
                return Err("No such plugin".to_string());
            }
        }
    }

    pub async fn reap_all(self) -> Result<(), std::io::Error> {
        for (k, x) in self.plugins {
            if x.active.lock().unwrap().is_some() {
                let mut handle = x.active.lock().unwrap();
                let mut_handle = handle.as_mut().unwrap();
                mut_handle.kill();
                let ret = std::fs::remove_file(format!("/tmp/tonic/{}", k));
                match ret {
                    Ok(_) => {},
                    Err(e) => return Err(e)
                }
            }
        }
        return Ok(());
    }
}