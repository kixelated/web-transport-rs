use bytes::Bytes;
use js_sys::Uint8Array;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    WebTransportBidirectionalStream, WebTransportCloseInfo, WebTransportReceiveStream,
    WebTransportSendStream,
};

use crate::{Reader, RecvStream, SendStream, WebError, Writer};

#[derive(Clone)]
pub struct Session {
    inner: web_sys::WebTransport,
}

impl Session {
    pub async fn new(url: &str) -> Result<Self, WebError> {
        let inner = web_sys::WebTransport::new(url)?;
        JsFuture::from(inner.ready()).await?;

        Ok(Self { inner })
    }

    async fn accept_uni(&mut self) -> Result<RecvStream, WebError> {
        let mut reader = Reader::new(&self.inner.incoming_unidirectional_streams())?;
        let stream: WebTransportReceiveStream = reader.read().await?.expect("closed without error");
        let recv = RecvStream::new(stream)?;
        Ok(recv)
    }

    async fn accept_bi(&mut self) -> Result<(SendStream, RecvStream), WebError> {
        let mut reader = Reader::new(&self.inner.incoming_bidirectional_streams())?;
        let stream: WebTransportBidirectionalStream =
            reader.read().await?.expect("closed without error");

        let send = SendStream::new(stream.writable())?;
        let recv = RecvStream::new(stream.readable())?;

        Ok((send, recv))
    }

    async fn open_bi(&mut self) -> Result<(SendStream, RecvStream), WebError> {
        let stream: WebTransportBidirectionalStream =
            JsFuture::from(self.inner.create_bidirectional_stream())
                .await?
                .dyn_into()?;

        let send = SendStream::new(stream.writable())?;
        let recv = RecvStream::new(stream.readable())?;

        Ok((send, recv))
    }

    async fn open_uni(&mut self) -> Result<SendStream, WebError> {
        let stream: WebTransportSendStream =
            JsFuture::from(self.inner.create_unidirectional_stream())
                .await?
                .dyn_into()?;

        let send = SendStream::new(stream)?;
        Ok(send)
    }

    async fn send_datagram(&mut self, payload: Bytes) -> Result<(), WebError> {
        let mut writer = Writer::new(&self.inner.datagrams().writable())?;
        writer.write(&Uint8Array::from(payload.as_ref())).await?;
        Ok(())
    }

    async fn recv_datagram(&mut self) -> Result<Bytes, WebError> {
        let mut reader = Reader::new(&self.inner.datagrams().readable())?;
        let data: Uint8Array = reader.read().await?.unwrap_or_default();
        Ok(data.to_vec().into())
    }

    fn close(self, code: u32, reason: &str) {
        let mut info = WebTransportCloseInfo::new();
        info.close_code(code);
        info.reason(reason);
        self.inner.close_with_close_info(&info);
    }

    async fn closed(&self) -> WebError {
        let err = JsFuture::from(self.inner.closed()).await.unwrap();
        WebError::from(err)
    }
}

#[async_trait::async_trait(?Send)]
impl webtransport_generic::Session for Session {
    type SendStream = SendStream;
    type RecvStream = RecvStream;
    type Error = WebError;

    async fn accept_uni(&mut self) -> Result<Self::RecvStream, Self::Error> {
        Session::accept_uni(self).await
    }

    async fn accept_bi(&mut self) -> Result<(Self::SendStream, Self::RecvStream), Self::Error> {
        Session::accept_bi(self).await
    }

    async fn open_bi(&mut self) -> Result<(Self::SendStream, Self::RecvStream), Self::Error> {
        Session::open_bi(self).await
    }

    async fn open_uni(&mut self) -> Result<Self::SendStream, Self::Error> {
        Session::open_uni(self).await
    }

    async fn send_datagram(&mut self, payload: Bytes) -> Result<(), Self::Error> {
        Session::send_datagram(self, payload).await
    }

    async fn recv_datagram(&mut self) -> Result<Bytes, Self::Error> {
        Session::recv_datagram(self).await
    }

    fn close(self, code: u32, reason: &str) {
        Session::close(self, code, reason)
    }

    async fn closed(&self) -> WebError {
        Session::closed(self).await
    }
}
