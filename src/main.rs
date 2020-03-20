extern crate redis;

use std::time::Duration;
use std::sync::mpsc::{self, TryRecvError};
use tokio::signal::unix::{signal, SignalKind};
use serde::{Serialize, Deserialize};
use std::env;
use std::thread;
use redis::Commands;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use reqwest::blocking::ClientBuilder as BlockingClientBuilder;

#[derive(Serialize, Deserialize, Debug, Hash)]
struct BottleMessage {
    msg: String
}

#[derive(Serialize, Deserialize, Debug)]
struct SendGridMessage {
    personalizations: [SendGridMessagePersonalizations; 1],
    from: SendGridMessageAddress,
    reply_to: SendGridMessageAddress,
    template_id: String
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct SendGridMessageAddress {
    email: String,
    name: String
}

#[derive(Serialize, Deserialize, Debug)]
struct SendGridMessagePersonalizations {
    to: [SendGridMessageAddress; 1],
    dynamic_template_data: SendGridMessageDynamicTemplateData
}

#[derive(Serialize, Deserialize, Debug)]
struct SendGridMessageDynamicTemplateData {
    msg: String
}

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn bottle(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {

        (&Method::GET, "/") => build_response(StatusCode::OK, String::from("Message in a Bottle™")),

        (&Method::GET, "/health") => {
            let result: redis::RedisResult<String> = execute_redis_command(|con: &mut redis::Connection| {
                redis::cmd("SETEX").arg("health").arg(1).arg(42).query(con)
            });
            match result {
                Ok(_) => build_response(StatusCode::OK, String::from("All good!")),
                Err(e) => build_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        }

        (&Method::POST, "/msg") => {
            let body =  hyper::body::to_bytes(req.into_body()).await?;
            let body = body.iter().cloned().collect::<Vec<u8>>();
            let body = std::str::from_utf8(&body).expect("Unable to parse body");
            let parsed: Result<BottleMessage, serde_json::error::Error> = serde_json::from_str(body);

            match parsed {
                Ok(bottle_message) => {
                    let hash = calculate_hash(&bottle_message);
                    let expire_hash = format!("trigger:{}", hash);
                    let trigger_expiration_in_seconds: usize = 1;
                    let key_expiration_in_seconds: usize = 60;
                    let result: redis::RedisResult<()> = execute_redis_command(|con: &mut redis::Connection| {
                        redis::pipe().atomic()
                        .set_ex(expire_hash, "", trigger_expiration_in_seconds)
                        .set_ex(hash, &bottle_message.msg, key_expiration_in_seconds)
                        .query(con)
                    });
                    match result {
                        Ok(_) => build_response(StatusCode::OK, String::from(format!("Gotcha! ACK {}", hash))),
                        Err(e) => {
                            println!("Error while setting a msg: '{}'", e);
                            build_response(StatusCode::INTERNAL_SERVER_ERROR, String::from("Something went wrong"))
                        }
                    }
                },
                Err(_) => build_response(StatusCode::BAD_REQUEST, String::from("Invalid bottle"))
            }
        }

        _ => build_response(StatusCode::NOT_FOUND, String::from("Not found"))
    }
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (tx, rx) = mpsc::channel::<String>();

    let handle = thread::spawn(move || {
        listen_to_redis_expiration_notifications(rx);
    });

    let addr = get_addr_from_args(&env::args().collect());
    let service = make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(bottle)) });
    let server = Server::bind(&addr)
        .serve(service)
        .with_graceful_shutdown(shutdown_signal());
    
    println!("Listening on http://{}", addr);
    server.await?;

    println!("Shutting down...");

    tx.send(String::from("Die")).expect("Unable to send dead letter to background thread");

    handle.join().unwrap();

    println!("All done, stopping.");
    Ok(())
}

async fn shutdown_signal() {
    let stream = signal(SignalKind::terminate());
    match stream {
        Ok(mut s) => {
            s.recv().await;
        },
        _ => {
            println!("Unablet to set graceful shutdown");
        }
    }
}

