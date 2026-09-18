//! The Standard Business Document Header a Peppol business document
//! travels in: the sender and the receiver as participants, the document
//! identification, and the business scope naming the document type and
//! the process (UN/CEFACT SBDH 1.3, Peppol Envelope Specification 1.2).
//!
//! The header is the front of a `StandardBusinessDocument` whose one other
//! child is the business document itself — a UBL Invoice, an Order — so
//! what an access point posts is one XML document with the header first,
//! and what a Receive Location hands up is the document alone. Written by
//! hand and read by scanning for local names through the as4 technology's
//! reader, so a partner's prefix — none, `sh:`, `ns0:` — does not matter.
//!
//! A document type identifier is `busdox-docid-qns::{root namespace}::
//! {local name}##{customization}::{version}`, and the header's `Standard`,
//! `Type` and `TypeVersion` are read off it; a process identifier is
//! `cenbii-procid-ubl::{process}`.

use std::fmt;

use as4::UserMessage;
use as4::envelope::{attribute, element};
use transport::error::{Result, protocol_error};
use transport::xml::{escape, unescape};

use crate::participant::{Participant, SCHEME};

/// The SBDH namespace.
pub const NAMESPACE: &str = "http://www.unece.org/cefact/namespaces/StandardBusinessDocumentHeader";
/// The scheme a document type identifier is under.
pub const DOCUMENT_SCHEME: &str = "busdox-docid-qns";
/// The scheme a process identifier is under.
pub const PROCESS_SCHEME: &str = "cenbii-procid-ubl";

/// One Peppol identifier that is not a participant: a scheme, two colons,
/// a value — a document type or a process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identifier {
    pub scheme: String,
    pub value: String,
}

impl Identifier {
    /// `value` under `scheme`.
    #[must_use]
    pub fn new(scheme: &str, value: &str) -> Self {
        Self {
            scheme: scheme.to_string(),
            value: value.to_string(),
        }
    }

    /// The document type `value`, under [`DOCUMENT_SCHEME`].
    #[must_use]
    pub fn document(value: &str) -> Self {
        Self::new(DOCUMENT_SCHEME, value)
    }

    /// The process `value`, under [`PROCESS_SCHEME`].
    #[must_use]
    pub fn process(value: &str) -> Self {
        Self::new(PROCESS_SCHEME, value)
    }

    /// The identifier `text` is: `scheme::value`.
    ///
    /// # Errors
    /// Where there is no `::`, or nothing on one side of it.
    pub fn parse(text: &str) -> Result<Self> {
        match text.split_once("::") {
            Some((scheme, value)) if !scheme.is_empty() && !value.is_empty() => {
                Ok(Self::new(scheme, value))
            }
            _ => Err(protocol_error(format!(
                "an identifier that is not scheme::value: {text}"
            ))),
        }
    }

    /// The standard, the type and the version a document type identifier
    /// carries — `urn:…:Invoice-2`, `Invoice`, `2.1` — each empty where
    /// the value does not have it.
    #[must_use]
    pub fn parts(&self) -> (&str, &str, &str) {
        let Some((standard, rest)) = self.value.split_once("::") else {
            return (&self.value, "", "");
        };
        let kind = rest.split("##").next().unwrap_or_default();
        let version = rest.rsplit_once("::").map_or("", |(_, version)| version);
        (standard, kind, version)
    }
}

impl fmt::Display for Identifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}::{}", self.scheme, self.value)
    }
}

/// One header as it describes the document behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    pub sender: Participant,
    pub receiver: Participant,
    pub document: Identifier,
    pub process: Identifier,
    /// The instance identifier: no other document from this process
    /// carries it.
    pub instance: String,
    /// When the document was made, `xs:dateTime` in UTC.
    pub created: String,
}

impl Header {
    /// A header for a `document` of `process` from `sender` to `receiver`,
    /// made now.
    #[must_use]
    pub fn new(
        sender: &Participant,
        receiver: &Participant,
        document: &Identifier,
        process: &Identifier,
    ) -> Self {
        // The instance identifier and the creation time are the ones a
        // fresh User Message carries: the as4 technology keeps its id well
        // and its clock to itself, and one of each serves both headers.
        let stamp = UserMessage::new(&sender.value, &receiver.value, &process.value, "");
        Self {
            sender: sender.clone(),
            receiver: receiver.clone(),
            document: document.clone(),
            process: process.clone(),
            instance: stamp.message_id,
            created: stamp.timestamp,
        }
    }

