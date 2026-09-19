//! Peppol at both ends on this machine: one participant's access point
//! posting to itself over as4, unsigned (ADR-0051). The far end is this
//! participant's inbox bound at an ephemeral port, receipting the one
//! Standard Business Document and unwrapping it; the near end is this
//! participant again, wrapping and posting. Signing needs a certificate
//! and a loopback has none, so the far end is an [`Unsigned`] twin
//! whatever the near end was given.
//!
//! What the protocol carries is one XML business document, so the refusals
//! are about content: bytes that are no XML element are not carried, and a
//! document with an XML declaration or whitespace around it is carried
//! without them, which is not as it is.

use std::net::TcpListener;
use std::sync::{Arc, OnceLock};

use as4::Unsigned;
use transport::error::{Result, protocol_error};
use transport::loopback::{FarEnd, LOOPBACK_TIMEOUT, Loopback};
use transport::{Arrived, Transport};

use crate::PeppolTransport;
use crate::participant::Participant;
use crate::sbdh::business_document;

/// The participant both ends of a loopback are: a GLN under ICD 0088.
pub const PARTICIPANT: &str = "0088:7300010000001";

impl PeppolTransport {
    /// Both ends on this machine: an ephemeral local port, the loopback
    /// timeout, and one participant — [`PARTICIPANT`] — sending to itself
    /// under the billing process until [`Self::carrying`].
    #[must_use]
    pub fn loopback() -> Self {
        let me = Participant {
            value: PARTICIPANT.to_string(),
        };
        Self::new("as4://127.0.0.1:0/as4", me.clone(), me).timing_out_after(LOOPBACK_TIMEOUT)
    }

    /// This participant at `endpoint`, unsigned and with nothing seen: what
    /// each end of a loopback is.
    fn twin(&self, endpoint: String) -> Self {
        Self {
            endpoint,
            me: self.me.clone(),
            partner: self.partner.clone(),
            document: self.document.clone(),
            process: self.process.clone(),
            directory: self.directory.clone(),
            signer: Arc::new(Unsigned),
            timeout: self.timeout,
            inbox: OnceLock::new(),
        }
    }
}

/// A bound access point waiting for its one Standard Business Document,
/// which it receipts and unwraps.
struct Listening {
    transport: PeppolTransport,
    listener: TcpListener,
    address: String,
}

impl FarEnd for Listening {
    fn address(&self) -> &str {
        &self.address
    }

    fn take_one(self: Box<Self>) -> Result<Arrived> {
        self.transport
            .accept_one(&self.listener)?
            .ok_or_else(|| protocol_error("a document seen before: receipted, not delivered"))
    }
}

impl Loopback for PeppolTransport {
    fn refuses(&self, payload: &[u8]) -> Option<String> {
        match business_document(payload) {
            Err(error) => Some(error.message),
            Ok(document) if document.len() != payload.len() => Some(String::from(
                "a business document travels inside the Standard Business Document as an \
                 element: its XML declaration and the whitespace around it are set aside",
            )),
            Ok(_) => None,
        }
    }

    fn far_end(&self) -> Result<Box<dyn FarEnd>> {
        let transport = self.twin(self.endpoint.clone());
        let (listener, address) = transport.bind()?;
        Ok(Box::new(Listening {
            transport,
            listener,
            address,
        }))
    }

    fn send_to(&self, address: &str, payload: &[u8]) -> Result<()> {
        self.twin(format!("as4://{address}{}", path_of(&self.endpoint)))
            .send("", payload)
    }
}

/// The path of `endpoint`, `/` where it has none.
fn path_of(endpoint: &str) -> &str {
    let rest = endpoint
        .split_once("://")
        .map_or(endpoint, |(_, rest)| rest);
    rest.find('/').map_or("/", |at| &rest[at..])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loopback_carries_a_business_document_whole_and_refuses_what_is_none() {
        let pair = PeppolTransport::loopback();
        let probe = b"<probe><n>1</n>round-trip</probe>";
        assert!(pair.refuses(probe).is_none());
        assert_eq!(pair.round(probe).expect("round").bytes, probe);
        for (name, bytes) in transport::payload::edge_payloads() {
            let why = pair
                .refuses(&bytes)
                .unwrap_or_else(|| panic!("{name} not refused"));
            assert!(why.contains("one XML business document"), "{name}: {why}");
            let error = pair.round(&bytes).expect_err(name);
            assert!(error.message.starts_with("send failed:"), "{name}: {error}");
        }
        let declared = b"<?xml version=\"1.0\"?>\n<probe/>\n";
        let why = pair.refuses(declared).expect("not as it is");
        assert!(why.contains("set aside"), "{why}");
        assert_eq!(pair.round(declared).expect("round").bytes, b"<probe/>");
        assert!(pair.ceiling().is_none());
        assert!(pair.unavailable().is_none());
    }

    #[test]
    fn the_far_end_is_this_participant_unsigned_and_keeps_the_path() {
        let pair = PeppolTransport::loopback();
        let arrived = pair.round(b"<Order/>").expect("round");
        assert!(
            arrived.origin_uri.starts_with("peppol://127.0.0.1:"),
            "{}",
            arrived.origin_uri
        );
        assert!(
            arrived
                .origin_uri
                .contains("/as4?sender=0088:7300010000001&receiver=0088:7300010000001&instance="),
            "{}",
            arrived.origin_uri
        );
        assert_eq!(pair.name(), "peppol");
        assert_eq!(pair.me().value, PARTICIPANT);
        assert_eq!(path_of("as4://127.0.0.1:0/as4"), "/as4");
        assert_eq!(path_of("https://ap.example/peppol/as4"), "/peppol/as4");
        assert_eq!(path_of("as4://127.0.0.1:0"), "/");
        assert_eq!(path_of("host:1/x"), "/x");
    }
}
