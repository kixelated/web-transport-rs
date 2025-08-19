use std::{
    collections::{hash_map, HashMap},
    ops::RangeInclusive,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

use bytes::{Buf, BufMut, Bytes, BytesMut};
use futures::{SinkExt, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{mpsc, watch},
};
use tokio_tungstenite::{
    tungstenite::{client::IntoClientRequest, handshake::server, http, Message},
    WebSocketStream,
};
use web_transport_generic as generic;
use web_transport_proto::{VarInt, VarIntUnexpectedEnd};

pub use tokio_tungstenite;
pub use tokio_tungstenite::tungstenite;

// We use this ALPN to identify our WebTransport compatibility layer.
pub const ALPN: &str = "web-transport";

#[derive(Debug, thiserror::Error, Clone)]
pub enum Error {
    #[error("websocket error: {0}")]
    WebSocket(String),

    #[error("invalid frame type: {0}")]
    InvalidFrameType(VarInt),

    #[error("protocol violation: {0}")]
    ProtocolViolation(String),

    #[error("stream closed")]
    StreamClosed,

    #[error("connection closed: {code}: {reason}")]
    ConnectionClosed { code: VarInt, reason: String },

    #[error("stream reset: {0}")]
    StreamReset(VarInt),

    #[error("stream stop: {0}")]
    StreamStop(VarInt),

    #[error("short frame")]
    Short,

    #[error("connection closed")]
    Closed,
}

impl From<VarIntUnexpectedEnd> for Error {
    fn from(_: VarIntUnexpectedEnd) -> Self {
        Self::Short
    }
}

impl From<tokio_tungstenite::tungstenite::Error> for Error {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::WebSocket(err.to_string())
    }
}

impl generic::Error for Error {}

/// Emulates a WebTransport session over a WebSocket connection.
#[derive(Clone)]
pub struct Session {
    is_server: bool,

    outbound: mpsc::Sender<Frame>,
    outbound_priority: mpsc::UnboundedSender<Frame>,

    accept_bi: Arc<tokio::sync::Mutex<mpsc::Receiver<(SendStream, RecvStream)>>>,
    accept_uni: Arc<tokio::sync::Mutex<mpsc::Receiver<RecvStream>>>,

    create_uni: mpsc::Sender<(StreamId, SendState)>,
    create_bi: mpsc::Sender<(StreamId, SendState, RecvState)>,

    create_uni_id: Arc<AtomicU64>,
    create_bi_id: Arc<AtomicU64>,

    closed: watch::Sender<Option<Error>>,
}

struct SessionState<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> {
    ws: WebSocketStream<T>,
    is_server: bool,

    outbound: (mpsc::Sender<Frame>, mpsc::Receiver<Frame>),
    outbound_priority: (mpsc::UnboundedSender<Frame>, mpsc::UnboundedReceiver<Frame>),

    accept_bi: mpsc::Sender<(SendStream, RecvStream)>,
    accept_uni: mpsc::Sender<RecvStream>,

    create_uni: mpsc::Receiver<(StreamId, SendState)>,
    create_bi: mpsc::Receiver<(StreamId, SendState, RecvState)>,

    send_streams: HashMap<StreamId, SendState>,
    recv_streams: HashMap<StreamId, RecvState>,

    closed: watch::Sender<Option<Error>>,
}

