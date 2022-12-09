// identity_op: we write out how test values are computed
#![allow(clippy::identity_op)]

use std::{borrow::BorrowMut, time::Duration};

use assert_matches::assert_matches;
use bytes::{Buf, Bytes, BytesMut};
use futures_util::{future, StreamExt};
use http::{Request, Response, StatusCode};

use crate::{
    client::{self, SendRequest},
    connection::ConnectionState,
    error::{Code, Error, Kind},
    proto::{
        coding::Encode as _,
        frame::{Frame, Settings},
        stream::{StreamId, StreamType},
    },
    quic::{self, SendStream},
    server,
};

use super::h3_quinn;
use super::{init_tracing, Pair};

#[tokio::test]
async fn graceful_shutdown_grace_interval() {
    init_tracing();
    let mut pair = Pair::default();
    let mut server = pair.server();

    let client_fut = async {};

    let server_fut = async {
        let conn = server.next().await;
        let mut incoming = server::Connection::new(conn).await.unwrap();
        let (_, first) = incoming.accept().await.unwrap().unwrap();
        incoming.shutdown(1).await.unwrap();
        let (_, in_flight) = incoming.accept().await.unwrap().unwrap();
        response(first).await;
        response(in_flight).await;

        while let Ok(Some((_, stream))) = incoming.accept().await {
            response(stream).await;
        }
        // Ensure `too_late` request is executed as the connection is still
        // closing (no QUIC `Close` frame has been fired yet)
        tokio::time::sleep(Duration::from_millis(50)).await;
    };

    tokio::join!(server_fut, client_fut);
}

async fn request<T, O, B>(mut send_request: T) -> Result<Response<()>, Error>
where
    T: BorrowMut<SendRequest<O, B>>,
    O: quic::OpenStreams<B>,
    B: Buf,
{
    let mut request_stream = send_request
        .borrow_mut()
        .send_request(Request::get("http://no.way").body(()).unwrap())
        .await?;
    request_stream.recv_response().await
}

async fn response<S, B>(mut stream: server::RequestStream<S, B>)
where
    S: quic::RecvStream + SendStream<B>,
    B: Buf,
{
    stream
        .send_response(
            Response::builder()
                .status(StatusCode::IM_A_TEAPOT)
                .body(())
                .unwrap(),
        )
        .await
        .unwrap();
    stream.finish().await.unwrap();
}
