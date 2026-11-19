use std::io::Read;
use std::time::Duration;
use url::Url;

pub(super) const BODY_LIMIT: usize = 1024 * 1024;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REDIRECT_START: u16 = 300;
const REDIRECT_END: u16 = 400;
const SUCCESS_START: u16 = 200;
const SUCCESS_END: u16 = 300;

pub(super) trait Backend: Send + Sync {
    fn get(&self, url: &Url, remaining: Duration) -> Result<Response, String>;
}

#[derive(Debug)]
pub(super) struct Response {
    pub location: Option<String>,
    pub content_type: String,
    pub body: Vec<u8>,
}

#[derive(Debug)]
pub(super) struct UreqBackend;

impl Backend for UreqBackend {
    fn get(&self, url: &Url, remaining: Duration) -> Result<Response, String> {
        let mut response = request(url, remaining)?;
        let status = response.status().as_u16();
        if (REDIRECT_START..REDIRECT_END).contains(&status) {
            return redirect(&response);
        }
        if !(SUCCESS_START..SUCCESS_END).contains(&status) {
            return Err(format!("web_read received HTTP {status}"));
        }
        let content_type = header(&response, "content-type").unwrap_or_default();
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take((BODY_LIMIT + 1) as u64)
            .read_to_end(&mut body)
            .map_err(|_| "web_read could not read the response body".to_owned())?;
        Ok(Response {
            location: None,
            content_type,
            body,
        })
    }
}

fn request(url: &Url, remaining: Duration) -> Result<ureq::http::Response<ureq::Body>, String> {
    if remaining.is_zero() {
        return Err("web_read exceeded its 10 second deadline".into());
    }
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(remaining))
        .timeout_connect(Some(CONNECT_TIMEOUT.min(remaining)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build();
    ureq::Agent::with_parts(
        config,
        ureq::unversioned::transport::DefaultConnector::default(),
        crate::safety::SafeResolver,
    )
    .get(url.as_str())
    .call()
    .map_err(network_error)
}

fn redirect(response: &ureq::http::Response<ureq::Body>) -> Result<Response, String> {
    let location = header(response, "location")
        .ok_or_else(|| "web_read received a redirect without a location".to_owned())?;
    Ok(Response {
        location: Some(location),
        content_type: String::new(),
        body: Vec::new(),
    })
}

fn header(response: &ureq::http::Response<ureq::Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

fn network_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::Timeout(_) => "web_read exceeded its deadline".into(),
        _ => "web_read could not fetch the URL".into(),
    }
}
