#![forbid(unsafe_code)]

//! Streams that arrive as Peppol business documents. One business document
//! is one Stream, its participants and its instance beside it.
//!
//! Peppol is the `OpenPeppol` network's four corners: a sender hands a
//! document to its access point, that access point posts it to the
//! receiver's, and the receiver's hands it on. Between the two access
//! points the wire is AS4 under one profile, and that is what is here: the
//! document goes inside a Standard Business Document whose header names the
//! sending and the receiving participant, the document type and the process
//! ([`Header`]); the AS4 User Message around it carries the access-point
//! party type, the transport infrastructure agreement, the process as its
//! service, the document type as its action, and the participants again as
//! the `originalSender` and `finalRecipient` properties. A Receive Location
//! is this participant's access point: it takes what is posted, refuses what
//! is not under the profile or not for this participant, receipts it, and
//! hands up the business document alone. A Send Location wraps and posts,
//! and the send is refused where the Receipt is.
//!
//! Where the receiver's access point is comes from a [`Directory`], a static
//! table, or from a URL given as the target. The network's own answer — the
//! SML naming the participant's SMP, the SMP naming the endpoint and its
//! certificate — is not written yet, so a participant the table does not
//! know is refused, by name. Three more things the profile asks for are not
//! here and are the as4 technology's to carry: the signature and the
//! encryption of WS-Security, which need a certificate and ride on its
//! [`Signer`]; the gzip compression of the payload part; and the `type`
//! attribute of a message property. Until a certificate names an access
//! point, the AS4 party id is the participant's own value.
//!
//! The origin URI carries what the header knew:
//! `peppol://peer/as4?sender=0088:1&receiver=0088:2&instance=1.2@xmip`.

mod loopback;
pub mod participant;
pub mod sbdh;

use std::net::TcpListener;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use as4::{As4Transport, Signer, Unsigned, UserMessage};
pub use loopback::PARTICIPANT;
pub use participant::{Directory, Participant};
pub use sbdh::{Header, Identifier};
use transport::error::{Result, protocol_error};
use transport::{Arrived, Directions, Transport};

/// The Peppol BIS Billing 3.0 invoice, the document type a transport carries
/// until [`PeppolTransport::carrying`].
pub const BILLING_INVOICE: &str = "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2::\
    Invoice##urn:cen.eu:en16931:2017#compliant#urn:fdc:peppol.eu:2017:poacc:billing:3.0::2.1";
/// The Peppol billing process, the one [`BILLING_INVOICE`] belongs to.
pub const BILLING_PROCESS: &str = "urn:fdc:peppol.eu:2017:poacc:billing:01:1.0";
/// The `type` both AS4 party ids carry: an access point.
pub const PARTY_TYPE: &str = "urn:fdc:peppol.eu:2017:identifiers:ap";
/// The agreement every Peppol exchange is under.
pub const AGREEMENT: &str = "urn:fdc:peppol.eu:2017:agreements:tia:ap_provider";
/// The message property naming the participant the document is from.
pub const ORIGINAL_SENDER: &str = "originalSender";
/// The message property naming the participant the document is for.
pub const FINAL_RECIPIENT: &str = "finalRecipient";

pub struct PeppolTransport {
    /// The partner's access point to send to, or the address to listen at.
    endpoint: String,
    me: Participant,
    partner: Participant,
    document: Identifier,
    process: Identifier,
    directory: Directory,
    signer: Arc<dyn Signer>,
    timeout: Option<Duration>,
    /// This participant's access point, made on the first accept and kept,
    /// so a document seen before stays seen.
    inbox: OnceLock<As4Transport>,
}

impl PeppolTransport {
    /// Speak as participant `me` to `partner`, whose access point is at
    /// `endpoint` — `https://ap.example/as4` or `as4://host:port/as4` —
    /// unless the [`Directory`] knows better; invoices under the billing
    /// process until [`Self::carrying`], unsigned until
    /// [`Self::signing_with`].
    #[must_use]
    pub fn new(endpoint: impl Into<String>, me: Participant, partner: Participant) -> Self {
        Self {
            endpoint: endpoint.into(),
            me,
            partner,
            document: Identifier::document(BILLING_INVOICE),
            process: Identifier::process(BILLING_PROCESS),
            directory: Directory::default(),
            signer: Arc::new(Unsigned),
            timeout: None,
            inbox: OnceLock::new(),
        }
    }

