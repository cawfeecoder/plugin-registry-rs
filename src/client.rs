use plugin::plugin_client::PluginClient;
use plugin::{Function, ListFunctionResponse, ExecuteRequest, ExecuteResponse, Empty};
use tonic::{Request};
#[cfg(unix)]
use tokio::net::UnixStream;

use std::collections::HashMap;
use std::str;
use std::path::Path;
use tonic::transport::{Endpoint, Uri};
use tonic::transport::channel::Channel;
use std::convert::TryFrom;
use async_process::{Command, Child, Stdio};
use futures_lite::{io::BufReader, prelude::*};
use tower::service_fn;

#[macro_use]
mod macros;

pub mod plugin {
    tonic::include_proto!("plugin");
}

#[derive(Debug)]
pub struct Plugin {
    path: String,
    plugin_type: String,
    version: String,
    active: Option<Child>,
    client: Option<PluginClient<tonic::transport::channel::Channel>>
}

#[derive(Debug)]
pub struct PluginProcess {

}

#[derive(Debug)]
pub struct PluginRegistry {
    plugins: HashMap<String, Plugin>
}

impl PluginRegistry {
    pub async fn dispense(&mut self, plugin_name: String) -> Result<PluginClient<tonic::transport::channel::Channel>, String> {
        match self.plugins.get_mut(&plugin_name) {
            Some(ref mut x) => {
                if x.active.is_some() {
                    return Ok(x.client.clone().unwrap());
                }
                let handle = Command::new("sh")
                                        .arg("-c")
                                        .arg(&x.path)
                                        .stdout(Stdio::piped())
                                        .stderr(Stdio::piped())
                                        .spawn();
                match handle {
                    Ok(mut y) => {
                        x.active = Some(y);
                        let mut retry: i32 = 0; 
                        let mut channel: Result<tonic::transport::channel::Channel, tonic::transport::Error>;
                        let path_dest = format!("/tmp/tonic/{}", plugin_name);
                        while retry < 3 {
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
                                Ok(x) => {
                                    let client = PluginClient::new(x);
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
                                    .output()
                                    .await;
        if plugin_info.is_err() {
            return Err("could not load plugin info".to_string())
        }
        let output = plugin_info.unwrap().stdout.to_vec();
        let info_parts: Vec<&str> = std::str::from_utf8(&output).unwrap().split("|").map(|x| x.trim()).collect();
        self.plugins.insert(info_parts[0].to_string(), Plugin {
            path: plugin_path,
            plugin_type: "execution".to_string(),
            version: info_parts[1].to_string(),
            active: None,
            client: None
        });
        return Ok(());
    }

    pub async fn reap(mut self, plugin_name: String) -> Result<(), String> {
        match self.plugins.get_mut(&plugin_name) {
            Some(x) => {
                if x.active.is_some() {
                    let handle = x.active.as_mut().unwrap();
                    handle.kill();
                    println!("{:?}", std::fs::remove_file(format!("/tmp/tonic/{}", plugin_name)));
                    return Ok(());
                } 
                return Err("Plugin inactive".to_string());
            }
            None => {
                return Err("No such plugin".to_string());
            }
        }
    }

    pub async fn reap_all(self) -> Result<(), String> {
        for (k, mut x) in self.plugins {
            if x.active.is_some() {
                let mut handle = x.active.unwrap();
                handle.kill();
                println!("{:?}", std::fs::remove_file(format!("/tmp/tonic/{}", k)));
            }
        }
        return Ok(());
    }
}

#[cfg(unix)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = PluginRegistry {
        plugins: HashMap::new()
    };

    registry.load_plugin("/home/nfrush/workspace/rust/rust-plugin/target/release/plugin-server".to_string()).await;

    let mut plugin_client = match registry.dispense("my_plugin".to_string()).await {
        Ok(x) => {
            println!("dispensing plugin");
            Some(x)
        },
        Err(e) => {
            println!("we encountered an error: {:?}", e);
            None
        }
    };

    if plugin_client.is_some() {
        let mut client = plugin_client.unwrap();
        let e_req = ExecuteRequest{
            function_name: "echo".to_string(),
            args: hashmap!["message".to_string() => "hello world!".as_bytes().to_vec()]
        };
        let request = Request::new(e_req.clone());

        println!("calling remote plugin");
        let start = std::time::Instant::now();
        let response = client.execute(request).await?;
        println!("plugin contacted and executed in {}micros", start.elapsed().as_micros());
        println!("Response={:?}", str::from_utf8(response.into_inner().rets.get("stdout").unwrap()).unwrap());

        let request = Request::new(e_req.clone());
        let start = std::time::Instant::now();
        let response = client.execute(request).await?;
        println!("plugin contacted and executed in {}micros", start.elapsed().as_micros());
        println!("Response={:?}", str::from_utf8(response.into_inner().rets.get("stdout").unwrap()).unwrap());
    }
    //This should really either be watching the sigterm of this program for graceful shutdown or a LRU-type cache
    //should be implemented to reap plugins after a bit
    registry.reap_all().await;

    Ok(())
}