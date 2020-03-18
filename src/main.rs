extern crate redis;

use std::env;
use futures_util::TryStreamExt;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};

/// This is our service handler. It receives a Request, routes on its
/// path, and returns a Future of a Response.
async fn echo(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {

        (&Method::GET, "/") => Ok(Response::new(Body::from(
            "Message in a Bottle™",
        ))),

        (&Method::GET, "/health") => {
            match env::var("REDIS_URL") {
                Ok(_) => Ok(Response::new(Body::from("All good!"))),
                Err(e) => build_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
            }
        }

        // Simply echo the body back to the client.
        (&Method::POST, "/echo") => Ok(Response::new(req.into_body())),

        // Convert to uppercase before sending back to client using a stream.
        (&Method::POST, "/echo/uppercase") => {
            let chunk_stream = req.into_body().map_ok(|chunk| {
                chunk
                    .iter()
                    .map(|byte| byte.to_ascii_uppercase())
                    .collect::<Vec<u8>>()
            });
            Ok(Response::new(Body::wrap_stream(chunk_stream)))
        }

        // Reverse the entire body before sending back to the client.
        //
        // Since we don't know the end yet, we can't simply stream
        // the chunks as they arrive as we did with the above uppercase endpoint.
        // So here we do `.await` on the future, waiting on concatenating the full body,
        // then afterwards the content can be reversed. Only then can we return a `Response`.
        (&Method::POST, "/echo/reversed") => {
            let whole_body = hyper::body::to_bytes(req.into_body()).await?;

            let reversed_body = whole_body.iter().rev().cloned().collect::<Vec<u8>>();
            Ok(Response::new(Body::from(reversed_body)))
        }

        // Return the 404 Not Found for other routes.
        _ => build_response(StatusCode::NOT_FOUND, String::from("Not found"))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = get_addr_from_args(&env::args().collect());

    let service = make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(echo)) });

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
    Ok(Response::builder().status(code).body(Body::from(msg)).unwrap())
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