use crate::grpc::{Request, Response, SRequest};
use futures::Stream;
use hyper::{client::HttpConnector, Uri};

#[async_trait::async_trait]
pub trait GrpcClient {
    async fn unary<M1, M2>(
        &mut self,
        path: &str,
        request: Request<M1>,
    ) -> anyhow::Result<Response<M2>>
    where
        M1: prost::Message,
        M2: prost::Message + Default + 'static;

    async fn streaming<M1, M2, S>(
        &mut self,
        path: &str,
        request: SRequest<S>,
    ) -> anyhow::Result<Response<M2>>
    where
        S: Stream<Item = M1> + Send + 'static,
        M1: prost::Message,
        M2: prost::Message + Default + 'static;
}

pub struct Client {
    inner: hyper::Client<hyper_rustls::HttpsConnector<HttpConnector>>,
    base_uri: Uri,
}

impl Client {
    pub fn new(base_uri: Uri) -> Self {
        let https = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http2()
            .build();

        Client {
            base_uri,
            inner: hyper::Client::builder().http2_only(true).build(https),
        }
    }
}

#[async_trait::async_trait]
impl GrpcClient for Client {
    async fn unary<M1, M2>(
        &mut self,
        path: &str,
        request: Request<M1>,
    ) -> anyhow::Result<Response<M2>>
    where
        M1: prost::Message,
        M2: prost::Message + Default + 'static,
    {
        let mut req = request.into_http();

        let uri = hyper::Uri::builder()
            .scheme(self.base_uri.scheme().unwrap().clone())
            .authority(self.base_uri.authority().unwrap().clone())
            .path_and_query(path)
            .build()
            .unwrap();

        *req.uri_mut() = uri;
        let resp = self.inner.request(req).await?;

        let resp = Response::from_http(resp);

        Ok(resp)
    }

    async fn streaming<M1, M2, S>(
        &mut self,
        path: &str,
        request: SRequest<S>,
    ) -> anyhow::Result<Response<M2>>
    where
        S: Stream<Item = M1> + Send + 'static,
        M1: prost::Message,
        M2: prost::Message + Default + 'static,
    {
        let mut req = request.into_http();

        let uri = hyper::Uri::builder()
            .scheme(self.base_uri.scheme().unwrap().clone())
            .authority(self.base_uri.authority().unwrap().clone())
            .path_and_query(path)
            .build()
            .unwrap();

        *req.uri_mut() = uri;
        let resp = self.inner.request(req).await?;

        let resp = Response::from_http(resp);

        Ok(resp)
    }
}
