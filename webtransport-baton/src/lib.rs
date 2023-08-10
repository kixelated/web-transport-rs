// Implements https://datatracker.ietf.org/doc/html/draft-frindell-webtrans-devious-baton

use std::{collections::HashMap, fmt};

use anyhow::Context;
use rand::Rng;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::task::JoinSet;

use webtransport_generic::{RecvStream, SendStream, Session};

pub fn parse(uri: &http::Uri) -> anyhow::Result<(u8, u16)> {
    if uri.path() != "/webtransport/devious-baton" {
        anyhow::bail!("invalid path: {}", uri.path());
    }

    let mut query = HashMap::new();

    // Get the query string after the path.
    if let Some(str) = uri.query() {
        // Split the query string into key-value pairs
        for part in str.split('&') {
            let (key, value) = part.split_once('=').context("failed to split")?;
            query.insert(key, value);
        }
    }

    let version = match query.get("version") {
        Some(version) => version.parse::<u8>().context("failed to parse version")?,
        None => 0,
    };

    if version != 0 {
        anyhow::bail!("invalid baton version: {}", version);
    }

    let baton_range = 1..=255;

    let value = match query.get("baton") {
        Some(baton) => match baton.parse::<u8>().context("failed to parse baton")? {
            baton if baton_range.contains(&baton) => baton,
            baton => anyhow::bail!("invalid baton: {}", baton),
        },
        None => rand::thread_rng().gen_range(baton_range),
    };

    let count = match query.get("count") {
        Some(count) => count.parse::<u16>().context("failed to parse count: {}")?,
        None => 1,
    };

    Ok((value, count))
}

// Sends and receives batons until they all reach 0.
pub async fn run<S>(
    session: S,
    init: Option<u8>, // None if we're the client
    mut count: u16,   // the number of batons
) -> anyhow::Result<()>
where
    S: Session,
{
    // Writing the baton to a stream
    let mut outbound = JoinSet::<anyhow::Result<(u8, Outbound<S::RecvStream>)>>::new();

    // Reading the baton from a stream
    let mut inbound = JoinSet::<anyhow::Result<(u8, Inbound<S::SendStream>)>>::new();

    // If we're the server, queue up the initial batons to send.
    if let Some(init) = init {
        for _ in 0..count {
            let session = session.clone();

            outbound.spawn(async move {
                let send = session.open_uni().await?;
                send_baton(send, init).await?;
                Ok((init, Outbound::Uni))
            });
        }
    }

    while count > 0 || !outbound.is_empty() || inbound.is_empty() {
        tokio::select! {
            // Resolves when we sent a baton.
            res = outbound.join_next(), if !outbound.is_empty() => {
                let (baton, source) = res.unwrap()??;
                log::info!("sent baton: value={} type={:?}", baton, source);

                if baton == 0 {
                    // We don't expect a response.
                    continue
                }

                if let Outbound::LocalBi(recv) = source {
                    inbound.spawn(async move {
                        let baton = recv_baton(recv).await?;
                        Ok((baton, Inbound::LocalBi))
                    });
                }
            }

            // Resolves when we received a baton.
            res = inbound.join_next(), if !inbound.is_empty() => {
                let (baton, source) = res.unwrap()??;
                log::info!("received baton: value={} type={:?}", baton, source);

                if baton == 0 {
                    // Nothing more to do.
                    count -= 1;
                    continue
                }

                let (baton, _) = baton.overflowing_add(1); // will overflow to 0
                let session = session.clone();

                match source {
                    Inbound::Uni => {
                        // If the incoming Baton message arrived on a unidirectional stream,
                        //   the endpoint opens a bidirectional stream and sends the outgoing Baton message on it.
                        outbound.spawn(async move {
                            let (send, recv) = session.open_bi().await?;
                            send_baton(send, baton).await?;

                            Ok((baton, Outbound::LocalBi(recv)))
                        });
                    },
                    Inbound::LocalBi => {
                        // If the Baton message arrived on a self-initiated bidirectional stream,
                        //   the endpoint opens a unidirectional stream and sends the outgoing Baton message on it.
                        outbound.spawn(async move {
                            let send = session.open_uni().await?;
                            send_baton(send, baton).await?;

                            Ok((baton, Outbound::Uni))
                        });
                    },
                    Inbound::RemoteBi(send) => {
                        // If the Baton message arrived on a peer-initiated bidirectional stream,
                        //   the endpoint sends the outgoing Baton message on that stream.
                        outbound.spawn(async move {
                            send_baton(send, baton).await?;

                            Ok((baton, Outbound::RemoteBi))
                        });
                    },
                }
            }

            // Resolves when we receive a new unidirectional stream.
            res = session.accept_uni() => {
                let recv = res?;
                inbound.spawn(async move {
                    let baton = recv_baton(recv).await?;
                    Ok((baton, Inbound::Uni))
                });
            }

            // Resolves when we receive a new bidirectional stream.
            res = session.accept_bi() => {
                let (send, recv) = res?;

                inbound.spawn(async move {
                    let baton = recv_baton(recv).await?;
                    Ok((baton, Inbound::RemoteBi(send)))
                });
            }
            err = session.closed() => {
                return Err(err.into())
            }
        };
    }

    Ok(())
}

async fn recv_baton<R: RecvStream>(mut stream: R) -> anyhow::Result<u8> {
    // Read the entire stream into the buffer
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;

    // TODO also check that padding varint is correct.
    if buf.len() < 2 {
        anyhow::bail!("baton message too small: {}", buf.len());
    }

    let baton = buf[buf.len() - 1];
    Ok(baton)
}

async fn send_baton<S: SendStream>(mut stream: S, baton: u8) -> anyhow::Result<()> {
    let buf = [0, baton];
    stream.write_all(&buf).await?;

    Ok(())
}

enum Inbound<S: SendStream> {
    Uni,
    LocalBi, // we already wrote the baton
    RemoteBi(S),
}

impl<S: SendStream> fmt::Debug for Inbound<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Inbound::Uni => write!(f, "Uni"),
            Inbound::LocalBi => write!(f, "LocalBi"),
            Inbound::RemoteBi(_) => write!(f, "RemoteBi"),
        }
    }
}

enum Outbound<R: RecvStream> {
    Uni,
    LocalBi(R),
    RemoteBi, // we already read the baton
}

impl<R: RecvStream> fmt::Debug for Outbound<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outbound::Uni => write!(f, "Uni"),
            Outbound::LocalBi(_) => write!(f, "LocalBi"),
            Outbound::RemoteBi => write!(f, "RemoteBi"),
        }
    }
}
