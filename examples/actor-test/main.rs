use actix::prelude::*;
use plugin_registry::registry::PluginRegistry;
use plugin_registry::registry::plugin::{ExecuteRequest, ExecuteResponse};
use std::collections::HashMap;
use tera::{Value, from_value, to_value};
use crossbeam_channel::bounded;

#[macro_use]
extern crate plugin_registry;

#[derive(Message, Debug)]
#[rtype(result = "Result<(), String>")]
struct PluginRequest {
    plugin_name: String,
    request: ExecuteRequest,
    send_chan: crossbeam_channel::Sender<Result<HashMap<String, Vec<u8>>, String>>
}

#[derive(Clone)]
struct RegistryActor {
    pub registry: PluginRegistry,
    pub recv_chan: crossbeam_channel::Receiver<PluginRequest>
}

impl Actor for RegistryActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        println!("Actor is alive");
        let this = self.clone();
        loop {
            println!("Starting waiting on channel");
            let res = this.recv_chan.recv();
            match res {
                Ok(v) => {
                    ctx.notify(v);
                    break;
                },
                Err(_) => {}
            }
        }
    }

    fn stopped(&mut self, ctx: &mut Context<Self>) {
        println!("Actor is stopped");
    }
}

fn to_string(data: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
    Ok(to_value(std::str::from_utf8(&from_value::<Vec<u8>>(data.clone()).unwrap()).unwrap().to_string()).unwrap())
}

impl Handler<PluginRequest> for RegistryActor {
    type Result = ResponseActFuture<Self, Result<(), String>>;

    fn handle(&mut self, msg: PluginRequest, ctx: &mut Context<Self>) -> Self::Result {
        let this = self.clone();
        Box::pin(actix::fut::wrap_future(this.dispense_plugin(msg)))
    }
}

impl RegistryActor {
    async fn dispense_plugin(mut self, msg: PluginRequest) -> Result<(), String> {
        println!("called");
        let plugin_name = msg.plugin_name;
        let request = msg.request;
        let chan = msg.send_chan;
        let res = self.registry.dispense(plugin_name.clone()).await;

        let mut client = res.unwrap();

        let request = tonic::Request::new(request);

        let res = client.execute(request).await;

        println!("Dispensed: {:?}", res);

        self.registry.reap(plugin_name.clone()).await;

        chan.send(Ok(res.unwrap().into_inner().rets)).unwrap();

        return Ok(())
    }
}

fn test_function(plugin_name: String, send_chan: crossbeam_channel::Sender<PluginRequest>) -> impl tera::Function {
    Box::new(move |args: &HashMap<String, tera::Value>| -> Result<Value, tera::Error> {
        match args.get("message") {
            Some(val) => match from_value::<String>(val.clone()) {
                    Ok(v) => {
                        let e_req = ExecuteRequest{
                            function_name: "echo".to_string(),
                            args: hashmap!["message".to_string() => "hello world!".as_bytes().to_vec()]
                        };

                        let plugin_name = plugin_name.clone();

                        let (s, r): (crossbeam_channel::Sender<Result<HashMap<String, Vec<u8>>, String>>, crossbeam_channel::Receiver<Result<HashMap<String, Vec<u8>>, String>>) = crossbeam_channel::unbounded();

                        let res = send_chan.send(PluginRequest { plugin_name: plugin_name, request: e_req, send_chan: s });

                        let mut ret: Result<Value, tera::Error> = Err("unable to receive ret".into());

                        loop {
                            let res = r.recv();
                            match res {
                                Ok(v) => {
                                    ret = Ok(to_value(v.unwrap()).unwrap());
                                    break;
                                },
                                Err(_) => {}
                            }
                        }
                       
                        ret
                    },
                    Err(_) => Err("failed to call plugin 2".into())
                },
                None => Err("failed to call plugin 3".into())
            }
        })
}

#[tokio::main]
async fn main() {
    let (s, r): (crossbeam_channel::Sender<PluginRequest>, crossbeam_channel::Receiver<PluginRequest>) = crossbeam_channel::unbounded();

    let mut threads: Vec<_> = vec![];

    let mut handle1 = std::thread::spawn(|| {
        let sys = actix::System::new();

        let execution = async {
            let mut registry = PluginRegistry {
                plugins: HashMap::new()
            };

            let res = registry.load_plugin("/home/nfrush/workspace/rust/rust-plugin/examples/target/release/my-plugin".to_string()).await;

            let addr = RegistryActor {
                registry: registry.clone(),
                recv_chan: r
            }.start();
        };

        let arb = actix::Arbiter::new();

        arb.handle().spawn(execution);

        sys.run();
    });

    threads.push(handle1);

    let mut handle2 = std::thread::spawn(|| {
        let mut tera = tera::Tera::new("*").unwrap();

        let context = tera::Context::new();

        tera.register_filter("to_string", to_string);

        tera.register_function("test_function", test_function("my_plugin".to_string(), s));

        let render = tera.render("test.tera", &context);

        if render.is_ok() {
            println!("{:?}", render.unwrap());
        }

        std::process::exit(0);
    });

    threads.push(handle2);

    for h in threads {
        h.join().unwrap();
    }
}