impl<T: AsyncRead + AsyncWrite + Unpin + Send + 'static> SessionState<T> {
    async fn run(&mut self) -> Result<(), Error> {
        let mut closed = self.closed.subscribe();

        loop {
            tokio::select! {
                biased;
                message = self.ws.next() => {
                    match message {
                        Some(Ok(Message::Binary(data))) => {
                            let frame = Frame::decode(data.into())?;
                            self.recv_frame(frame).await?;
                        },
                        None => return Err(Error::Closed),
                        _ => continue,
                    };
                }
                Some((id, send)) = self.create_uni.recv() => {
                    self.send_streams.insert(id, send);
                }
                Some((id, send, recv)) = self.create_bi.recv() => {
                    self.send_streams.insert(id, send);
                    self.recv_streams.insert(id, recv);
                }
                frame = self.outbound_priority.1.recv() => {
                    match frame {
                        Some(frame) => self.send_frame(frame).await?,
                        None => return Err(Error::Closed),
                    };
                }
                frame = self.outbound.1.recv() => {
                    match frame {
                        Some(frame) => self.send_frame(frame).await?,
                        None => return Err(Error::Closed),
                    };
                }
                _ = async { closed.wait_for(|err| err.is_some()).await.ok(); } => {
                    return Err(closed.borrow().clone().unwrap_or(Error::Closed))
                }
            }
        }
    }

    async fn send_frame(&mut self, frame: Frame) -> Result<(), Error> {
        // Update our state first.
        match &frame {
            Frame::ResetStream(reset) => {
                self.send_streams.remove(&reset.id);
            }
            Frame::Stream(stream) if stream.fin => {
                self.send_streams.remove(&stream.id);
            }
            Frame::StopSending(stop) => {
                self.recv_streams.remove(&stop.id);
            }
            _ => {}
        };

        let data = frame.encode();
        self.ws
            .send(Message::Binary(data.to_vec()))
            .await
            .map_err(|_| Error::Closed)?;

        Ok(())
    }

    async fn recv_frame(&mut self, frame: Frame) -> Result<(), Error> {
        match frame {
            Frame::Padding | Frame::Ping => {
                // These frames are no-ops in our implementation
            }
            Frame::Stream(stream) => {
                if !stream.id.can_recv(self.is_server) {
                    return Err(Error::ProtocolViolation("invalid stream id".into()));
                }

                let mut state = match self.recv_streams.entry(stream.id) {
                    hash_map::Entry::Vacant(e) => {
                        if self.is_server == stream.id.server_initiated() {
                            // Already closed, ignore it. TODO slightly wrong
                            return Ok(());
                        }

                        let (tx, rx) = mpsc::unbounded_channel();
                        let (tx2, rx2) = mpsc::unbounded_channel();

                        let recv_backend = RecvState {
                            inbound_data: tx,
                            inbound_reset: tx2,
                        };

                        let recv_frontend = RecvStream {
                            id: stream.id,
                            inbound_data: rx,
                            inbound_reset: rx2,
                            outbound_priority: self.outbound_priority.0.clone(),
                            buffer: Bytes::new(),
                            offset: 0,
                            closed: None,
                            fin: false,
                        };

                        match stream.id.dir() {
                            Dir::Uni => {
                                self.accept_uni
                                    .send(recv_frontend)
                                    .await
                                    .map_err(|_| Error::Closed)?;
                            }
                            Dir::Bi => {
                                let (tx, rx) = mpsc::unbounded_channel();
                                let send_backend = SendState {
                                    inbound_stopped: tx,
                                };

                                let send_frontend = SendStream {
                                    id: stream.id,
                                    outbound: self.outbound.0.clone(),
                                    outbound_priority: self.outbound_priority.0.clone(),
                                    inbound_stopped: rx,
                                    offset: 0,
                                    closed: None,
                                    fin: false,
                                };

                                self.send_streams.insert(stream.id, send_backend);
                                self.accept_bi
                                    .send((send_frontend, recv_frontend))
                                    .await
                                    .map_err(|_| Error::Closed)?;
                            }
                        };

                        e.insert_entry(recv_backend)
                    }
                    hash_map::Entry::Occupied(e) => e,
                };

                let fin = stream.fin;
                state.get_mut().inbound_data.send(stream).ok();
                if fin {
                    state.remove();
                }
            }
            Frame::ResetStream(reset) => {
                if !reset.id.can_recv(self.is_server) {
                    return Err(Error::ProtocolViolation("invalid stream id".into()));
                }

                match self.recv_streams.entry(reset.id) {
                    hash_map::Entry::Occupied(mut e) => {
                        e.get_mut().inbound_reset.send(reset).ok();
                        e.remove();
                    }
                    // Already closed. TODO slightly wrong
                    _ => {}
                };
            }
            Frame::StopSending(stop) => {
                if !stop.id.can_send(self.is_server) {
                    return Err(Error::ProtocolViolation("invalid stream id".into()));
                }

                if let Some(stream) = self.send_streams.get_mut(&stop.id) {
                    stream.inbound_stopped.send(stop).ok();
                }
            }
            Frame::ConnectionClose(_close) => {
                todo!("close connection");
            }
        }

        Ok(())
    }
}

