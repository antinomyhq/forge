#[cfg(test)]
pub mod server {
    use std::sync::Arc;

    use hyper::header::{HeaderValue, CONTENT_TYPE};
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Request, Response, Server};
    use tokio::sync::oneshot;

    pub struct TestServer {
        port: u16,
        request_path: String,
        response_fn: Arc<dyn Fn(Request<Body>) -> Vec<String> + Send + Sync>,
    }

    impl TestServer {
        /// Create a new test server instance.
        pub fn new<F>(port: u16, request_path: &str, response_fn: F) -> Self
        where
            F: Fn(Request<Body>) -> Vec<String> + Send + Sync + 'static,
        {
            Self {
                port,
                request_path: request_path.to_string(),
                response_fn: Arc::new(response_fn),
            }
        }

        /// Start the test server.
        pub async fn start(self) -> oneshot::Sender<()> {
            let (tx, rx) = oneshot::channel();
            let addr = ([127, 0, 0, 1], self.port).into();
            let response_fn = self.response_fn.clone();
            let request_path = self.request_path.clone();

            let service = make_service_fn(move |_| {
                let response_fn = response_fn.clone();
                let request_path = request_path.clone();
                async move {
                    Ok::<_, hyper::Error>(service_fn(move |req: Request<Body>| {
                        let response_fn = response_fn.clone();
                        let request_path = request_path.clone();
                        async move {
                            if req.uri().path() == request_path {
                                let chunks = (response_fn)(req);
                                let stream = tokio_stream::iter(
                                    chunks.into_iter().map(Ok::<_, std::io::Error>),
                                );
                                let body = Body::wrap_stream(stream);
                                let mut response = Response::new(body);
                                response.headers_mut().insert(
                                    CONTENT_TYPE,
                                    HeaderValue::from_static("application/json"),
                                );
                                Ok::<_, hyper::Error>(response)
                            } else {
                                Ok::<_, hyper::Error>(
                                    Response::builder()
                                        .status(404)
                                        .body(Body::from("Not Found"))
                                        .unwrap(),
                                )
                            }
                        }
                    }))
                }
            });

            let server = Server::bind(&addr)
                .serve(service)
                .with_graceful_shutdown(async {
                    rx.await.ok();
                });

            tokio::spawn(server);
            tx
        }
    }
}
