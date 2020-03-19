extern crate redis;

use serde::{Serialize, Deserialize};
use std::env;
use redis::Commands;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};


#[derive(Serialize, Deserialize, Debug, Hash)]
struct BottleMessage {
    msg: String
}

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn bottle(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {

        (&Method::GET, "/") => build_response(StatusCode::OK, String::from("Message in a Bottle™")),

        (&Method::GET, "/health") => {
            match execute_redis_command(|con: &mut redis::Connection| con.set_ex("health", 42, 1)) {
                Ok(_) => build_response(StatusCode::OK, String::from("All good!")),
                Err(e) => build_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        }

        (&Method::GET, "/config") => {
            execute_redis_command(|con: &mut redis::Connection| {
                let config: redis::RedisResult<()> = redis::cmd("CONFIG")
                .arg("SET")
                .arg("notify-keyspace-events")
                .arg("Exg")
                .query(con);

                println!("config set notify-keyspace-events xE: '{}'", config.is_ok());
  
                let mut pubsub = con.as_pubsub();
                pubsub.psubscribe("__keyevent@0__:expire").expect("Subscription failed");
                loop {
                    let msg = pubsub.get_message()?;
                    let payload : String = msg.get_payload()?;
                    println!("channel '{}': payload '{}'", msg.get_channel_name(), payload);
                }
            }).expect("Unable to loop for messages");

            build_response(StatusCode::OK, String::from("Done"))
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
                    let expiration_in_seconds = 1;
                    match execute_redis_command(|con: &mut redis::Connection| {
                        redis::pipe().atomic()
                        .set_ex(expire_hash, "", expiration_in_seconds).ignore()
                        .set(hash, &bottle_message.msg)
                        .query(con)
                    }) {
                        Ok(_) => build_response(StatusCode::OK, String::from(format!("Gotcha! ACK {}", hash))),
                        Err(_) => build_response(StatusCode::INTERNAL_SERVER_ERROR, String::from("Something went wrong"))
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
    let addr = get_addr_from_args(&env::args().collect());

    let service = make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(bottle)) });

    let server = Server::bind(&addr).serve(service);

    println!("Listening on http://{}", addr);

    server.await?;

    Ok(())
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

fn execute_redis_command<C>(command: C) -> redis::RedisResult<()> where C: FnOnce(&mut redis::Connection) -> redis::RedisResult<()> {
    let url = env::var("REDIS_URL").expect("$REDIS_URL");
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