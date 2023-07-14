use bytes::{Buf, BufMut};

use crate::{VarInt, VarIntUnexpectedEnd};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Frame(pub VarInt);

impl Frame {
    pub fn decode<B: Buf>(buf: &mut B) -> Result<Self, VarIntUnexpectedEnd> {
        Ok(Frame(VarInt::decode(buf)?))
    }

    pub fn encode<B: BufMut>(&self, buf: &mut B) {
        self.0.encode(buf)
    }
}

macro_rules! frames {
    {$($name:ident = $val:expr,)*} => {
        impl Frame {
            $(pub const $name: Frame = Frame(VarInt::from_u32($val));)*
        }
    }
}

// Sent at the start of a bidirectional stream.
frames! {
    DATA = 0x00,
    HEADERS = 0x01,
    SETTINGS = 0x04,
    WEBTRANSPORT = 0x41,
}
