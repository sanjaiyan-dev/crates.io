use super::prelude::*;
use crate::util::errors::{forbidden, internal, AppError, AppResult};
use http::request::Parts;
use http::{Extensions, HeaderMap, HeaderValue, Method, Request, Uri, Version};

/// The Origin header (<https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Origin>)
/// is sent with CORS requests and POST requests, and indicates where the request comes from.
/// We don't want to accept authenticated requests that originated from other sites, so this
/// function returns an error if the Origin header doesn't match what we expect "this site" to
/// be: <https://crates.io> in production, or <http://localhost:port/> in development.
pub fn verify_origin<T: RequestPartsExt>(req: &T) -> AppResult<()> {
    let headers = req.headers();
    let allowed_origins = &req.app().config.allowed_origins;

    let bad_origin = headers
        .get_all(header::ORIGIN)
        .iter()
        .find(|value| !allowed_origins.contains(value));

    if let Some(bad_origin) = bad_origin {
        let error_message =
            format!("only same-origin requests can be authenticated. got {bad_origin:?}");
        return Err(internal(&error_message).chain(forbidden()));
    }
    Ok(())
}

pub trait RequestPartsExt {
    fn method(&self) -> &Method;
    fn uri(&self) -> &Uri;
    fn version(&self) -> Version;
    fn headers(&self) -> &HeaderMap<HeaderValue>;
    fn extensions(&self) -> &Extensions;
    fn extensions_mut(&mut self) -> &mut Extensions;
}

impl RequestPartsExt for Parts {
    fn method(&self) -> &Method {
        &self.method
    }
    fn uri(&self) -> &Uri {
        &self.uri
    }
    fn version(&self) -> Version {
        self.version
    }
    fn headers(&self) -> &HeaderMap<HeaderValue> {
        &self.headers
    }
    fn extensions(&self) -> &Extensions {
        &self.extensions
    }
    fn extensions_mut(&mut self) -> &mut Extensions {
        &mut self.extensions
    }
}

impl<B> RequestPartsExt for Request<B> {
    fn method(&self) -> &Method {
        self.method()
    }
    fn uri(&self) -> &Uri {
        self.uri()
    }
    fn version(&self) -> Version {
        self.version()
    }
    fn headers(&self) -> &HeaderMap<HeaderValue> {
        self.headers()
    }
    fn extensions(&self) -> &Extensions {
        self.extensions()
    }
    fn extensions_mut(&mut self) -> &mut Extensions {
        self.extensions_mut()
    }
}

impl RequestPartsExt for BytesRequest {
    fn method(&self) -> &Method {
        self.0.method()
    }
    fn uri(&self) -> &Uri {
        self.0.uri()
    }
    fn version(&self) -> Version {
        self.0.version()
    }
    fn headers(&self) -> &HeaderMap<HeaderValue> {
        self.0.headers()
    }
    fn extensions(&self) -> &Extensions {
        self.0.extensions()
    }
    fn extensions_mut(&mut self) -> &mut Extensions {
        self.0.extensions_mut()
    }
}