    /// The Standard Business Document: this header, then `payload` as the
    /// business document, its XML declaration and the whitespace around it
    /// set aside.
    ///
    /// # Errors
    /// Where `payload` is not one XML element.
    pub fn wrap(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let document = business_document(payload)?;
        let (standard, kind, version) = self.document.parts();
        Ok(format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <StandardBusinessDocument xmlns=\"{NAMESPACE}\"><StandardBusinessDocumentHeader>\
             <HeaderVersion>1.0</HeaderVersion>{}{}<DocumentIdentification>\
             <Standard>{}</Standard><TypeVersion>{}</TypeVersion>\
             <InstanceIdentifier>{}</InstanceIdentifier><Type>{}</Type>\
             <CreationDateAndTime>{}</CreationDateAndTime></DocumentIdentification>\
             <BusinessScope>{}{}</BusinessScope></StandardBusinessDocumentHeader>\
             {document}</StandardBusinessDocument>",
            party("Sender", &self.sender),
            party("Receiver", &self.receiver),
            escape(standard),
            escape(version),
            escape(&self.instance),
            escape(kind),
            escape(&self.created),
            scope("DOCUMENTID", &self.document),
            scope("PROCESSID", &self.process),
        )
        .into_bytes())
    }

    /// The header a Standard Business Document carries and the business
    /// document behind it, whitespace around it trimmed.
    ///
    /// # Errors
    /// Where `bytes` are not UTF-8, hold no Standard Business Document,
    /// one without its header, a header without its participants or its
    /// scopes, or nothing after the header.
    pub fn unwrap(bytes: &[u8]) -> Result<(Self, Vec<u8>)> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| protocol_error("a Standard Business Document that is not UTF-8"))?;
        let root = element(text, "StandardBusinessDocument")
            .ok_or_else(|| protocol_error("no StandardBusinessDocument in what arrived"))?;
        let header = element(root, "StandardBusinessDocumentHeader")
            .ok_or_else(|| protocol_error("a StandardBusinessDocument with no header"))?;
        let after_header = offset(root, header) + header.len();
        let closing = root[after_header..]
            .find('>')
            .ok_or_else(|| protocol_error("a header that never closes"))?;
        let document = root[after_header + closing + 1..].trim();
        if document.is_empty() {
            return Err(protocol_error(
                "a StandardBusinessDocument with no business document after its header",
            ));
        }
        let (mut document_type, mut process) = (None, None);
        for (kind, identifier) in scopes(element(header, "BusinessScope").unwrap_or_default()) {
            match kind.as_str() {
                "DOCUMENTID" => document_type = Some(identifier),
                "PROCESSID" => process = Some(identifier),
                _ => {}
            }
        }
        let identification = element(header, "DocumentIdentification").unwrap_or_default();
        Ok((
            Self {
                sender: participant(header, "Sender")?,
                receiver: participant(header, "Receiver")?,
                document: document_type
                    .ok_or_else(|| protocol_error("a header with no DOCUMENTID scope"))?,
                process: process
                    .ok_or_else(|| protocol_error("a header with no PROCESSID scope"))?,
                instance: text_of(identification, "InstanceIdentifier"),
                created: text_of(identification, "CreationDateAndTime"),
            },
            document.as_bytes().to_vec(),
        ))
    }
}

/// The one XML element `payload` is — its XML declaration, a byte order
/// mark and the whitespace around it set aside — as it goes inside the
/// Standard Business Document.
///
/// # Errors
/// Where `payload` is not UTF-8, or is not one element: empty, text, a
/// processing instruction, a doctype.
pub fn business_document(payload: &[u8]) -> Result<&str> {
    let text = std::str::from_utf8(payload).map_err(|_| {
        protocol_error("a Peppol payload is one XML business document, and these bytes are not UTF-8")
    })?;
    let mut document = text.trim_start_matches('\u{feff}').trim();
    if document.starts_with("<?xml") {
        let end = document
            .find("?>")
            .ok_or_else(|| protocol_error("an XML declaration that never closes"))?;
        document = document[end + 2..].trim_start();
    }
    let is_element = document.len() > 2
        && document.starts_with('<')
        && !document.starts_with("<?")
        && !document.starts_with("<!")
        && document.ends_with('>');
    if !is_element {
        return Err(protocol_error(
            "a Peppol payload is one XML business document, and this is not one",
        ));
    }
    Ok(document)
}

/// `<Sender>` or `<Receiver>` naming `participant` under its authority.
fn party(side: &str, participant: &Participant) -> String {
    format!(
        "<{side}><Identifier Authority=\"{SCHEME}\">{}</Identifier></{side}>",
        escape(&participant.value)
    )
}

