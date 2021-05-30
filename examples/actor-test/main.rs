use plugin_registry::registry::PluginRegistry;
use plugin_registry::registry::plugin::{ExecuteRequest, ExecuteResponse};
use tera::{Value, from_value, to_value};
use std::collections::HashMap;
use actix::prelude::*;
use actix::{Handler, Context, Message, Addr, SyncArbiter, SyncContext};
use futures::executor::block_on;

#[macro_use]
extern crate plugin_registry;

#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<HashMap<String, Vec<u8>>, String>")]
struct PluginRequest {
    plugin_name: String,
    request: ExecuteRequest
}

#[derive(Clone, Debug)]
struct RegistryActor {
    registry: PluginRegistry
}

impl Actor for RegistryActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        println!("RegistryActor is alive");
     }
 
     fn stopped(&mut self, ctx: &mut Context<Self>) {
        let this = self.clone();
        let res = block_on(async move {
            let resp = this.registry.reap_all().await;
            println!("REAP: {:?}", resp);
        });
        println!("RegistryActor is stopped");
     }
}

impl Handler<PluginRequest> for RegistryActor {
    type Result = ResponseActFuture<Self, Result<HashMap<String, Vec<u8>>, String>>;

    fn handle(&mut self, msg: PluginRequest, ctx: &mut Context<Self>) -> Self::Result {
        let this = self.clone();
        Box::pin(actix::fut::wrap_future(this.dispense_plugin(msg)))
    }
}

impl RegistryActor {
        async fn dispense_plugin(mut self, msg: PluginRequest) -> Result<HashMap<String, Vec<u8>>, String> {
            let plugin_name = msg.plugin_name;
            let request = msg.request;
            let res = self.registry.dispense(plugin_name.clone()).await;
    
            let mut client = res.unwrap();
    
            let request = tonic::Request::new(request);
    
            let res = client.execute(request).await;

            match res {
                Ok(v) => return Ok(v.into_inner().rets),
                Err(e) => return Err(e.to_string())
            }
        }
}

#[derive(Message, Debug, Clone)]
//#[rtype(result = "Result<String, String>")]
#[rtype(result = "()")]
struct TemplateRequest {
    template_path: String
}

#[derive(Clone, Debug)]
struct TemplateActor {
    plugin_actor: Addr<RegistryActor>
}

impl Actor for TemplateActor {
    type Context = SyncContext<Self>;

    fn started(&mut self, ctx: &mut SyncContext<Self>) {
        println!("TemplateActor is alive");
     }
 
     fn stopped(&mut self, ctx: &mut SyncContext<Self>) {
        println!("TemplateActor is stopped");
     }
}

fn to_string(data: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
    Ok(to_value(std::str::from_utf8(&from_value::<Vec<u8>>(data.clone()).unwrap()).unwrap().to_string()).unwrap())
}

impl Handler<TemplateRequest> for TemplateActor {
    //type Result = Result<String, String>;
    type Result = ();

    fn handle(&mut self, msg: TemplateRequest, ctx: &mut Self::Context) -> Self::Result {
        let this = self.clone();

        let mut tera = tera::Tera::new("*").unwrap();

        let context = tera::Context::new(); 

        tera.register_filter("to_string", to_string);

        tera.register_function("test_function", self.test_function("my_plugin".to_string()));

        let render = tera.render("test.tera", &context);

        if render.is_ok() {
            println!("{}", render.unwrap());
        }
    }
}

impl TemplateActor {

    fn test_function(&self, plugin_name: String) -> impl tera::Function {
            let plugin_actor = self.clone().plugin_actor;
            Box::new(move |args: &HashMap<String, tera::Value>| -> Result<Value, tera::Error> {
                match args.get("message") {
                    Some(val) => match from_value::<String>(val.clone()) {
                            Ok(v) => {
                                let e_req = ExecuteRequest{
                                    function_name: "echo".to_string(),
                                    args: hashmap!["message".to_string() => "hello world!".as_bytes().to_vec()]
                                };
        
                                let plugin_name = plugin_name.clone();

                                let res = block_on(async {
                                    let resp = plugin_actor.send(PluginRequest { plugin_name, request: e_req }).await;
                                    resp
                                }).unwrap();

                                let mut ret: Result<Value, tera::Error> = Err("unable to receive ret".into());
        
                                ret = Ok(to_value(res.unwrap()).unwrap());
                               
                                ret
                            },
                            Err(_) => Err("failed to call plugin 2".into())
                        },
                        None => Err("failed to call plugin 3".into())
                    }
                })
    }
}

#[actix::main]
async fn main() {
    let mut registry = PluginRegistry {
        plugins: HashMap::new()
    };
            
    let res = registry.load_plugin("/home/nrfrush/workspace/rust/plugin-registry-rs/examples/target/release/my-plugin".to_string()).await;
        
    let r_addr = RegistryActor{
        registry: registry.clone()
    }.start();

    let t_addr = SyncArbiter::start(2, move || TemplateActor {
        plugin_actor: r_addr.clone()
    });

    let res = t_addr.send(TemplateRequest { template_path: String::from("tera.test") }).await;
}



// impl Actor for RegistryActor {
//     type Context = Context<Self>;

//     fn started(&mut self, ctx: &mut Self::Context) {
//         println!("Starting");
//     }