    /// Carry documents of type `document` under `process`.
    #[must_use]
    pub fn carrying(mut self, document: Identifier, process: Identifier) -> Self {
        self.document = document;
        self.process = process;
        self
    }

    /// Resolve a participant's access point from `directory`.
    #[must_use]
    pub fn resolving_from(mut self, directory: Directory) -> Self {
        self.directory = directory;
        self
    }

    /// Sign with this, and verify with it.
    #[must_use]
    pub fn signing_with(mut self, signer: impl Signer + 'static) -> Self {
        self.signer = Arc::new(signer);
        self
    }

    /// Give up on an access point that stops mid-message.
    #[must_use]
    pub const fn timing_out_after(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// The participant this transport speaks as.
    #[must_use]
    pub const fn me(&self) -> &Participant {
        &self.me
    }

    /// The participant this transport sends to.
    #[must_use]
    pub const fn partner(&self) -> &Participant {
        &self.partner
    }

    /// Bind at the endpoint's authority as the access point partners post
    /// to, and report the address actually assigned.
    ///
    /// # Errors
    /// Where the address is taken, malformed, or not permitted.
    pub fn bind(&self) -> Result<(TcpListener, String)> {
        self.inbox().bind()
    }

    /// Accept one Standard Business Document on an already-bound listener,
    /// receipt it and unwrap it; `None` where it was one seen before,
    /// receipted again and not delivered again.
    ///
    /// # Errors
    /// Where the connection broke, the message is not under the profile or
    /// not for this participant — each answered with the Error that says
    /// so — or what it carried is no Standard Business Document for this
    /// participant.
    pub fn accept_one(&self, listener: &TcpListener) -> Result<Option<Arrived>> {
        let Some((_, posted)) = self.inbox().accept_one(listener)? else {
            return Ok(None);
        };
        let (header, document) = Header::unwrap(&posted.bytes)?;
        if header.receiver != self.me {
            return Err(protocol_error(format!(
                "a business document for {}, and this participant is {}",
                header.receiver, self.me
            )));
        }
        let at = posted.origin_uri.strip_prefix("as4://").unwrap_or_default();
        let at = at.split('?').next().unwrap_or_default();
        let origin = format!(
            "peppol://{at}?sender={}&receiver={}&instance={}",
            header.sender.value, header.receiver.value, header.instance
        );
        Ok(Some(Arrived::new(origin, document)))
    }

    /// This participant's access point, checking the profile.
    fn inbox(&self) -> &As4Transport {
        self.inbox.get_or_init(|| {
            let me = self.me.clone();
            self.access_point(&self.endpoint, &self.me)
                .checking(move |message| under_the_profile(message, &me))
        })
    }

    /// The as4 transport that speaks the profile at `endpoint` to
    /// `receiver`'s access point.
    fn access_point(&self, endpoint: &str, receiver: &Participant) -> As4Transport {
        let mut template = UserMessage::new(
            &self.me.value,
            &receiver.value,
            &self.process.value,
            &self.document.to_string(),
        );
        template.party_type = Some(PARTY_TYPE.to_string());
        template.service_type = Some(self.process.scheme.clone());
        template.agreement = Some(AGREEMENT.to_string());
        template.properties = vec![
            (ORIGINAL_SENDER.to_string(), self.me.value.clone()),
            (FINAL_RECIPIENT.to_string(), receiver.value.clone()),
        ];
        template.payload_properties = vec![("MimeType".to_string(), "application/xml".to_string())];
        let transport = As4Transport::new(endpoint, &self.me.value, &receiver.value)
            .shaped(template)
            .signing_with(Shared(Arc::clone(&self.signer)));
        match self.timeout {
            Some(timeout) => transport.timing_out_after(timeout),
            None => transport,
        }
    }

    /// Who a target sends to and where: empty is the partner; a URL is the
    /// partner at that access point; anything else is a participant the
    /// directory must know.
    fn resolve(&self, target: &str) -> Result<(Participant, String)> {
        if target.contains("://") {
            return Ok((self.partner.clone(), target.to_string()));
        }
        if target.is_empty() {
            let endpoint = self
                .directory
                .endpoint_of(&self.partner)
                .unwrap_or(&self.endpoint);
            return Ok((self.partner.clone(), endpoint.to_string()));
        }
        let receiver = Participant::parse(target)?;
        let endpoint = self.directory.endpoint_of(&receiver).ok_or_else(|| {
            protocol_error(format!(
                "no access point known for {receiver}: the directory does not hold it, and SMP \
                 lookup is not written"
            ))
        })?;
        Ok((receiver.clone(), endpoint.to_string()))
    }
}

/// Whether `message` is a Peppol one for `me`: the access-point party type,
/// the agreement, and `me` as its final recipient.
fn under_the_profile(message: &UserMessage, me: &Participant) -> Result<()> {
    if message.party_type.as_deref() != Some(PARTY_TYPE) {
        return Err(protocol_error(format!(
            "a message whose parties are not Peppol access points: the party type is not \
             {PARTY_TYPE}"
        )));
    }
    if message.agreement.as_deref() != Some(AGREEMENT) {
        return Err(protocol_error(format!(
            "a message that is not under the Peppol agreement {AGREEMENT}"
        )));
    }
    let recipient = message
        .properties
        .iter()
        .find(|(name, _)| name == FINAL_RECIPIENT)
        .map(|(_, value)| value.as_str())
        .ok_or_else(|| protocol_error("a message with no finalRecipient property"))?;
    if Participant::parse(recipient)? != *me {
        return Err(protocol_error(format!(
            "a message whose final recipient is {recipient}, and this participant is {me}"
        )));
    }
    Ok(())
}

/// One signer held by the transport and lent to each as4 transport it
/// makes.
struct Shared(Arc<dyn Signer>);

impl Signer for Shared {
    fn sign(&self, envelope: String, attachments: &[(String, Vec<u8>)]) -> Result<String> {
        self.0.sign(envelope, attachments)
    }

