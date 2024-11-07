/// A kube-proxy error.
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum Error {
    #[error("{source}")]
    Forward { source: kube_forward::Error },
    #[error("{source}")]
    Kube { source: kube::Error },
    #[error("{source}")]
    AnyHow { source: anyhow::Error },
    #[error("Invalid url: {source}")]
    InvalidUrl { source: url::ParseError },
    #[error("Invalid uri: {source}")]
    InvalidUri {
        source: hyper::http::uri::InvalidUri,
    },
}

impl From<kube_forward::Error> for Error {
    fn from(source: kube_forward::Error) -> Self {
        Self::Forward { source }
    }
}

impl From<anyhow::Error> for Error {
    fn from(source: anyhow::Error) -> Self {
        Self::AnyHow { source }
    }
}

impl From<kube::Error> for Error {
    fn from(source: kube::Error) -> Self {
        Self::Kube { source }
    }
}

impl From<url::ParseError> for Error {
    fn from(source: url::ParseError) -> Self {
        Self::InvalidUrl { source }
    }
}

impl From<hyper::http::uri::InvalidUri> for Error {
    fn from(source: hyper::http::uri::InvalidUri) -> Self {
        Self::InvalidUri { source }
    }
}