impl Session {
    pub fn new<T: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        ws: WebSocketStream<T>,
        is_server: bool,
    ) -> Self {
        let (accept_bi_tx, accept_bi_rx) = mpsc::channel(1024);
        let (accept_uni_tx, accept_uni_rx) = mpsc::channel(1024);

        let (create_uni_tx, create_uni_rx) = mpsc::channel(8);
        let (create_bi_tx, create_bi_rx) = mpsc::channel(8);

        let (outbound_tx, outbound_rx) = mpsc::channel(8);
        let (outbound_priority_tx, outbound_priority_rx) = mpsc::unbounded_channel();

        let closed = watch::Sender::new(None);

        let mut backend = SessionState {
            ws,
            outbound: (outbound_tx.clone(), outbound_rx),
            outbound_priority: (outbound_priority_tx.clone(), outbound_priority_rx),
            accept_bi: accept_bi_tx,
            accept_uni: accept_uni_tx,
            create_uni: create_uni_rx,
            create_bi: create_bi_rx,
            is_server,
            send_streams: HashMap::new(),
            recv_streams: HashMap::new(),
            closed: closed.clone(),
        };
        tokio::spawn(async move {
            let err = backend.run().await.err().unwrap_or(Error::Closed);
            backend.closed.send(Some(err)).ok();
        });

        Session {
            is_server,
            outbound: outbound_tx,
            outbound_priority: outbound_priority_tx,
            accept_bi: Arc::new(tokio::sync::Mutex::new(accept_bi_rx)),
            accept_uni: Arc::new(tokio::sync::Mutex::new(accept_uni_rx)),
            create_uni: create_uni_tx,
            create_bi: create_bi_tx,
            create_uni_id: Default::default(),
            create_bi_id: Default::default(),
            closed,
        }
    }

    pub async fn accept<T: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        socket: T,
    ) -> Result<Session, Error> {
        // Create callback to handle WebTransport protocol negotiation
        let callback = |req: &server::Request,
                        mut response: server::Response|
         -> Result<server::Response, server::ErrorResponse> {
            // Check for WebTransport subprotocol in Sec-WebSocket-Protocol header
            let protocols = req
                .headers()
                .get(http::header::SEC_WEBSOCKET_PROTOCOL)
                .and_then(|h| h.to_str().ok())
                .unwrap_or_default();

            if !protocols.split(',').any(|p| p.trim() == ALPN) {
                return Err(http::Response::builder()
                    .status(http::StatusCode::BAD_REQUEST)
                    .body(Some("'web-transport' protocol required".to_string()))
                    .unwrap());
            }

            // Add the selected protocol to the response
            response.headers_mut().insert(
                http::header::SEC_WEBSOCKET_PROTOCOL,
                http::HeaderValue::from_str(ALPN).unwrap(),
            );

            Ok(response)
        };

        let ws = tokio_tungstenite::accept_hdr_async_with_config(socket, callback, None).await?;
        Ok(Session::new(ws, false))
    }

    pub async fn connect(url: &str) -> Result<Session, Error> {
        let mut request = url.into_client_request()?;
        request.headers_mut().insert(
            http::header::SEC_WEBSOCKET_PROTOCOL,
            http::HeaderValue::from_str(ALPN).unwrap(),
        );

        let (ws_stream, _) = tokio_tungstenite::connect_async(request).await?;
        Ok(Session::new(ws_stream, true))
    }
}

impl generic::Session for Session {
    type SendStream = SendStream;
    type RecvStream = RecvStream;
    type Error = Error;

    async fn accept_uni(&self) -> Result<Self::RecvStream, Self::Error> {
        self.accept_uni
            .lock()
            .await
            .recv()
            .await
            .ok_or(Error::Closed)
    }

    async fn accept_bi(&self) -> Result<(Self::SendStream, Self::RecvStream), Self::Error> {
        self.accept_bi
            .lock()
            .await
            .recv()
            .await
            .ok_or(Error::Closed)
    }