fn send_email(msg: String) {
    let api_key = env::var("SENDGRID_API_KEY").expect("$SENDGRID_API_KEY");

    let msg = SendGridMessage {
        from: SendGridMessageAddress {
            email: "towalkaway@gmail.com".to_string(),
            name: "Message in a Bottle".to_string()
        },
        reply_to: SendGridMessageAddress {
            email: "towalkaway@gmail.com".to_string(),
            name: "Message in a Bottle".to_string()
        },
        template_id: "d-3e85b81589ac4a76947baa9b13e2dc05".to_string(),
        personalizations: [ SendGridMessagePersonalizations {
            to: [SendGridMessageAddress {
                email: "towalkaway@gmail.com".to_string(),
                name: "Message in a Bottle".to_string()
            }],
            dynamic_template_data: SendGridMessageDynamicTemplateData {
                msg: msg
            }
        }]
    };

    let json_content = serde_json::to_string_pretty(&msg).unwrap();
    println!("Going to send an email: {}", json_content);

    let client = BlockingClientBuilder::new()
        .danger_accept_invalid_certs(true)
        .build().expect("Unable to create sync client");
    
    let response = client
        .post("https://api.sendgrid.com/v3/mail/send")
        .bearer_auth(api_key)
        .json(&msg)
        .send();
    
    match response {
        Ok(r) => println!("Message sent, status: {}, response: {}", r.status(), r.text().unwrap()),
        Err(e) => println!("Unable to send message: {}", e)
    }   
}

fn listen_to_redis_expiration_notifications(rx: mpsc::Receiver<String>) {
    execute_redis_command(|con: &mut redis::Connection| {
        let config_parameter = "notify-keyspace-events";
        let config_value = "Exg";
        let config: redis::RedisResult<()> = redis::cmd("CONFIG")
        .arg("SET")
        .arg(config_parameter)
        .arg(config_value)
        .query(con);

        println!("config :set {} {}: '{}'", 
            config_parameter, 
            config_value, 
            config.is_ok()
        );

        let mut pubsub = con.as_pubsub();
        pubsub.psubscribe("__keyevent@0__:expired").expect("Subscription failed");
        pubsub.set_read_timeout(Some(Duration::from_millis(5000))).expect("Unable to set read timeout");

        println!("Background thread set: listening for notifications");

        loop {
            let msg = pubsub.get_message();

            match msg {
                Ok(m) => {
                    let payload : String = m.get_payload()?;
                    println!("channel '{}': payload '{}'", m.get_channel_name(), payload);

                    if payload.starts_with("trigger:") {
                        let key = payload.replace("trigger:", "");
                        let msg: String = execute_redis_command(|con2: &mut redis::Connection| con2.get(key))?;
                        send_email(msg);
                    }
                }
                Err(e) => println!("No notifications, {}", e)
            }

            match rx.try_recv() {
                Ok(_) | Err(TryRecvError::Disconnected) => {
                    break;
                }
                Err(TryRecvError::Empty) => {}
            }
        }

        println!("Terminating thread.");
        Ok(String::from("Terminated"))
    }).expect("Unable to loop for messages");
}

fn get_addr_from_args(args: &Vec<String>) -> std::net::SocketAddr {
    let default_port = String::from("3000");
    let port =  args.get(1).unwrap_or(&default_port);
    let port: u16 = port.parse().expect(&format!("Invalid port: {}", port));

    ([0, 0, 0, 0], port).into()
}

fn build_response(code: StatusCode, msg: String) -> Result<Response<Body>, hyper::Error> {
    Ok(Response::builder().header("Content-Type", "text/plain; charset=utf-8").status(code).body(Body::from(msg)).unwrap())
}

fn execute_redis_command<T, C: FnOnce(&mut redis::Connection) -> redis::RedisResult<T>>(command: C) -> redis::RedisResult<T> {
    let url = env::var("REDISCLOUD_URL").expect("$REDISCLOUD_URL");
    let client = redis::Client::open(url)?;
    let mut con = client.get_connection()?;
    command(&mut con)
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

#[cfg(test)]
mod tests {
    use std::net;
    use super::*;

    #[test]
    fn test_get_addr_from_args_with_no_args() {
        let expected = net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(0, 0, 0, 0)), 3000);
        let actual = get_addr_from_args(&Vec::new());
        
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_get_addr_from_args_with_custom_port() {
        let expected = net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(0, 0, 0, 0)), 5000);
        let actual = get_addr_from_args(&[String::from("program_name"), String::from("5000")].to_vec());
        
        assert_eq!(actual, expected);
    }

    #[test]
    #[should_panic(expected = "Invalid port: 5aaa: ParseIntError { kind: InvalidDigit }")]
    fn test_get_addr_from_args_with_invalid_custom_port() {
        let expected = net::SocketAddr::new(net::IpAddr::V4(net::Ipv4Addr::new(0, 0, 0, 0)), 5000);
        let actual = get_addr_from_args(&[String::from("program_name"), String::from("5aaa")].to_vec());
        
        assert_eq!(actual, expected);
    }

}