/// One `<Scope>` of `kind` naming `identifier`.
fn scope(kind: &str, identifier: &Identifier) -> String {
    format!(
        "<Scope><Type>{kind}</Type><InstanceIdentifier>{}</InstanceIdentifier>\
         <Identifier>{}</Identifier></Scope>",
        escape(&identifier.value),
        escape(&identifier.scheme)
    )
}

/// The participant a `Sender` or `Receiver` element names.
fn participant(header: &str, side: &str) -> Result<Participant> {
    let party =
        element(header, side).ok_or_else(|| protocol_error(format!("a header with no {side}")))?;
    let authority = attribute(party, "Identifier", "Authority").unwrap_or_else(|| SCHEME.to_string());
    if authority != SCHEME {
        return Err(protocol_error(format!(
            "a {side} under {authority}, and Peppol participants are {SCHEME}"
        )));
    }
    let value = element(party, "Identifier")
        .map(unescape)
        .ok_or_else(|| protocol_error(format!("a {side} with no Identifier")))?;
    Participant::new(value.trim())
}

/// Every `Scope` in `xml`: its `Type` and the identifier it names, the
/// scheme from `Identifier` where one is given.
fn scopes(xml: &str) -> Vec<(String, Identifier)> {
    let mut found = Vec::new();
    let mut rest = xml;
    while let Some(scope) = element(rest, "Scope") {
        let kind = text_of(scope, "Type");
        let value = text_of(scope, "InstanceIdentifier");
        let scheme = text_of(scope, "Identifier");
        let scheme = if scheme.is_empty() {
            match kind.as_str() {
                "PROCESSID" => PROCESS_SCHEME,
                _ => DOCUMENT_SCHEME,
            }
        } else {
            scheme.as_str()
        };
        found.push((kind, Identifier::new(scheme, &value)));
        rest = &rest[offset(rest, scope) + scope.len()..];
    }
    found
}

/// The text of the first `name` in `xml`, trimmed; empty where none.
fn text_of(xml: &str, name: &str) -> String {
    element(xml, name)
        .map(|text| unescape(text.trim()))
        .unwrap_or_default()
}