    async fn open_uni(&self) -> Result<Self::SendStream, Self::Error> {
        let id = self.create_uni_id.fetch_add(1, Ordering::Relaxed);
        let id = StreamId::new(id, Dir::Uni, self.is_server);

        let (tx, rx) = mpsc::unbounded_channel();
        let send_backend = SendState {
            inbound_stopped: tx,
        };
        let send_frontend = SendStream {
            id,
            outbound: self.outbound.clone(),
            outbound_priority: self.outbound_priority.clone(),
            inbound_stopped: rx,
            offset: 0,
            closed: None,
            fin: false,
        };

        self.create_uni
            .send((id, send_backend))
            .await
            .map_err(|_| Error::Closed)?;

        Ok(send_frontend)
    }

    async fn open_bi(&self) -> Result<(Self::SendStream, Self::RecvStream), Self::Error> {
        let id = self.create_bi_id.fetch_add(1, Ordering::Relaxed);
        let id = StreamId::new(id, Dir::Bi, self.is_server);

        let (tx, rx) = mpsc::unbounded_channel();
        let (tx2, rx2) = mpsc::unbounded_channel();

        let send_backend = SendState {
            inbound_stopped: tx,
        };
        let send_frontend = SendStream {
            id,
            outbound: self.outbound.clone(),
            outbound_priority: self.outbound_priority.clone(),
            inbound_stopped: rx,
            offset: 0,
            closed: None,
            fin: false,
        };

        let (tx, rx) = mpsc::unbounded_channel();
        let recv_backend = RecvState {
            inbound_data: tx,
            inbound_reset: tx2,
        };
        let recv_frontend = RecvStream {
            id,
            inbound_data: rx,
            inbound_reset: rx2,
            outbound_priority: self.outbound_priority.clone(),
            buffer: Bytes::new(),
            offset: 0,
            closed: None,
            fin: false,
        };

        self.create_bi
            .send((id, send_backend, recv_backend))
            .await
            .map_err(|_| Error::Closed)?;

        Ok((send_frontend, recv_frontend))
    }

    fn close(&self, code: u32, reason: &str) {
        self.closed
            .send(Some(Error::ConnectionClosed {
                code: VarInt::from(code),
                reason: reason.to_string(),
            }))
            .ok();
    }

    async fn closed(&self) -> Self::Error {
        let mut closed = self.closed.subscribe();
        closed
            .wait_for(|err| err.is_some())
            .await
            .map(|e| e.clone().unwrap_or(Error::Closed))
            .unwrap_or(Error::Closed)
    }

    fn send_datagram(&self, _payload: Bytes) -> Result<(), Self::Error> {
        todo!()
    }

    fn max_datagram_size(&self) -> usize {
        todo!()
    }

    async fn recv_datagram(&self) -> Result<Bytes, Self::Error> {
        todo!()
    }
}

struct SendState {
    inbound_stopped: mpsc::UnboundedSender<StopSending>,
}

pub struct SendStream {
    id: StreamId,

    outbound: mpsc::Sender<Frame>,                   // STREAM
    outbound_priority: mpsc::UnboundedSender<Frame>, // RESET_STREAM
    inbound_stopped: mpsc::UnboundedReceiver<StopSending>,

    offset: u64,
    closed: Option<Error>,
    fin: bool,
}

impl SendStream {
    fn recv_stop(&mut self, code: VarInt) -> Error {
        if let Some(error) = &self.closed {
            return error.clone();
        }

        let frame = ResetStream {
            id: self.id,
            code,
            size: VarInt::try_from(self.offset).unwrap(),
        };

        let error = Error::StreamStop(code);

        self.outbound_priority.send(frame.into()).ok();
        self.closed = Some(error.clone());

        error
    }
}

impl generic::SendStream for SendStream {
    type Error = Error;

