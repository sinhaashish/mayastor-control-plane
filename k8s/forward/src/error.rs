/// A kube-forward error.
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum Error {
    #[error("Service '{selector}' not found on '{namespace:?}'")]
    ServiceNotFound {
        selector: String,
        namespace: crate::NameSpace,
    },
    #[error("{source}")]
    Kube { source: kube::Error },
    #[error("{source}")]
    AnyHow { source: anyhow::Error },
    #[error("Invalid uri: {source}")]
    InvalidUri {
        source: hyper::http::uri::InvalidUri,
    },
    #[error("{source}")]
    Io { source: std::io::Error },
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

impl From<hyper::http::uri::InvalidUri> for Error {
    fn from(source: hyper::http::uri::InvalidUri) -> Self {
        Self::InvalidUri { source }
    }
}
