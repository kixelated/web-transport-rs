use std::collections::HashMap;
use std::fmt;

use anyhow::Context;
use rand::Rng;
use tokio::task::JoinSet;

use webtransport_quinn::{RecvStream, Request, SendStream, Session};

#[allow(dead_code)] // used by server only
pub fn parse(request: &Request) -> anyhow::Result<(u8, u16)> {
    if request.uri().path() != "/webtransport/devious-baton" {
        anyhow::bail!("invalid path: {}", request.uri().path());
    }

    let mut query = HashMap::new();

    // Get the query string after the path.
    if let Some(str) = request.uri().query() {
        // Split the query string into key-value pairs
        for part in str.split('&') {
            let mut split = part.splitn(2, '=');
            let key = split.next().context("no key")?;
            let value = split.next().context("no value")?;
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
pub async fn run(
    session: Session,
    init: Option<u8>, // None if we're the client
    mut count: u16,   // the number of batons
) -> anyhow::Result<()> {
    // Writing the baton to a stream
    let mut outbound = JoinSet::<anyhow::Result<(u8, Outbound)>>::new();

    // Reading the baton from a stream
    let mut inbound = JoinSet::<anyhow::Result<(u8, Inbound)>>::new();

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

async fn recv_baton(mut recv: RecvStream) -> anyhow::Result<u8> {
    let buf = recv.read_to_end(u16::MAX as usize).await?; // arbitrary max padding.

    // TODO also check that padding varint is correct.
    if buf.len() < 2 {
        anyhow::bail!("baton message too small");
    }

    let baton = buf[buf.len() - 1];
    Ok(baton)
}

async fn send_baton(mut send: SendStream, baton: u8) -> anyhow::Result<()> {
    // TODO support padding
    send.write_all(&[0, baton]).await?;
    send.finish().await?;

    Ok(())
}

enum Inbound {
    Uni,
    LocalBi, // we already wrote the baton
    RemoteBi(SendStream),
}

impl fmt::Debug for Inbound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Inbound::Uni => write!(f, "Uni"),
            Inbound::LocalBi => write!(f, "LocalBi"),
            Inbound::RemoteBi(_) => write!(f, "RemoteBi"),
        }
    }
}

enum Outbound {
    Uni,
    LocalBi(RecvStream),
    RemoteBi, // we already read the baton
}

impl fmt::Debug for Outbound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Outbound::Uni => write!(f, "Uni"),
            Outbound::LocalBi(_) => write!(f, "LocalBi"),
            Outbound::RemoteBi => write!(f, "RemoteBi"),
        }
    }
}