//     fn stopped(&mut self, ctx: &mut Self::Context) {
//         println!("{:?}", self.registry.clone().reap_all());
//         println!("Stopping");
//     }
// }

// impl Handler<PluginRequest> for RegistryActor {
//     type Result = ResponseActFuture<Self, Result<HashMap<String, Vec<u8>>, String>>;

//     fn handle(&mut self, msg: PluginRequest, _ctx: &mut Self::Context) -> Self::Result {
//         let this = self.clone();
//         Box::pin(actix::fut::wrap_future(this.dispense_plugin(msg)))
//     }
// }

// impl RegistryActor {
//     async fn dispense_plugin(mut self, msg: PluginRequest) -> Result<HashMap<String, Vec<u8>>, String> {
//         let plugin_name = msg.plugin_name;
//         let request = msg.request;
//         let res = self.registry.dispense(plugin_name.clone()).await;

//         let mut client = res.unwrap();

//         let request = tonic::Request::new(request);

//         let res = client.execute(request).await;

//         return Ok(res.unwrap().into_inner().rets);
//     }
// }

// impl Actor for TemplateRenderActor {
//     type Context = Context<Self>;

//     fn started(&mut self, ctx: &mut Self::Context) {
//         println!("TemplateActor started");
//     }
// }

// impl Handler<TemplateRequest> for TemplateRenderActor {
//     type Result = ResponseFuture<Result<HashMap<String, Vec<u8>>, String>>;

//     fn handle(&mut self, _msg: TemplateRequest, _ctx: &mut Self::Context) -> Self::Result {
//         let e_req = ExecuteRequest{
//             function_name: "echo".to_string(),
//             args: hashmap!["message".to_string() => "hello world!".as_bytes().to_vec()]
//         };

//         let plugin_name = String::from("my_plugin");
//         let this = self.clone();
//         let res = Box::pin(async move {
//             let res = this.registry_addr.clone().send(PluginRequest { plugin_name: plugin_name, request: e_req }).await;
//             res.unwrap()
//         });
//         res
//     }
// }

// // impl Handler<MessageOne> for ActorTwo {
// //     type Result = ();

// //     fn handle(&mut self, msg: MessageOne, _ctx: &mut Self::Context) {
// //         println!("ActorTwo Received: {:?}", msg);
// //     }
// // }

// // #[derive(Clone, Debug, Message)]
// // #[rtype(result = "()")]
// // struct MessageOne(String);

// // #[derive(Clone, Debug, Message)]
// // #[rtype(result = "()")]
// // struct MessageTwo(u8);

// #[derive(Clone, Message, Debug)]
// #[rtype(result = "Result<HashMap<String, Vec<u8>>, String>")]
// struct PluginRequest {
//     plugin_name: String,
//     request: ExecuteRequest,
// }

// #[derive(Clone, Message, Debug)]
// #[rtype(result = "Result<HashMap<String, Vec<u8>>, String>")]
// struct TemplateRequest {
//     path: String
// }

// fn to_string(data: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
//     Ok(to_value(std::str::from_utf8(&from_value::<Vec<u8>>(data.clone()).unwrap()).unwrap().to_string()).unwrap())
// }

// fn test_function(plugin_name: String, a1: actix::Addr<TemplateRenderActor>) -> impl tera::Function {
//     Box::new(move |args: &HashMap<String, tera::Value>| -> Result<Value, tera::Error> {
//         match args.get("message") {
//             Some(val) => match from_value::<String>(val.clone()) {
//                     Ok(v) => {
//                         let e_req = ExecuteRequest{
//                             function_name: "echo".to_string(),
//                             args: hashmap!["message".to_string() => "hello world!".as_bytes().to_vec()]
//                         };

//                         let plugin_name = plugin_name.clone();

//                         let res = a1.send(TemplateRequest { path: String::from("") }).wait().unwrap().unwrap();

//                         let mut ret: Result<Value, tera::Error> = Err("unable to receive ret".into());

//                         ret = Ok(to_value(res.unwrap()).unwrap());
                       
//                         ret
//                     },
//                     Err(_) => Err("failed to call plugin 2".into())
//                 },
//                 None => Err("failed to call plugin 3".into())
//             }
//         })
// }

// #[actix::main]
// async fn main() {
//     let mut registry = PluginRegistry {
//         plugins: HashMap::new()
//     };
    
//     let res = registry.load_plugin("/home/nrfrush/workspace/rust/plugin-registry-rs/examples/target/release/my-plugin".to_string()).await;

//     println!("REGISTRY LOADED PLUGIN: {:?}", res);

//     let a3 = RegistryActor {
//             registry: registry.clone()
//     }.start();

//     let a1 = TemplateRenderActor {
//             registry_addr: a3.clone()
//     }.start();

//     //Grpc function here
//     let mut tera = tera::Tera::new("*").unwrap();

//     let context = tera::Context::new(); 

//     tera.register_filter("to_string", to_string);

//     tera.register_function("test_function", test_function("my_plugin".to_string(), a1.clone()));

//     let render = tera.render("test.tera", &context);

//     if render.is_ok() {
//         println!("{:?}", render.unwrap());
//     }

//     tokio::signal::ctrl_c().await.unwrap();
//     println!("🎩 Ctrl-C received, shutting down");
//     System::current().stop();
// }
