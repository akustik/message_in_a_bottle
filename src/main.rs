mod message;
mod storage;
mod util;

use std::sync::mpsc;
use tokio::signal::unix::{signal, SignalKind};

use std::env;
use std::thread;
use std::sync::Arc;

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};

use message::DefaultChannel;

use storage::BottleMessage;
use storage::Storage;
use storage::RedisStorage;


async fn bottle(req: Request<Body>, storage: &RedisStorage) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {

        (&Method::GET, "/") => build_response(StatusCode::OK, String::from("Message in a Bottle™")),

        (&Method::GET, "/health") => {
            let result = storage.health();
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
                    let result = storage.store(bottle_message);
                    match result {
                        Ok(_) => build_response(StatusCode::OK, "Gotcha! ACK".to_string()),
                        Err(_) => build_response(StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong".to_string())
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
    
    let handle = thread::spawn(move || RedisStorage{}.subscribe(rx, &DefaultChannel{}).expect("Subscribe failed for Storage"));
    let addr = get_addr_from_args(&env::args().collect());

    let storage = Arc::new(RedisStorage{});
    let service = make_service_fn(|_| {
        let storage = Arc::clone(&storage);
        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                let storage = Arc::clone(&storage);
                async move {
                    bottle(req, &*storage).await
                }
            }))
        }
    });

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

fn get_addr_from_args(args: &Vec<String>) -> std::net::SocketAddr {
    let default_port = String::from("3000");
    let port =  args.get(1).unwrap_or(&default_port);
    let port: u16 = port.parse().expect(&format!("Invalid port: {}", port));

    ([0, 0, 0, 0], port).into()
}

fn build_response(code: StatusCode, msg: String) -> Result<Response<Body>, hyper::Error> {
    Ok(Response::builder().header("Content-Type", "text/plain; charset=utf-8").status(code).body(Body::from(msg)).unwrap())
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