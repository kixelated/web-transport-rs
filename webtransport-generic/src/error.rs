use std::error::Error;
use std::fmt::Debug;

// An error that optionally contains a QUIC error code.
pub trait ErrorCode: Error + Debug + 'static {
    fn code(&self) -> Option<u32>;
}
