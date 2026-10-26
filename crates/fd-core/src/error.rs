use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("i/o error on {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("could not decode image: {0}")]
    Image(#[from] image::ImageError),
    #[error("invalid document: {0}")]
    Document(String),
    #[error("invalid profile: {0}")]
    Profile(String),
    #[error("generation failed: {0}")]
    Generation(String),
    #[error(transparent)]
    Inject(#[from] crate::inject::InjectError),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}
