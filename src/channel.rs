use hyper::{client::HttpConnector, Body, Request, Response, Uri};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};

use crate::status::{Code, Status};

#[derive(Debug, Clone)]
pub struct Channel {
    client: hyper::Client<HttpsConnector<HttpConnector>>,
    uris: Vec<Uri>,
}

impl Channel {
    pub fn new(target: impl Iterator<Item = impl AsRef<str>>) -> Result<Self, Status> {
        let mut uris = Vec::new();

        for item in target {
            let uri = Uri::try_from(item.as_ref())
                .map_err(|err| Status::new(Code::InvalidArgument, "invalid uri").with_cause(err))?;

            if uri.authority().is_none() || uri.scheme().is_none() {
                return Err(Status::new(Code::InvalidArgument, "invalid uri"));
            }
            uris.push(uri);
        }

        if uris.is_empty() {
            return Err(Status::new(Code::InvalidArgument, "invalid target"));
        }

        let https = HttpsConnectorBuilder::new()
            .with_native_roots()
            .https_or_http()
            .enable_http2()
            .build();

        let client = hyper::Client::builder().http2_only(true).build(https);

        Ok(Channel { client, uris })
    }

    pub async fn call(
        &mut self,
        method: &str,
        mut request: Request<Body>,
    ) -> Result<Response<Body>, Status> {
        // load balance select
        let i = rand::random::<usize>() % self.uris.len();

        let uri = self
            .uris
            .get(i)
            .ok_or_else(||Status::new(Code::Internal, "no avaliable target"))?;

        let uri = Uri::builder()
            .scheme(uri.scheme().expect("scheme must not empty").clone())
            .authority(uri.authority().expect("authority must not empty").clone())
            .path_and_query(method)
            .build()
            .map_err(|err| Status::new(Code::InvalidArgument, "uri error").with_cause(err))?;

        *request.uri_mut() = uri;

        let resp = self.client.request(request).await?;

        Ok(resp)
    }
}