    async fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.write_buf(&mut std::io::Cursor::new(buf)).await
    }

    async fn write_buf<B: Buf + Send>(&mut self, buf: &mut B) -> Result<usize, Self::Error> {
        if let Some(error) = &self.closed {
            return Err(error.clone());
        }

        if self.fin {
            return Err(Error::StreamClosed);
        }

        let size = buf.remaining();
        let frame = Stream {
            id: self.id,
            // We're not hitting 2^62
            offset: VarInt::try_from(self.offset).unwrap(),
            data: buf.copy_to_bytes(size),
            fin: false,
        };

        tokio::select! {
            _ = self.outbound.send(frame.into()) => {
                self.offset += size as u64;
                Ok(size)
            }
            Some(stop) = self.inbound_stopped.recv() => {
                Err(self.recv_stop(stop.code))
            }
        }
    }

    fn set_priority(&mut self, _priority: i32) {
        // Priority not implemented in this version
    }

    fn reset(&mut self, code: u32) {
        if self.closed.is_some() {
            return;
        }

        let code = VarInt::from(code);
        let frame = ResetStream {
            id: self.id,
            code,
            size: VarInt::try_from(self.offset).unwrap_or(VarInt::MAX),
        };

        self.outbound_priority.send(frame.into()).ok();
        self.closed = Some(Error::StreamReset(code));
    }

    async fn finish(&mut self) -> Result<(), Self::Error> {
        if let Some(error) = &self.closed {
            return Err(error.clone());
        }

        let frame = Stream {
            id: self.id,
            offset: VarInt::try_from(self.offset).unwrap(),
            data: Bytes::new(),
            fin: true,
        };

        self.outbound
            .send(frame.into())
            .await
            .map_err(|_| Error::Closed)?;
        self.fin = true;

        Ok(())
    }

    async fn closed(&mut self) -> Result<(), Self::Error> {
        if let Some(error) = &self.closed {
            return Err(error.clone());
        }

        // NOTE: will be racey if this is not &mut

        match self.inbound_stopped.recv().await {
            Some(stop) => Err(self.recv_stop(stop.code)),
            None => Err(Error::Closed),
        }
    }
}

struct RecvState {
    inbound_data: mpsc::UnboundedSender<Stream>,
    inbound_reset: mpsc::UnboundedSender<ResetStream>,
}

pub struct RecvStream {
    id: StreamId,

    outbound_priority: mpsc::UnboundedSender<Frame>, // STOP_SENDING
    inbound_data: mpsc::UnboundedReceiver<Stream>,
    inbound_reset: mpsc::UnboundedReceiver<ResetStream>,

    buffer: Bytes,

    offset: u64,
    closed: Option<Error>,
    fin: bool,
}

impl RecvStream {
    fn recv_reset(&mut self, code: VarInt) -> Error {
        if let Some(error) = &self.closed {
            return error.clone();
        }

        self.closed = Some(Error::StreamReset(code));
        Error::StreamReset(code)
    }
}

impl generic::RecvStream for RecvStream {
    type Error = Error;

    async fn read(&mut self) -> Result<Option<Bytes>, Self::Error> {
        loop {
            if let Some(error) = &self.closed {
                return Err(error.clone());
            }

            if self.fin {
                return Ok(None);
            }

            if !self.buffer.is_empty() {
                return Ok(Some(self.buffer.split_to(self.buffer.len())));
            }

            tokio::select! {
                Some(stream) = self.inbound_data.recv() => {
                    assert_eq!(stream.id, self.id);
                    if self.offset != stream.offset.into_inner() {
                        return Err(Error::ProtocolViolation("stream data out of order".into()));
                    }

                    self.offset += stream.data.len() as u64;
                    self.fin = stream.fin;

                    if !stream.data.is_empty() {
                        return Ok(Some(stream.data));
                    }
                }
                Some(reset) = self.inbound_reset.recv() => {
                    return Err(self.recv_reset(reset.code));
                }
                else => return Err(Error::Closed),
            }
        }
    }

    async fn read_buf<B: BufMut + Send>(
        &mut self,
        buf: &mut B,
    ) -> Result<Option<usize>, Self::Error> {
        if !self.buffer.is_empty() {
            let to_read = buf.remaining_mut().min(self.buffer.len());
            buf.put_slice(&self.buffer[..to_read]);
            self.buffer.advance(to_read);
            return Ok(Some(to_read));
        }

        Ok(match self.read().await? {
            Some(mut data) => {
                let to_read = buf.remaining_mut().min(data.len());
                buf.put_slice(&data[..to_read]);
                self.buffer = data.split_to(to_read);
                Some(to_read)
            }
            None => None,
        })
    }