    fn verify(&self, envelope: &str, attachments: &[(String, Vec<u8>)]) -> Result<()> {
        self.0.verify(envelope, attachments)
    }
}

impl Transport for PeppolTransport {
    fn name(&self) -> &'static str {
        "peppol"
    }

    fn directions(&self) -> Directions {
        Directions::BOTH
    }

    fn receive(&self) -> Result<Vec<Arrived>> {
        let (listener, _) = self.bind()?;
        Ok(self.accept_one(&listener)?.into_iter().collect())
    }

    fn send(&self, target: &str, bytes: &[u8]) -> Result<()> {
        let (receiver, endpoint) = self.resolve(target)?;
        let header = Header::new(&self.me, &receiver, &self.document, &self.process);
        let wrapped = header.wrap(bytes)?;
        self.access_point(&endpoint, &receiver).send("", &wrapped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn participant(value: &str) -> Participant {
        Participant::new(value).expect("a well-formed participant")
    }

    fn seller() -> (PeppolTransport, TcpListener, String) {
        let seller = PeppolTransport::new(
            "as4://127.0.0.1:0/as4",
            participant("0192:2"),
            participant("0088:1"),
        )
        .timing_out_after(secs(2));
        let (listener, address) = seller.bind().expect("binding");
        (seller, listener, address)
    }

    #[test]
    fn a_document_sent_to_a_participant_the_directory_knows_arrives_unwrapped() {
        let (seller, listener, address) = seller();
        let sender = std::thread::spawn(move || {
            let directory =
                Directory::default().with(participant("0192:2"), &format!("as4://{address}/as4"));
            PeppolTransport::new(
                "as4://127.0.0.1:1/nowhere",
                participant("0088:1"),
                participant("0192:9"),
            )
            .resolving_from(directory)
            .carrying(
                Identifier::document("urn:o::Order##c::2.1"),
                Identifier::process("urn:p"),
            )
            .timing_out_after(secs(2))
            .send("iso6523-actorid-upis::0192:2", b"<Order><ID>7</ID></Order>")
        });
        let arrived = seller
            .accept_one(&listener)
            .expect("accepted")
            .expect("new");
        sender.join().expect("thread").expect("receipted");
        assert_eq!(arrived.bytes, b"<Order><ID>7</ID></Order>");
        assert!(arrived.origin_uri.starts_with("peppol://127.0.0.1:"));
        assert!(
            arrived
                .origin_uri
                .contains("/as4?sender=0088:1&receiver=0192:2&instance=")
        );
        assert_eq!(seller.name(), "peppol");
        assert_eq!(seller.directions(), Directions::BOTH);
        assert!(seller.claims().is_none());
        assert_eq!(seller.partner().value, "0088:1");
    }

    #[test]
    fn a_participant_the_directory_does_not_know_is_refused_by_name() {
        let buyer = PeppolTransport::new(
            "as4://127.0.0.1:1/as4",
            participant("0088:1"),
            participant("0192:2"),
        );
        let error = buyer.send("0192:3", b"<Order/>").expect_err("unknown");
        assert!(!error.retryable);
        assert!(
            error.message.contains("iso6523-actorid-upis::0192:3"),
            "{error}"
        );
        assert!(
            error.message.contains("SMP lookup is not written"),
            "{error}"
        );
        assert!(buyer.send("not a participant", b"<Order/>").is_err());
        let error = buyer.send("", b"not a document").expect_err("no XML");
        assert!(
            error.message.contains("one XML business document"),
            "{error}"
        );
        let (partner, endpoint) = buyer.resolve("https://ap.example/as4").expect("a URL");
        assert_eq!(
            (partner.value.as_str(), endpoint.as_str()),
            ("0192:2", "https://ap.example/as4")
        );
        let (_, endpoint) = buyer.resolve("").expect("the partner");
        assert_eq!(endpoint, "as4://127.0.0.1:1/as4");
    }

    #[test]
    fn a_message_that_is_not_under_the_profile_is_answered_with_an_error() {
        let (seller, listener, address) = seller();
        let sender = std::thread::spawn(move || {
            As4Transport::new(format!("as4://{address}/as4"), "0088:1", "0192:2")
                .timing_out_after(secs(2))
                .send("", b"<Order/>")
        });
        let refused = seller.accept_one(&listener).expect_err("plain as4");
        assert!(
            refused.message.contains("not Peppol access points"),
            "{refused}"
        );
        let error = sender.join().expect("thread").expect_err("refused");
        assert!(error.message.contains("EBMS:0103"), "{error}");
        let mut message = UserMessage::new("a", "b", "s", "a");
        message.party_type = Some(PARTY_TYPE.to_string());
        let me = participant("0192:2");
        let why = |message: &UserMessage| under_the_profile(message, &me).expect_err("no").message;
        assert!(why(&message).contains("not under the Peppol agreement"));
        message.agreement = Some(AGREEMENT.to_string());
        assert!(why(&message).contains("no finalRecipient"));
        message.properties = vec![(FINAL_RECIPIENT.to_string(), "0192:3".to_string())];
        assert!(why(&message).contains("final recipient is 0192:3"));
        message.properties = vec![(FINAL_RECIPIENT.to_string(), "0192:2".to_string())];
        assert!(under_the_profile(&message, &me).is_ok());
    }

    #[test]
    fn a_document_whose_header_names_another_receiver_is_not_handed_up() {
        let (seller, listener, address) = seller();
        let sender = std::thread::spawn(move || {
            let buyer = PeppolTransport::new(
                format!("as4://{address}/as4"),
                participant("0088:1"),
                participant("0192:2"),
            )
            .timing_out_after(secs(2));
            let header = Header::new(
                buyer.me(),
                &participant("0192:3"),
                &Identifier::document(BILLING_INVOICE),
                &Identifier::process(BILLING_PROCESS),
            );
            let wrapped = header.wrap(b"<Invoice/>").expect("wrapped");
            buyer
                .access_point(&buyer.endpoint, buyer.partner())
                .send("", &wrapped)
        });
        let error = seller.accept_one(&listener).expect_err("another receiver");
        assert!(error.message.contains("a business document for"), "{error}");
        assert!(error.message.contains("0192:3"), "{error}");
        drop(sender.join().expect("thread"));
    }
}
