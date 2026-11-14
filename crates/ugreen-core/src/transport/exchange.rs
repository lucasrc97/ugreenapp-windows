//! One request/response exchange over an already connected byte channel.
//!
//! This mirrors, step for step and with the same timings, what
//! `linux.rs` does on its RFCOMM socket: write the inner frame directly and
//! wait for a framed response; if that fails, retry the same frame wrapped in
//! RCSP on the same connection. It is written against a small trait so the
//! logic can be unit-tested without Bluetooth.
//!
//! `linux.rs` deliberately keeps its own copy for now: it is the code that was
//! verified on real hardware and it cannot be re-tested from WSL.

use anyhow::{Context, Result, anyhow};
use std::time::Duration;

use super::TransportError;
use crate::protocol::{
    DecodedFrame, ProtocolCommand, ProtocolFacade, ProtocolType, find_complete_rcsp_frame,
    find_complete_ug1_frame, unwrap_rcsp_custom, wrap_rcsp_custom,
};

const READ_ATTEMPTS: usize = 8;
const DIRECT_READ_WAIT: Duration = Duration::from_millis(1200);
const RCSP_READ_WAIT: Duration = Duration::from_millis(1500);

pub(crate) enum ReadOutcome {
    /// This many bytes were written to the start of the buffer.
    Data(usize),
    /// Nothing arrived within the timeout.
    TimedOut,
    /// The peer closed the connection.
    Closed,
}

pub(crate) trait Channel {
    fn write_all(&mut self, bytes: &[u8]) -> Result<()>;
    fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> Result<ReadOutcome>;
}

pub(crate) fn exchange(
    channel: &mut dyn Channel,
    facade: &dyn ProtocolFacade,
    protocol: ProtocolType,
    command: &ProtocolCommand,
    inner: &[u8],
    seq: u8,
) -> Result<DecodedFrame> {
    let direct_error = match send_direct(channel, facade, protocol, command, inner) {
        Ok(frame) => return Ok(frame),
        Err(error) => error,
    };
    log::debug!("direct exchange failed ({direct_error:#}); retrying wrapped in RCSP");
    let rcsp_error = match send_rcsp(channel, facade, protocol, inner, seq) {
        Ok(frame) => return Ok(frame),
        Err(error) => error,
    };

    let detail = format!("direct failed: {direct_error:#}; rcsp failed: {rcsp_error:#}");
    if is_timeout(&direct_error) && is_timeout(&rcsp_error) {
        Err(TransportError::Timeout { detail }.into())
    } else {
        Err(anyhow!(detail))
    }
}

fn is_timeout(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<TransportError>(),
        Some(TransportError::Timeout { .. })
    )
}

fn send_direct(
    channel: &mut dyn Channel,
    facade: &dyn ProtocolFacade,
    protocol: ProtocolType,
    command: &ProtocolCommand,
    inner: &[u8],
) -> Result<DecodedFrame> {
    log::debug!("TX direct {}", hex(inner));
    channel
        .write_all(inner)
        .context("failed to write direct protocol packet")?;

    if protocol != ProtocolType::Ug1
        && matches!(
            command,
            ProtocolCommand::SetAnc { .. }
                | ProtocolCommand::SetEq { .. }
                | ProtocolCommand::SetPromptLanguage { .. }
                | ProtocolCommand::SetPromptVolume { .. }
                | ProtocolCommand::FactoryReset
        )
    {
        return Ok(DecodedFrame {
            success: true,
            command: command.name().to_string(),
            raw: inner.to_vec(),
            data: serde_json::json!({ "status": "sent" }),
        });
    }

    let raw_response = read_frame(
        channel,
        DIRECT_READ_WAIT,
        "direct",
        |buffer| match protocol {
            ProtocolType::Ug1 => find_complete_ug1_frame(buffer),
            _ => {
                let total = 5 + *buffer.get(4)? as usize;
                (buffer.len() >= total).then(|| buffer[..total].to_vec())
            }
        },
    )?;
    let mut decoded = facade
        .decode_inner(protocol, &raw_response)
        .context("failed to decode direct device response")?;
    decoded.raw = raw_response;
    Ok(decoded)
}