    fn stop(&mut self, code: u32) {
        let code = VarInt::from(code);
        let frame = StopSending { id: self.id, code };

        self.outbound_priority.send(frame.into()).ok();
        self.closed = Some(Error::StreamStop(code));
    }

    async fn closed(&mut self) -> Result<(), Self::Error> {
        if let Some(error) = &self.closed {
            return Err(error.clone());
        }

        loop {
            if self.fin {
                return Ok(());
            }

            if !self.buffer.is_empty() {
                // We have buffered data, so there's no point waiting for more.
                return Err(match self.inbound_reset.recv().await {
                    Some(reset) => self.recv_reset(reset.code),
                    None => Error::Closed,
                });
            }

            tokio::select! {
                Some(reset) = self.inbound_reset.recv() => {
                    return Err(self.recv_reset(reset.code));
                }
                Some(stream) = self.inbound_data.recv() => {
                    assert_eq!(stream.id, self.id);
                    self.buffer = stream.data;
                    self.fin = stream.fin;
                }
                else => {
                    return Err(Error::Closed);
                }
            }
        }
    }
}

/// Stream direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Bi,
    Uni,
}

/// Stream ID with direction encoding (QUIC-style)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StreamId(VarInt);

impl StreamId {
    fn new(id: u64, dir: Dir, is_server: bool) -> Self {
        let mut stream_id = id << 2;
        if dir == Dir::Uni {
            stream_id |= 0x02;
        }
        if is_server {
            stream_id |= 0x01;
        }
        StreamId(VarInt::try_from(stream_id).expect("stream ID too large"))
    }

    fn dir(&self) -> Dir {
        if self.0.into_inner() & 0x02 != 0 {
            Dir::Uni
        } else {
            Dir::Bi
        }
    }

    fn server_initiated(&self) -> bool {
        self.0.into_inner() & 0x01 != 0
    }

    pub fn can_recv(&self, is_server: bool) -> bool {
        match self.dir() {
            Dir::Uni => self.server_initiated() != is_server,
            Dir::Bi => true,
        }
    }

    pub fn can_send(&self, is_server: bool) -> bool {
        match self.dir() {
            Dir::Uni => self.server_initiated() == is_server,
            Dir::Bi => true,
        }
    }
}

const STREAM_TYS: RangeInclusive<u64> = 0x08..=0x0f;

/// QUIC Frame types (subset for WebSocket transport)
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum FrameType {
    Padding = 0x00,
    Ping = 0x01,
    ResetStream = 0x04,
    StopSending = 0x05,
    Stream = 0x08, // Base type, actual value depends on flags
    ApplicationClose = 0x1d,
}

#[derive(Debug, Clone)]
struct Stream {
    id: StreamId,
    offset: VarInt,
    data: Bytes,
    fin: bool,
}

impl Stream {
    fn encode(&self, mut buf: &mut BytesMut) {
        // Calculate frame type based on flags
        let mut frame_type = FrameType::Stream as u8;
        if self.fin {
            frame_type |= 0x01;
        }
        if self.offset.into_inner() != 0 {
            frame_type |= 0x04;
        }
        // Always set length bit
        frame_type |= 0x02;

        buf.put_u8(frame_type);
        self.id.0.encode(&mut buf);

        if self.offset.into_inner() != 0 {
            self.offset.encode(&mut buf);
        }

        // Always encode length
        let len = VarInt::try_from(self.data.len()).expect("data too large");
        len.encode(&mut buf);
        buf.put_slice(&self.data);
    }

    fn decode(ty: u64, mut data: Bytes) -> Result<Self, Error> {
        let id = StreamId(VarInt::decode(&mut data)?);

        let offset = if ty & 0x04 != 0 {
            VarInt::decode(&mut data)?
        } else {
            VarInt::default()
        };

        let length = if ty & 0x02 != 0 {
            VarInt::decode(&mut data)?.into_inner() as usize
        } else {
            data.len()
        };

        if data.len() < length {
            return Err(Error::Short);
        }

        let stream_data = data.split_to(length);
        let fin = ty & 0x01 != 0;

        Ok(Stream {
            id,
            offset,
            data: stream_data,
            fin,
        })
    }
}

