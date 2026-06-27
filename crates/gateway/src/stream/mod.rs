//! SSE streaming helpers — parse an inbound provider SSE stream and
//! re-emit as canonical ChatStreamChunk items.

use crate::{
    providers::registry::WireFormat,
    translation::decode_stream_chunk,
    types::ChatStreamChunk,
};
use bytes::Bytes;
use futures_util::StreamExt;
use tokio::sync::mpsc;
use tracing::warn;

/// Spawn a task that reads `response_stream`, decodes each SSE data line,
/// and forwards canonical `ChatStreamChunk` values through the returned channel.
///
/// The channel is closed when the stream ends or errors.
pub fn pipe_sse_stream(
    response_stream: impl futures_util::Stream<Item = std::result::Result<Bytes, reqwest::Error>>
        + Send
        + 'static,
    format: WireFormat,
) -> mpsc::Receiver<std::result::Result<ChatStreamChunk, String>> {
    let (tx, rx) = mpsc::channel(128);

    tokio::spawn(async move {
        let mut stream = Box::pin(response_stream);
        let mut buf = String::new();

        while let Some(chunk_result) = stream.next().await {
            let bytes = match chunk_result {
                Ok(b) => b,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string())).await;
                    return;
                }
            };

            let text = match std::str::from_utf8(&bytes) {
                Ok(t) => t,
                Err(_) => {
                    warn!("Non-UTF8 SSE chunk, skipping");
                    continue;
                }
            };

            buf.push_str(text);

            // SSE messages are separated by double newlines.
            while let Some(pos) = buf.find("\n\n") {
                let message = buf[..pos].to_string();
                buf = buf[pos + 2..].to_string();

                for line in message.lines() {
                    if let Some(data) = line.strip_prefix("data: ") {
                        if data.trim() == "[DONE]" {
                            return;
                        }
                        match decode_stream_chunk(data, &format) {
                            Ok(Some(chunk)) => {
                                if tx.send(Ok(chunk)).await.is_err() {
                                    return; // receiver dropped
                                }
                            }
                            Ok(None) => {} // skip non-content events
                            Err(e) => {
                                warn!("SSE decode error: {e}");
                            }
                        }
                    }
                }
            }
        }
    });

    rx
}
