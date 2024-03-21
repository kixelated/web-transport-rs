use std::{error::Error, fmt, ops};

#[derive(Clone)]
pub struct SessionError(quinn::ConnectionError);

impl From<quinn::ConnectionError> for SessionError {
    fn from(error: quinn::ConnectionError) -> Self {
        SessionError(error)
    }
}

impl ops::Deref for SessionError {
    type Target = quinn::ConnectionError;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ops::DerefMut for SessionError {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, f)
    }
}

impl Error for SessionError {}

impl webtransport_generic::SessionError for SessionError {
    fn session_error(&self) -> Option<u32> {
        match &self.0 {
            quinn::ConnectionError::ApplicationClosed(msg) => {
                msg.error_code.into_inner().try_into().ok()
            }
            _ => None,
        }
    }
}