#[derive(Debug, Clone)]
struct ResetStream {
    id: StreamId,
    code: VarInt,
    size: VarInt,
}

impl ResetStream {
    fn encode(&self, mut buf: &mut BytesMut) {
        buf.put_u8(FrameType::ResetStream as u8);
        self.id.0.encode(&mut buf);
        self.code.encode(&mut buf);
        self.size.encode(&mut buf);
    }

    fn decode(mut data: Bytes) -> Result<Self, Error> {
        let id = StreamId(VarInt::decode(&mut data)?);
        let code = VarInt::decode(&mut data)?;
        let size = VarInt::decode(&mut data)?;
        Ok(ResetStream { id, code, size })
    }
}

#[derive(Debug, Clone)]
struct StopSending {
    id: StreamId,
    code: VarInt,
}

impl StopSending {
    fn encode(&self, mut buf: &mut BytesMut) {
        buf.put_u8(FrameType::StopSending as u8);
        self.id.0.encode(&mut buf);
        self.code.encode(&mut buf);
    }

    fn decode(mut data: Bytes) -> Result<Self, Error> {
        let id = StreamId(VarInt::decode(&mut data)?);
        let code = VarInt::decode(&mut data)?;
        Ok(StopSending { id, code })
    }
}

#[derive(Debug, Clone)]
struct ConnectionClose {
    code: VarInt,
    reason: String,
}

impl ConnectionClose {
    fn encode(&self, mut buf: &mut BytesMut) {
        buf.put_u8(FrameType::ApplicationClose as u8);
        self.code.encode(&mut buf);
        buf.put_slice(self.reason.as_bytes());
    }

    fn decode(mut data: Bytes) -> Result<Self, Error> {
        let code = VarInt::decode(&mut data)?;
        let reason = String::from_utf8_lossy(&data).into_owned();
        Ok(ConnectionClose { code, reason })
    }
}

/// QUIC-compatible frames for WebSocket transport
#[derive(Debug)]
enum Frame {
    Padding,
    Ping,
    ResetStream(ResetStream),
    StopSending(StopSending),
    ConnectionClose(ConnectionClose),
    Stream(Stream),
}

impl Frame {
    fn encode(&self) -> Bytes {
        let mut buf = BytesMut::new();

        match self {
            Frame::Padding => buf.put_u8(FrameType::Padding as u8),
            Frame::Ping => buf.put_u8(FrameType::Ping as u8),
            Frame::ResetStream(frame) => frame.encode(&mut buf),
            Frame::StopSending(frame) => frame.encode(&mut buf),
            Frame::Stream(frame) => frame.encode(&mut buf),
            Frame::ConnectionClose(frame) => frame.encode(&mut buf),
        }

        buf.freeze()
    }

    fn decode(mut data: Bytes) -> Result<Self, Error> {
        let frame_type = VarInt::decode(&mut data)?;

        match frame_type.into_inner() {
            0x00 => Ok(Frame::Padding),
            0x01 => Ok(Frame::Ping),
            0x04 => Ok(Frame::ResetStream(ResetStream::decode(data)?)),
            0x05 => Ok(Frame::StopSending(StopSending::decode(data)?)),
            ty if STREAM_TYS.contains(&ty) => Ok(Frame::Stream(Stream::decode(ty, data)?)),
            0x1d => Ok(Frame::ConnectionClose(ConnectionClose::decode(data)?)),
            _ => Err(Error::InvalidFrameType(frame_type)),
        }
    }
}

impl From<Stream> for Frame {
    fn from(stream: Stream) -> Self {
        Frame::Stream(stream)
    }
}

impl From<ResetStream> for Frame {
    fn from(reset: ResetStream) -> Self {
        Frame::ResetStream(reset)
    }
}

impl From<StopSending> for Frame {
    fn from(stop: StopSending) -> Self {
        Frame::StopSending(stop)
    }
}

impl From<ConnectionClose> for Frame {
    fn from(close: ConnectionClose) -> Self {
        Frame::ConnectionClose(close)
    }
}
