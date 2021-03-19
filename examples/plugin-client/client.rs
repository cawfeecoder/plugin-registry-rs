use plugin_registry::registry::plugin::{ExecuteRequest};
use plugin_registry::registry::plugin::plugin_client::PluginClient;
use plugin_registry::registry::PluginRegistry;
use tonic::{Request};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::str;
use tera::{Value, from_value, to_value, Context};
use once_cell::sync::Lazy;

#[macro_use]
extern crate plugin_registry;

static REGISTRY: Lazy<Arc<Mutex<PluginRegistry>>> = Lazy::new(|| {
    let mut registry = PluginRegistry {
        plugins: HashMap::new()
    };

    let future = async {
        let res = registry.load_plugin("/home/nfrush/workspace/rust/rust-plugin/examples/target/release/my-plugin".to_string()).await;
        res
    };

    let response = std::thread::spawn(|| {
        tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(future)
    }).join();

    Arc::new(Mutex::new(registry))
});

fn to_string(data: &Value, args: &HashMap<String, Value>) -> tera::Result<Value> {
    Ok(to_value(str::from_utf8(&from_value::<Vec<u8>>(data.clone()).unwrap()).unwrap().to_string()).unwrap())
}

fn get_plugin(plugin_name: String) -> PluginClient<tonic::transport::channel::Channel> {
    let plugin_name = plugin_name.clone();

    let registry = REGISTRY.clone();
    
    let future = async {
        let reg = registry.lock().unwrap();
        let res = reg.dispense(plugin_name).await;
        res
    };
        
    let response = std::thread::spawn(|| {
        tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(future)
    }).join();
        
    let plugin_client = match response {
        Ok(x) => {
            println!("dispensing plugin");
            Some(x)
        },
        Err(e) => {
            println!("Plugin doesn't exist dumb ass");
            None
        }
    };

    let client = plugin_client.unwrap().unwrap();
    return client;
}

fn test_function(plugin_name: String) -> impl tera::Function {
    let client = get_plugin(plugin_name);

    Box::new(move |args: &HashMap<String, tera::Value>| -> Result<Value, tera::Error> {
        match args.get("message") {
            Some(val) => match from_value::<String>(val.clone()) {
                Ok(v) => {
                    let e_req = ExecuteRequest{
                        function_name: "echo".to_string(),
                        args: hashmap!["message".to_string() => "hello world!".as_bytes().to_vec()]
                    };
                    let request = Request::new(e_req.clone());
                    let client = client.clone();
                    let (send, mut recv) = tokio::sync::oneshot::channel();
                    let future = async move {
                        let res = client.clone().execute(request).await;
                        send.send(res).unwrap();
                    };
                    if let Ok(handle) = tokio::runtime::Handle::try_current() {
                        handle.spawn(future);
                    } else {
                        let _res = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(future);
                    }

                    let mut response = Err("failed to call plugin 1".into());

                    for _ in 0..10 {
                        match recv.try_recv() {
                            Ok(val) => {
                                match val {
                                    Ok(v) => {
                                        let future = async move {
                                            let res = REGISTRY.reap_all().await;
                                        };

                                        if let Ok(handle) = tokio::runtime::Handle::try_current() {
                                            handle.spawn(future);
                                        } else {
                                            let _res = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap().block_on(future);
                                        }

                                        response = Ok(to_value(v.into_inner().rets).unwrap());
                                        break;
                                    },
                                    Err(e) => {
                                        println!("{:?}", e);;
                                        break;
                                    }
                                }
                            },
                            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                                break;
                            },
                            _ => {
                                std::thread::sleep(std::time::Duration::from_millis(30))
                            }
                        }
                    }
                    return response;
                },
                Err(_) => Err("failed to call plugin 2".into())
            },
            None => Err("failed to call plugin 3".into())
        }
    })
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    // let plugin_client = match registry.dispense("my_plugin".to_string()).await {
    //     Ok(x) => {
    //         println!("dispensing plugin");
    //         Some(x)
    //     },
    //     Err(e) => {
    //         println!("we encountered an error: {:?}", e);
    //         None
    //     }
    // };



    // if plugin_client.is_some() {
    //     let mut client = plugin_client.unwrap();

        let mut tera = tera::Tera::new("*").unwrap();

        let context = Context::new();

        tera.register_filter("to_string", to_string);

        tera.register_function("test_function", test_function("my_plugin".to_string()));

        match tera.render("test.tera", &context) {
            Ok(s) => println!("{:?}", s),
            Err(e) => {
                println!("Error: {:?}", e);
            }
        }
    // }
    //This should really either be watching the sigterm of this program for graceful shutdown or a LRU-type cache
    //should be implemented to reap plugins after a bit
    // REGISTRY.reap_all().await;

    Ok(())
}