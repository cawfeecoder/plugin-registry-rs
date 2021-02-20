use tonic::{transport::Server, Request, Response, Status};

use plugin::plugin_server::{Plugin, PluginServer};
use plugin::{Function, ListFunctionResponse, ExecuteRequest, ExecuteResponse, Empty};
#[cfg(unix)]
use tokio::net::UnixListener;
use std::path::Path;
use futures::TryFutureExt;
use tonic_health::server::HealthReporter;

use std::process::Command;
use std::str;

use clap::Clap;

#[derive(Clap)]
#[clap(version = "1.0", author = "Nicholas Frush")]
struct Opts {
    #[clap(short = 'i', long = "info")]
    info: bool
}

#[macro_use]
mod macros;

pub mod plugin {
    tonic::include_proto!("plugin");
}

#[derive(Default)]
pub struct MyPlugin {}

#[tonic::async_trait]
impl Plugin for MyPlugin {
    async fn list_functions(&self, _request: Request<Empty>) -> Result<Response<ListFunctionResponse>, Status> {
        let reply = plugin::ListFunctionResponse {
            functions: vec![Function {
                name: "echo".to_string(),
                args: hashmap!["message".to_string() => "string".to_string()],
                rets: hashmap!["stdout".to_string() => "string".to_string()]
            }]
        };
        Ok(Response::new(reply))
    }

    async fn execute(&self, request: Request<ExecuteRequest>) -> Result<Response<ExecuteResponse>, Status> {
        let req = request.into_inner();
        match &req.clone().function_name[..] {
            "echo" => {

                println!("hello world");

                let reply = ExecuteResponse {
                    rets: hashmap!["stdout".to_string() => "success".as_bytes().to_vec()]
                };

                return Ok(Response::new(reply))
            },
            _ => {
                return Err(Status::invalid_argument(format!("plugin does not export function {}", req.clone().function_name)))
            }
        }
    }
}

#[cfg(unix)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opts: Opts = Opts::parse();

    if opts.info {
        println!("my_plugin|1.0.0");
        std::process::exit(0);
    }

    let path = "/tmp/tonic/my_plugin";

    tokio::fs::create_dir_all(Path::new(path).parent().unwrap()).await?;

    let my_plugin = MyPlugin::default();

    let incoming = {
        let uds = UnixListener::bind(path)?;

        async_stream::stream! {
            while let item = uds.accept().map_ok(|(st, _)| unix::UnixStream(st)).await {
                yield item;
            }
        }
    };

    println!("MyPlugin listening on {}", path);
 
    Server::builder()
        .add_service(PluginServer::new(my_plugin))
        .serve_with_incoming(incoming)
        .await?;

    Ok(())
}

#[cfg(unix)]
mod unix {
    use std::{
        pin::Pin,
        task::{Context, Poll},
    };

    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use tonic::transport::server::Connected;

    #[derive(Debug)]
    pub struct UnixStream(pub tokio::net::UnixStream);

    impl Connected for UnixStream {}

    impl AsyncRead for UnixStream {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.0).poll_read(cx, buf)
        }
    }

    impl AsyncWrite for UnixStream {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<std::io::Result<usize>> {
            Pin::new(&mut self.0).poll_write(cx, buf)
        }

        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.0).poll_flush(cx)
        }

        fn poll_shutdown(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<std::io::Result<()>> {
            Pin::new(&mut self.0).poll_shutdown(cx)
        }
    }
}