use bytes::Bytes;
use js_sys::{Reflect, Uint8Array};
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    WebTransportBidirectionalStream, WebTransportCloseInfo, WebTransportReceiveStream,
    WebTransportSendStream,
};

use crate::{Reader, RecvStream, SendStream, SessionError, WebErrorExt, Writer};

#[derive(Clone)]
pub struct Session {
    inner: web_sys::WebTransport,
}

impl Session {
    pub async fn connect(url: &str) -> Result<Self, SessionError> {
        let inner = web_sys::WebTransport::new(url).throw()?;
        JsFuture::from(inner.ready()).await.throw()?;

        Ok(Self { inner })
    }

    pub async fn accept_uni(&mut self) -> Result<RecvStream, SessionError> {
        let mut reader = Reader::new(&self.inner.incoming_unidirectional_streams())?;
        let stream: WebTransportReceiveStream = reader.read().await?.expect("closed without error");
        let recv = RecvStream::new(stream)?;
        Ok(recv)
    }

    pub async fn accept_bi(&mut self) -> Result<(SendStream, RecvStream), SessionError> {
        let mut reader = Reader::new(&self.inner.incoming_bidirectional_streams())?;
        let stream: WebTransportBidirectionalStream =
            reader.read().await?.expect("closed without error");

        let send = SendStream::new(stream.writable())?;
        let recv = RecvStream::new(stream.readable())?;

        Ok((send, recv))
    }

    pub async fn open_bi(&mut self) -> Result<(SendStream, RecvStream), SessionError> {
        let stream: WebTransportBidirectionalStream =
            JsFuture::from(self.inner.create_bidirectional_stream())
                .await
                .throw()?
                .into();

        let send = SendStream::new(stream.writable())?;
        let recv = RecvStream::new(stream.readable())?;

        Ok((send, recv))
    }

    pub async fn open_uni(&mut self) -> Result<SendStream, SessionError> {
        let stream: WebTransportSendStream =
            JsFuture::from(self.inner.create_unidirectional_stream())
                .await
                .throw()?
                .into();

        let send = SendStream::new(stream)?;
        Ok(send)
    }

    pub async fn send_datagram(&mut self, payload: Bytes) -> Result<(), SessionError> {
        let mut writer = Writer::new(&self.inner.datagrams().writable())?;
        writer.write(&Uint8Array::from(payload.as_ref())).await?;
        Ok(())
    }

    pub async fn recv_datagram(&mut self) -> Result<Bytes, SessionError> {
        let mut reader = Reader::new(&self.inner.datagrams().readable())?;
        let data: Uint8Array = reader.read().await?.unwrap_or_default();
        Ok(data.to_vec().into())
    }

    pub fn close(&mut self, closed: Closed) {
        let mut info = WebTransportCloseInfo::new();
        info.close_code(closed.code);
        info.reason(&closed.reason);
        self.inner.close_with_close_info(&info);
    }

    pub async fn closed(&self) -> Result<Closed, SessionError> {
        let result: js_sys::Object = JsFuture::from(self.inner.closed()).await.throw()?.into();

        // For some reason, WebTransportCloseInfo only contains setters
        let info = Closed {
            code: Reflect::get(&result, &"closeCode".into())
                .throw()?
                .as_f64()
                .unwrap() as u32,
            reason: Reflect::get(&result, &"reason".into())
                .throw()?
                .as_string()
                .unwrap(),
        };

        Ok(info)
    }
}

pub struct Closed {
    pub code: u32,
    pub reason: String,
}