fn send_rcsp(
    channel: &mut dyn Channel,
    facade: &dyn ProtocolFacade,
    protocol: ProtocolType,
    inner: &[u8],
    seq: u8,
) -> Result<DecodedFrame> {
    let rcsp = wrap_rcsp_custom(inner, seq);
    log::debug!("TX rcsp {}", hex(&rcsp));
    channel
        .write_all(&rcsp)
        .context("failed to write RFCOMM RCSP packet")?;

    let raw_response = read_frame(channel, RCSP_READ_WAIT, "RCSP", find_complete_rcsp_frame)?;
    let inner_response = unwrap_rcsp_custom(&raw_response)?;
    let mut decoded = facade
        .decode_inner(protocol, &inner_response)
        .context("failed to decode RCSP device response")?;
    decoded.raw = raw_response;
    Ok(decoded)
}

/// Reads until `find_frame` recognises a complete frame in what was received,
/// giving up after [`READ_ATTEMPTS`] waits of `wait` each.
fn read_frame(
    channel: &mut dyn Channel,
    wait: Duration,
    kind: &str,
    find_frame: impl Fn(&[u8]) -> Option<Vec<u8>>,
) -> Result<Vec<u8>> {
    let mut buffer = Vec::with_capacity(512);
    let mut chunk = [0u8; 256];

    for _ in 0..READ_ATTEMPTS {
        let read = match channel
            .read(&mut chunk, wait)
            .with_context(|| format!("failed to read {kind} protocol data"))?
        {
            ReadOutcome::TimedOut => continue,
            ReadOutcome::Closed => break,
            ReadOutcome::Data(read) => read,
        };
        log::debug!("RX {}", hex(&chunk[..read]));
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(frame) = find_frame(&buffer) {
            return Ok(frame);
        }
    }

    Err(TransportError::Timeout {
        detail: format!(
            "device did not send a complete {kind} response; partial={}",
            hex(&buffer)
        ),
    }
    .into())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::UgProtocolFacade;
    use std::collections::VecDeque;

    const QUERY_FW: [u8; 8] = [0xAA, 0xBB, 0xCC, 0x01, 0x01, 0x00, 0x21, 0x90];
    // Synthetic response frame (see golden_tests.rs).
    const FW_RESPONSE: [u8; 14] = [
        0xDD, 0xEE, 0xFF, 0x01, 0x01, 0x06, 0x01, 0x00, 0x07, 0x00, 0x00, 0x00, 0x2A, 0x01,
    ];

    enum Step {
        Data(&'static [u8]),
        TimedOut,
        Closed,
    }

    #[derive(Default)]
    struct FakeChannel {
        written: Vec<Vec<u8>>,
        script: VecDeque<Step>,
        waits: Vec<Duration>,
        fail_writes: bool,
    }

    impl FakeChannel {
        fn with(script: impl IntoIterator<Item = Step>) -> Self {
            Self {
                script: script.into_iter().collect(),
                ..Self::default()
            }
        }
    }

    impl Channel for FakeChannel {
        fn write_all(&mut self, bytes: &[u8]) -> Result<()> {
            if self.fail_writes {
                return Err(anyhow!("write refused"));
            }
            self.written.push(bytes.to_vec());
            Ok(())
        }

        fn read(&mut self, buffer: &mut [u8], timeout: Duration) -> Result<ReadOutcome> {
            self.waits.push(timeout);
            Ok(match self.script.pop_front() {
                Some(Step::Data(bytes)) => {
                    buffer[..bytes.len()].copy_from_slice(bytes);
                    ReadOutcome::Data(bytes.len())
                }
                Some(Step::Closed) => ReadOutcome::Closed,
                // An exhausted script behaves like a silent device.
                Some(Step::TimedOut) | None => ReadOutcome::TimedOut,
            })
        }
    }

    fn run(
        channel: &mut FakeChannel,
        protocol: ProtocolType,
        command: ProtocolCommand,
    ) -> Result<DecodedFrame> {
        let facade = UgProtocolFacade::new();
        let payload = command.payload(protocol);
        let inner = facade.encode_inner(protocol, &command, &payload, 3)?;
        exchange(channel, &facade, protocol, &command, &inner, 3)
    }

    #[test]
    fn ug1_direct_round_trip() {
        let mut channel = FakeChannel::with([Step::Data(&FW_RESPONSE)]);
        let frame = run(
            &mut channel,
            ProtocolType::Ug1,
            ProtocolCommand::QueryFirmware,
        )
        .unwrap();
        assert_eq!(channel.written, [QUERY_FW.to_vec()]);
        assert_eq!(channel.waits, [DIRECT_READ_WAIT]);
        assert!(frame.success);
        assert_eq!(frame.command, "GET_VERSION");
        assert_eq!(frame.raw, FW_RESPONSE);
        assert_eq!(frame.data["firmwareVersion"], "1.0.7");
    }

    #[test]
    fn response_split_across_reads_and_timeouts_is_reassembled() {
        let mut channel = FakeChannel::with([
            Step::TimedOut,
            Step::Data(&FW_RESPONSE[..5]),
            Step::TimedOut,
            Step::Data(&FW_RESPONSE[5..]),
        ]);
        let frame = run(
            &mut channel,
            ProtocolType::Ug1,
            ProtocolCommand::QueryFirmware,
        )
        .unwrap();
        assert_eq!(frame.raw, FW_RESPONSE);
        assert_eq!(channel.written.len(), 1, "no RCSP retry expected");
    }

    #[test]
    fn silent_device_falls_back_to_rcsp_then_reports_timeout() {
        let mut channel = FakeChannel::default();
        let error = run(
            &mut channel,
            ProtocolType::Ug1,
            ProtocolCommand::QueryFirmware,
        )
        .err()
        .expect("must fail");

        assert_eq!(
            channel.written,
            [QUERY_FW.to_vec(), wrap_rcsp_custom(&QUERY_FW, 3)]
        );
        let mut expected_waits = vec![DIRECT_READ_WAIT; READ_ATTEMPTS];
        expected_waits.extend([RCSP_READ_WAIT; READ_ATTEMPTS]);
        assert_eq!(channel.waits, expected_waits);

        assert!(is_timeout(&error), "{error:#}");
        let message = error.to_string();
        assert!(message.contains("direct failed"), "{message}");
        assert!(message.contains("rcsp failed"), "{message}");
    }

    #[test]
    fn closed_connection_stops_reading_and_keeps_partial_bytes_in_error() {
        let mut channel = FakeChannel::with([Step::Data(&FW_RESPONSE[..4]), Step::Closed]);
        let error = run(
            &mut channel,
            ProtocolType::Ug1,
            ProtocolCommand::QueryFirmware,
        )
        .err()
        .expect("must fail");
        assert!(error.to_string().contains("partial=DDEEFF01"), "{error:#}");
    }

    #[test]
    fn rcsp_fallback_can_succeed() {
        // UG2 direct read gives up after a close; the RCSP reply then decodes.
        const RCSP_REPLY: [u8; 16] = [
            0xFE, 0xDC, 0xBA, 0x00, 0xFF, 0x00, 0x08, 0x00, 0x03, 0xFF, 0x27, 0x01, 0x00, 0x01,
            0x00, 0xEF,
        ];
        let mut channel = FakeChannel::with([Step::Closed, Step::Data(&RCSP_REPLY)]);
        let frame = run(
            &mut channel,
            ProtocolType::Ug2,
            ProtocolCommand::QueryFirmware,
        )
        .unwrap();
        assert_eq!(channel.written.len(), 2);
        assert_eq!(frame.command, "GET_INFO");
        assert_eq!(frame.raw, RCSP_REPLY);
    }

    #[test]
    fn non_ug1_setters_return_without_reading() {
        let mut channel = FakeChannel::default();
        let command = ProtocolCommand::SetEq {
            mode: "bass".into(),
        };
        let frame = run(&mut channel, ProtocolType::Ug2, command).unwrap();
        assert!(frame.success);
        assert_eq!(frame.data["status"], "sent");
        assert!(channel.waits.is_empty());
        assert_eq!(channel.written, [vec![0x20, 0x01, 0x00, 0x01, 0x06]]);
    }

    #[test]
    fn write_failure_is_not_reported_as_timeout() {
        let mut channel = FakeChannel {
            fail_writes: true,
            ..FakeChannel::default()
        };
        let error = run(
            &mut channel,
            ProtocolType::Ug1,
            ProtocolCommand::QueryFirmware,
        )
        .err()
        .expect("must fail");
        assert!(!is_timeout(&error));
        assert!(error.to_string().contains("write refused"), "{error:#}");
    }
}