/// Where `part`, a slice of `whole`, begins in it.
fn offset(whole: &str, part: &str) -> usize {
    part.as_ptr() as usize - whole.as_ptr() as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header() -> Header {
        Header::new(
            &Participant::new("0088:1").expect("sender"),
            &Participant::new("0192:2").expect("receiver"),
            &Identifier::document(crate::BILLING_INVOICE),
            &Identifier::process(crate::BILLING_PROCESS),
        )
    }

    #[test]
    fn a_business_document_reads_back_off_the_standard_business_document_it_went_in() {
        let header = header();
        let invoice = b"<Invoice xmlns=\"urn:x\"><ID>A &amp; B</ID></Invoice>";
        let wrapped = header.wrap(invoice).expect("wrapped");
        let text = String::from_utf8(wrapped.clone()).expect("utf-8");
        assert!(text.contains("<Sender><Identifier Authority=\"iso6523-actorid-upis\">0088:1"));
        assert!(text.contains("<Standard>urn:oasis:names:specification:ubl:schema:xsd:Invoice-2"));
        assert!(text.contains("<TypeVersion>2.1</TypeVersion>"));
        assert!(text.contains("<Type>Invoice</Type>"));
        assert!(text.contains("<Type>PROCESSID</Type><InstanceIdentifier>urn:fdc:peppol.eu"));
        assert!(text.ends_with("</StandardBusinessDocumentHeader><Invoice xmlns=\"urn:x\">\
            <ID>A &amp; B</ID></Invoice></StandardBusinessDocument>"));
        let (read, document) = Header::unwrap(&wrapped).expect("unwrapped");
        assert_eq!(read, header);
        assert_eq!(document, invoice);
        assert!(header.instance.ends_with("@xmip"));
        assert_eq!(header.created.len(), 20);
        assert_ne!(header().instance, header.instance);
    }

    #[test]
    fn a_partner_header_with_a_prefix_and_indentation_is_read_and_a_hollow_one_refused() {
        let theirs = "<?xml version=\"1.0\"?>\n<sh:StandardBusinessDocument xmlns:sh=\"x\">\n  \
            <sh:StandardBusinessDocumentHeader>\n    <sh:HeaderVersion>1.0</sh:HeaderVersion>\n    \
            <sh:Sender><sh:Identifier Authority=\"iso6523-actorid-upis\">0007:5560001234\
            </sh:Identifier></sh:Sender>\n    <sh:Receiver><sh:Identifier>0088:2</sh:Identifier>\
            </sh:Receiver>\n    <sh:DocumentIdentification><sh:Standard>urn:x</sh:Standard>\
            <sh:InstanceIdentifier>ab-12</sh:InstanceIdentifier><sh:CreationDateAndTime>\
            2026-09-16T10:00:00Z</sh:CreationDateAndTime></sh:DocumentIdentification>\n    \
            <sh:BusinessScope><sh:Scope><sh:Type>PROCESSID</sh:Type><sh:InstanceIdentifier>\
            urn:p</sh:InstanceIdentifier></sh:Scope><sh:Scope><sh:Type>DOCUMENTID</sh:Type>\
            <sh:InstanceIdentifier>urn:d::D##c::2.1</sh:InstanceIdentifier><sh:Identifier>\
            busdox-docid-qns</sh:Identifier></sh:Scope></sh:BusinessScope>\n  \
            </sh:StandardBusinessDocumentHeader>\n  <Order xmlns=\"urn:o\">\n    <ID>7</ID>\n  \
            </Order>\n</sh:StandardBusinessDocument>\n";
        let (header, document) = Header::unwrap(theirs.as_bytes()).expect("read");
        assert_eq!(header.sender.value, "0007:5560001234");
        assert_eq!(header.receiver.value, "0088:2");
        assert_eq!(header.instance, "ab-12");
        assert_eq!(header.created, "2026-09-16T10:00:00Z");
        assert_eq!(header.process, Identifier::process("urn:p"));
        assert_eq!(header.document, Identifier::document("urn:d::D##c::2.1"));
        assert_eq!(document, b"<Order xmlns=\"urn:o\">\n    <ID>7</ID>\n  </Order>");
        let refused = |sbd: &str| Header::unwrap(sbd.as_bytes()).expect_err(sbd).message;
        assert!(refused("<Order/>").contains("no StandardBusinessDocument"));
        assert!(refused("<StandardBusinessDocument><Order/></StandardBusinessDocument>")
            .contains("no header"));
        assert!(refused(&theirs.replace("<Order xmlns=\"urn:o\">\n    <ID>7</ID>\n  </Order>", ""))
            .contains("no business document after"));
        assert!(refused(&theirs.replace("<sh:Type>DOCUMENTID</sh:Type>", "")).contains("DOCUMENTID"));
        assert!(refused(&theirs.replace("<sh:Type>PROCESSID</sh:Type>", "")).contains("PROCESSID"));
        assert!(refused(&theirs.replace("iso6523-actorid-upis", "gln")).contains("under gln"));
        assert!(refused(&theirs.replace("<sh:Receiver>", "<sh:Receiver/>")).contains("no Receiver"));
        assert!(refused(&theirs.replace("0088:2", "2")).contains("no ICD"));
        assert!(Header::unwrap(&[0xff, 0xfe]).is_err());
    }

    #[test]
    fn a_declaration_is_set_aside_and_bytes_that_are_no_document_are_refused() {
        let declared = "\u{feff}<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<probe/>\n";
        assert_eq!(business_document(declared.as_bytes()).expect("document"), "<probe/>");
        assert_eq!(business_document(b"<a><b/></a>").expect("document"), "<a><b/></a>");
        for (name, bytes) in transport::payload::edge_payloads() {
            let error = business_document(&bytes).expect_err(name);
            assert!(error.message.starts_with("a Peppol payload is one XML"), "{name}");
        }
        assert!(business_document(b"<!doctype html><p>x</p>").is_err());
        assert!(business_document(b"<?xml version=\"1.0\"?><?pi?>").is_err());
        assert!(business_document(b"<?xml version=\"1.0\"").is_err());
        assert!(business_document(b"<a>").is_err());
        assert!(business_document(b"<a></a>x").is_err());
        let invoice = Identifier::parse(&format!("busdox-docid-qns::{}", crate::BILLING_INVOICE))
            .expect("parsed");
        assert_eq!(invoice, Identifier::document(crate::BILLING_INVOICE));
        assert_eq!(
            invoice.parts(),
            (
                "urn:oasis:names:specification:ubl:schema:xsd:Invoice-2",
                "Invoice",
                "2.1"
            )
        );
        assert_eq!(Identifier::process("urn:p").parts(), ("urn:p", "", ""));
        assert_eq!(Identifier::process("urn:p").to_string(), "cenbii-procid-ubl::urn:p");
        assert!(Identifier::parse("no-colons").is_err());
        assert!(Identifier::parse("::value").is_err());
        assert!(Identifier::parse("scheme::").is_err());
    }
}
